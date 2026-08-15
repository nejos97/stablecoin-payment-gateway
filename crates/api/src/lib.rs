use std::collections::HashMap;
use std::time::Duration as StdDuration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use db::{DepositAddressRow, WebhookDeliveryRow};
use domain::{DepositAddressStatus, Network, Token, WebhookDeliveryStatus};
use jobs::{get_cached_wallet_balances, get_live_balances, retrigger_webhook, AppState, RetriggerError};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tower_http::trace::TraceLayer;

const READINESS_TIMEOUT: StdDuration = StdDuration::from_secs(2);

pub fn router(state: AppState) -> Router {
    let api_v1 = Router::new()
        .route(
            "/deposit-addresses",
            post(create_deposit_address).get(list_deposit_addresses),
        )
        .route("/deposit-addresses/{id}", get(get_deposit_address))
        .route(
            "/deposits/{deposit_id}/webhooks/retry",
            post(retry_deposit_webhook),
        )
        .route("/balances", get(get_balances))
        .route("/wallet-balances", get(get_wallet_balances));

    Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .nest("/api/v1", api_v1)
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz() -> Json<Value> {
    Json(json!({ "status": "OK" }))
}

async fn readyz(State(state): State<AppState>) -> impl IntoResponse {
    let (postgres, redis) = tokio::join!(check_postgres(&state), check_redis(&state));

    let ready = postgres == "ok" && redis == "ok";
    let status = if ready {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        status,
        Json(json!({
            "status": if ready { "ready" } else { "not_ready" },
            "checks": {
                "postgres": postgres,
                "redis": redis,
            }
        })),
    )
}

async fn check_postgres(state: &AppState) -> String {
    match tokio::time::timeout(READINESS_TIMEOUT, state.db.ping()).await {
        Ok(Ok(())) => "ok".into(),
        Ok(Err(e)) => format!("error: {e:#}"),
        Err(_) => "timeout".into(),
    }
}

async fn check_redis(state: &AppState) -> String {
    let mut redis = state.redis.clone();
    match tokio::time::timeout(
        READINESS_TIMEOUT,
        redis::cmd("PING").query_async::<String>(&mut redis),
    )
    .await
    {
        Ok(Ok(_)) => "ok".into(),
        Ok(Err(e)) => format!("error: {e}"),
        Err(_) => "timeout".into(),
    }
}

async fn create_deposit_address(
    State(state): State<AppState>,
    Json(body): Json<CreateDepositAddressRequest>,
) -> Result<(StatusCode, Json<DepositAddressResponse>), ApiError> {
    let network = Network::parse_api(&body.network).map_err(ApiError::from)?;
    let token = Token::parse_api(&body.token).map_err(ApiError::from)?;
    let expected: Decimal = body
        .expected_amount
        .parse()
        .map_err(|_| ApiError::bad_request("Invalid expected_amount"))?;

    let expires_at = if let Some(exp) = body.expires_at {
        chrono::DateTime::parse_from_rfc3339(&exp)
            .map(|d| d.with_timezone(&Utc))
            .map_err(|_| ApiError::bad_request("Invalid expires_at"))?
    } else {
        Utc::now() + Duration::minutes(state.config.deposit_expiry_minutes as i64)
    };

    let derivation_index = state.db.next_derivation_index(network).await?;
    let address = state
        .wallet
        .derive_address(network, derivation_index as u32)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    let metadata = body.metadata.unwrap_or_else(|| json!({}));
    let row = state
        .db
        .create_deposit_address(
            network,
            token,
            &address,
            derivation_index,
            expected,
            expires_at,
            metadata,
        )
        .await?;

    Ok((StatusCode::CREATED, Json(to_response(&row)?)))
}

async fn list_deposit_addresses(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, ApiError> {
    let _ = state.db.expire_stale_addresses().await?;

    let status = query
        .status
        .as_deref()
        .map(DepositAddressStatus::parse_api)
        .transpose()
        .map_err(ApiError::from)?;
    let network = query
        .network
        .as_deref()
        .map(Network::parse_api)
        .transpose()
        .map_err(ApiError::from)?;

    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let (rows, total) = state
        .db
        .list_deposit_addresses(status, network, limit, offset)
        .await?;
    let data = rows
        .iter()
        .map(to_response)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(ListResponse {
        data,
        total,
        limit,
        offset,
    }))
}

async fn get_deposit_address(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let Some(row) = state.db.find_deposit_address(&id).await? else {
        return Err(ApiError::not_found(format!(
            "Deposit address not found: {id}"
        )));
    };
    let deposits = state.db.list_deposits_for_address(&id).await?;
    let deposit_ids: Vec<String> = deposits.iter().map(|d| d.id.clone()).collect();
    // Rows come back newest-first; keep the first one per deposit (the
    // authoritative delivery, same convention as the jobs crate).
    let mut deliveries: HashMap<String, WebhookDeliveryRow> = HashMap::new();
    for delivery in state
        .db
        .find_webhook_deliveries_for_deposits(&deposit_ids)
        .await?
    {
        deliveries
            .entry(delivery.deposit_id.clone())
            .or_insert(delivery);
    }

    let mut base = serde_json::to_value(to_response(&row)?)?;
    if let Some(obj) = base.as_object_mut() {
        obj.insert("metadata".into(), row.metadata);
        obj.insert(
            "deposits".into(),
            json!(deposits
                .into_iter()
                .map(|d| {
                    let webhook = deliveries
                        .get(&d.id)
                        .map(webhook_to_json)
                        .unwrap_or(Value::Null);
                    json!({
                        "id": d.id,
                        "tx_hash": d.tx_hash,
                        "amount": d.amount.to_string(),
                        "amount_raw": d.amount_raw,
                        "confirmations": d.confirmations,
                        "status": domain::DepositStatus::from_db(&d.status).map(|s| s.as_api()).unwrap_or("detected"),
                        "confirmed_at": d.confirmed_at.map(|t| t.to_rfc3339()),
                        "webhook": webhook,
                    })
                })
                .collect::<Vec<_>>()),
        );
    }
    Ok(Json(base))
}

fn webhook_to_json(delivery: &WebhookDeliveryRow) -> Value {
    json!({
        "status": WebhookDeliveryStatus::from_db(&delivery.status).map(|s| s.as_api()).unwrap_or("pending"),
        "attempts": delivery.attempts,
        "last_response": delivery.last_response,
        "last_error": delivery.last_error,
        "updated_at": delivery.updated_at.to_rfc3339(),
    })
}

async fn retry_deposit_webhook(
    State(state): State<AppState>,
    Path(deposit_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match retrigger_webhook(&state, &deposit_id).await {
        Ok(()) => Ok(Json(json!({
            "status": "queued",
            "deposit_id": deposit_id,
        }))),
        Err(RetriggerError::DepositNotFound) => Err(ApiError::not_found(format!(
            "Deposit not found: {deposit_id}"
        ))),
        Err(RetriggerError::DepositNotConfirmed) => {
            Err(ApiError::bad_request("Deposit is not confirmed"))
        }
        Err(RetriggerError::WebhooksDisabled) => Err(ApiError::service_unavailable(
            "WEBHOOK_CALLBACK_URL is not set".into(),
        )),
        Err(RetriggerError::Other(err)) => Err(ApiError::internal(err.to_string())),
    }
}

async fn get_balances(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let resp = get_live_balances(&state).await?;
    Ok(Json(serde_json::to_value(resp)?))
}

async fn get_wallet_balances(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let resp = get_cached_wallet_balances(&state).await?;
    Ok(Json(serde_json::to_value(resp)?))
}

fn to_response(row: &DepositAddressRow) -> Result<DepositAddressResponse, ApiError> {
    let network = Network::from_db(&row.network).map_err(ApiError::from)?;
    let status = DepositAddressStatus::from_db(&row.status).map_err(ApiError::from)?;
    Ok(DepositAddressResponse {
        id: row.id.clone(),
        address: row.address.clone(),
        network: network.as_api().to_string(),
        token: row.token.clone(),
        status: status.as_api().to_string(),
        expected_amount: row.expected_amount.normalize().to_string(),
        expires_at: row.expires_at.to_rfc3339(),
        created_at: row.created_at.to_rfc3339(),
    })
}

#[derive(Debug, Deserialize)]
struct CreateDepositAddressRequest {
    network: String,
    token: String,
    expected_amount: String,
    expires_at: Option<String>,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct ListQuery {
    status: Option<String>,
    network: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Serialize)]
struct DepositAddressResponse {
    id: String,
    address: String,
    network: String,
    token: String,
    status: String,
    expected_amount: String,
    expires_at: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct ListResponse {
    data: Vec<DepositAddressResponse>,
    total: i64,
    limit: i64,
    offset: i64,
}

#[derive(Debug)]
pub struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn not_found(message: String) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message,
        }
    }
    fn bad_request(message: &str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }
    fn internal(message: String) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message,
        }
    }
    fn service_unavailable(message: String) -> Self {
        Self {
            status: StatusCode::SERVICE_UNAVAILABLE,
            message,
        }
    }
}

impl From<domain::DomainError> for ApiError {
    fn from(value: domain::DomainError) -> Self {
        Self::not_found(value.to_string())
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        Self::internal(value.to_string())
    }
}

impl From<serde_json::Error> for ApiError {
    fn from(value: serde_json::Error) -> Self {
        Self::internal(value.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = Json(json!({
            "statusCode": self.status.as_u16(),
            "message": self.message,
        }));
        (self.status, body).into_response()
    }
}
