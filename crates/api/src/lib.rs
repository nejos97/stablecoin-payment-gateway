use std::time::Duration as StdDuration;

use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{Duration, Utc};
use config::AppConfig;
use db::DepositAddressRow;
use domain::{DepositAddressStatus, Network, Token};
use jobs::{get_cached_wallet_balances, get_live_balances, AppState};
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
    headers: HeaderMap,
    Json(body): Json<CreateDepositAddressRequest>,
) -> Result<(StatusCode, Json<DepositAddressResponse>), ApiError> {
    require_secret(&headers, &state.config)?;

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
    headers: HeaderMap,
    Query(query): Query<ListQuery>,
) -> Result<Json<ListResponse>, ApiError> {
    require_secret(&headers, &state.config)?;
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
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    require_secret(&headers, &state.config)?;
    let Some(row) = state.db.find_deposit_address(&id).await? else {
        return Err(ApiError::not_found(format!(
            "Deposit address not found: {id}"
        )));
    };
    let deposits = state.db.list_deposits_for_address(&id).await?;
    let mut base = serde_json::to_value(to_response(&row)?)?;
    if let Some(obj) = base.as_object_mut() {
        obj.insert("metadata".into(), row.metadata);
        obj.insert(
            "deposits".into(),
            json!(deposits
                .into_iter()
                .map(|d| {
                    json!({
                        "id": d.id,
                        "tx_hash": d.tx_hash,
                        "amount": d.amount.to_string(),
                        "amount_raw": d.amount_raw,
                        "confirmations": d.confirmations,
                        "status": domain::DepositStatus::from_db(&d.status).map(|s| s.as_api()).unwrap_or("detected"),
                        "confirmed_at": d.confirmed_at.map(|t| t.to_rfc3339()),
                    })
                })
                .collect::<Vec<_>>()),
        );
    }
    Ok(Json(base))
}

async fn get_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    require_secret(&headers, &state.config)?;
    let resp = get_live_balances(&state).await?;
    Ok(Json(serde_json::to_value(resp)?))
}

async fn get_wallet_balances(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Value>, ApiError> {
    require_secret(&headers, &state.config)?;
    let resp = get_cached_wallet_balances(&state).await?;
    Ok(Json(serde_json::to_value(resp)?))
}

fn require_secret(headers: &HeaderMap, config: &AppConfig) -> Result<(), ApiError> {
    let Some(expected) = config.api_secret.as_deref() else {
        // Auth disabled when API_SECRET is unset / empty
        return Ok(());
    };

    let provided = headers
        .get("x-api-secret")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !secrets_match(provided, expected) {
        return Err(ApiError::unauthorized(
            "Invalid or missing API secret".into(),
        ));
    }
    Ok(())
}

fn secrets_match(provided: &str, expected: &str) -> bool {
    use std::cmp;
    let a = provided.as_bytes();
    let b = expected.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    // touch lengths to avoid trivial timing on empty
    let _ = cmp::max(a.len(), b.len());
    diff == 0
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
    fn unauthorized(message: String) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message,
        }
    }
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
