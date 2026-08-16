use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use chrono::Utc;
use axum::routing::{delete, get, patch, post};
use axum::{Extension, Json, Router};
use db::StaffUserRow;
use domain::{StaffRole, WebhookDeliveryStatus};
use jobs::{retrigger_failed_webhooks, AppState, RetriggerError};
use redis::AsyncCommands;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::middleware::{require_admin, require_jwt, CurrentStaff};
use crate::{
    create_deposit_address, get_balances, get_deposit_address, get_wallet_balances,
    list_deposit_addresses, retry_deposit_webhook, ApiError, ListQuery,
};

const MIN_PASSWORD_LEN: usize = 8;

/// Everything the dashboard calls lives under `/api/admin/v1`. Only
/// setup/login/refresh are public; the rest requires the acting staff's JWT,
/// and staff/API-key management additionally requires the ADMIN role.
pub fn admin_router(state: &AppState) -> Router<AppState> {
    let public = Router::new()
        .route("/setup", get(get_setup).post(post_setup))
        .route("/auth/login", post(login))
        .route("/auth/refresh", post(refresh));

    let admin_only = Router::new()
        .route("/staff", get(list_staff).post(create_staff))
        .route("/staff/{id}", patch(update_staff))
        .route("/api-keys", get(list_api_keys).post(create_api_key))
        .route("/api-keys/{id}", delete(revoke_api_key))
        .route("/settings", get(get_settings).patch(update_settings))
        .route(
            "/webhook-endpoints",
            get(list_webhook_endpoints).post(create_webhook_endpoint),
        )
        .route(
            "/webhook-endpoints/{id}",
            patch(update_webhook_endpoint).delete(delete_webhook_endpoint),
        )
        .route_layer(axum::middleware::from_fn(require_admin));

    let authed = Router::new()
        .route("/auth/logout", post(logout))
        .route("/auth/me", get(me))
        .route("/webhooks", get(list_webhooks))
        .route("/webhooks/retry-failed", post(retry_failed))
        .route("/stats", get(stats))
        .route("/stats/deposits-timeseries", get(deposits_timeseries))
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
        .route("/wallet-balances", get(get_wallet_balances))
        .merge(admin_only)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            require_jwt,
        ));

    public.merge(authed)
}

async fn get_setup(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let count = state.db.count_staff().await?;
    Ok(Json(json!({ "needs_setup": count == 0 })))
}

#[derive(Debug, Deserialize)]
struct SetupRequest {
    email: String,
    password: String,
    first_name: String,
    last_name: String,
}

async fn post_setup(
    State(state): State<AppState>,
    Json(body): Json<SetupRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let email = validate_email(&body.email)?;
    let (first_name, last_name) = validate_names(&body.first_name, &body.last_name)?;
    validate_password(&body.password)?;

    let password_hash =
        auth::hash_password(&body.password).map_err(|e| ApiError::internal(e.to_string()))?;
    let Some(staff) = state
        .db
        .create_first_admin(&email, &first_name, &last_name, &password_hash)
        .await?
    else {
        return Err(ApiError::conflict("Setup already completed"));
    };

    let tokens = issue_token_pair(&state, &staff).await?;
    Ok((StatusCode::CREATED, Json(tokens)))
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<Value>, ApiError> {
    let staff = state.db.find_staff_by_email(body.email.trim()).await?;
    // Verify against a real hash whenever possible so the timing of the
    // response does not reveal whether the email exists.
    let Some(staff) = staff else {
        let _ = auth::verify_password(&body.password, "");
        return Err(ApiError::unauthorized("Invalid email or password"));
    };
    if !auth::verify_password(&body.password, &staff.password_hash) {
        return Err(ApiError::unauthorized("Invalid email or password"));
    }
    if !staff.is_active {
        return Err(ApiError::unauthorized("Account is deactivated"));
    }

    let tokens = issue_token_pair(&state, &staff).await?;
    Ok(Json(tokens))
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

async fn refresh(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<Value>, ApiError> {
    let hash = auth::sha256_hex(&body.refresh_token);
    let key = state.config.redis_key(&format!("auth:refresh:{hash}"));
    let mut redis = state.redis.clone();

    let staff_id: Option<String> = redis.get(&key).await?;
    let Some(staff_id) = staff_id else {
        return Err(ApiError::unauthorized("Invalid refresh token"));
    };
    // Rotation: the presented token is consumed no matter what happens next.
    let _: () = redis.del(&key).await?;

    let Some(staff) = state.db.find_staff_by_id(&staff_id).await? else {
        return Err(ApiError::unauthorized("Unknown staff user"));
    };
    if !staff.is_active {
        return Err(ApiError::unauthorized("Account is deactivated"));
    }

    let tokens = issue_token_pair(&state, &staff).await?;
    Ok(Json(tokens))
}

async fn logout(
    State(state): State<AppState>,
    Json(body): Json<RefreshRequest>,
) -> Result<Json<Value>, ApiError> {
    let hash = auth::sha256_hex(&body.refresh_token);
    let key = state.config.redis_key(&format!("auth:refresh:{hash}"));
    let mut redis = state.redis.clone();
    let _: () = redis.del(&key).await?;
    Ok(Json(json!({ "status": "ok" })))
}

async fn me(Extension(staff): Extension<CurrentStaff>, State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let Some(row) = state.db.find_staff_by_id(&staff.id).await? else {
        return Err(ApiError::unauthorized("Unknown staff user"));
    };
    Ok(Json(staff_to_json(&row)))
}

async fn list_staff(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);
    let (rows, total) = state.db.list_staff(limit, offset).await?;
    Ok(Json(json!({
        "data": rows.iter().map(staff_to_json).collect::<Vec<_>>(),
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

#[derive(Debug, Deserialize)]
struct CreateStaffRequest {
    email: String,
    password: String,
    first_name: String,
    last_name: String,
    role: String,
}

async fn create_staff(
    State(state): State<AppState>,
    Json(body): Json<CreateStaffRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let email = validate_email(&body.email)?;
    let (first_name, last_name) = validate_names(&body.first_name, &body.last_name)?;
    validate_password(&body.password)?;
    let role = StaffRole::parse_api(&body.role)
        .map_err(|_| ApiError::bad_request("Invalid role: expected admin or operator"))?;

    if state.db.find_staff_by_email(&email).await?.is_some() {
        return Err(ApiError::conflict("A staff user with this email already exists"));
    }

    let password_hash =
        auth::hash_password(&body.password).map_err(|e| ApiError::internal(e.to_string()))?;
    let row = state
        .db
        .create_staff_user(&email, &first_name, &last_name, &password_hash, role)
        .await?;
    Ok((StatusCode::CREATED, Json(staff_to_json(&row))))
}

#[derive(Debug, Deserialize)]
struct UpdateStaffRequest {
    first_name: Option<String>,
    last_name: Option<String>,
    role: Option<String>,
    is_active: Option<bool>,
    password: Option<String>,
}

async fn update_staff(
    State(state): State<AppState>,
    Extension(current): Extension<CurrentStaff>,
    Path(id): Path<String>,
    Json(body): Json<UpdateStaffRequest>,
) -> Result<Json<Value>, ApiError> {
    let Some(target) = state.db.find_staff_by_id(&id).await? else {
        return Err(ApiError::not_found(format!("Staff user not found: {id}")));
    };

    let role = body
        .role
        .as_deref()
        .map(StaffRole::parse_api)
        .transpose()
        .map_err(|_| ApiError::bad_request("Invalid role: expected admin or operator"))?;

    if target.id == current.id && body.is_active == Some(false) {
        return Err(ApiError::bad_request("You cannot deactivate your own account"));
    }

    // Never let the instance end up without an active admin.
    let target_is_active_admin = target.role == StaffRole::Admin.as_db() && target.is_active;
    let loses_admin = role == Some(StaffRole::Operator) || body.is_active == Some(false);
    if target_is_active_admin && loses_admin && state.db.count_active_admins().await? <= 1 {
        return Err(ApiError::conflict(
            "Cannot demote or deactivate the last active admin",
        ));
    }

    let password_hash = match body.password.as_deref() {
        Some(password) => {
            validate_password(password)?;
            Some(auth::hash_password(password).map_err(|e| ApiError::internal(e.to_string()))?)
        }
        None => None,
    };

    let (first_name, last_name) = (
        body.first_name.as_deref().map(str::trim),
        body.last_name.as_deref().map(str::trim),
    );
    if first_name == Some("") || last_name == Some("") {
        return Err(ApiError::bad_request("Names cannot be empty"));
    }

    let Some(row) = state
        .db
        .update_staff(
            &id,
            first_name,
            last_name,
            role,
            body.is_active,
            password_hash.as_deref(),
        )
        .await?
    else {
        return Err(ApiError::not_found(format!("Staff user not found: {id}")));
    };

    // Blocking must be immediate: besides `require_jwt` rejecting inactive
    // accounts, drop every stored refresh session of the blocked staff so a
    // saved refresh token cannot be replayed later.
    if body.is_active == Some(false) {
        revoke_staff_sessions(&state, &row.id).await?;
    }

    Ok(Json(staff_to_json(&row)))
}

/// Delete all refresh sessions belonging to `staff_id` from Redis.
async fn revoke_staff_sessions(state: &AppState, staff_id: &str) -> Result<(), ApiError> {
    let pattern = state.config.redis_key("auth:refresh:*");
    let mut redis = state.redis.clone();
    let mut cursor: u64 = 0;
    loop {
        let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(&mut redis)
            .await?;
        for key in keys {
            let owner: Option<String> = redis.get(&key).await?;
            if owner.as_deref() == Some(staff_id) {
                let _: () = redis.del(&key).await?;
            }
        }
        cursor = next;
        if cursor == 0 {
            return Ok(());
        }
    }
}

/// Display status: a key past its deadline reads `expired` even before the
/// periodic sweep has materialized the DB status.
fn api_key_display_status(status: &str, expires_at: Option<chrono::DateTime<chrono::Utc>>) -> &'static str {
    match domain::ApiKeyStatus::from_db(status) {
        Ok(domain::ApiKeyStatus::Active)
            if expires_at.is_some_and(|deadline| deadline <= chrono::Utc::now()) =>
        {
            domain::ApiKeyStatus::Expired.as_api()
        }
        Ok(status) => status.as_api(),
        Err(_) => "active",
    }
}

async fn list_api_keys(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = state.db.list_api_keys().await?;
    let data = rows
        .iter()
        .map(|k| {
            json!({
                "id": k.id,
                "name": k.name,
                "prefix": k.prefix,
                "status": api_key_display_status(&k.status, k.expires_at),
                "created_by": k.created_by_name,
                "last_used_at": k.last_used_at.map(|t| t.to_rfc3339()),
                "revoked_at": k.revoked_at.map(|t| t.to_rfc3339()),
                "expires_at": k.expires_at.map(|t| t.to_rfc3339()),
                "created_at": k.created_at.to_rfc3339(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "data": data })))
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    /// Days until expiry — one of 30/90/180/365; absent or null = no expiry.
    expires_in_days: Option<i64>,
}

async fn create_api_key(
    State(state): State<AppState>,
    Extension(current): Extension<CurrentStaff>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let name = body.name.trim();
    if name.is_empty() {
        return Err(ApiError::bad_request("Name is required"));
    }
    let expires_at = match body.expires_in_days {
        None => None,
        Some(days) if config::API_KEY_EXPIRY_ALLOWED_DAYS.contains(&days) => {
            Some(Utc::now() + chrono::Duration::days(days))
        }
        Some(_) => {
            return Err(ApiError::bad_request(
                "expires_in_days must be one of 30, 90, 180, 365 (omit for no expiry)",
            ))
        }
    };

    let tag = state.db.api_key_prefix().await?;
    let material = auth::generate_api_key(tag.as_deref());
    let row = state
        .db
        .create_api_key(
            name,
            &material.prefix,
            &material.sha256_hex,
            &current.id,
            expires_at,
        )
        .await?;

    // `key` is shown exactly once — only its hash is stored.
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.id,
            "name": row.name,
            "prefix": row.prefix,
            "key": material.full_key,
            "status": api_key_display_status(&row.status, row.expires_at),
            "expires_at": row.expires_at.map(|t| t.to_rfc3339()),
            "created_at": row.created_at.to_rfc3339(),
        })),
    ))
}

async fn revoke_api_key(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    if !state.db.revoke_api_key(&id).await? {
        return Err(ApiError::not_found(format!(
            "API key not found or already revoked: {id}"
        )));
    }
    Ok(Json(json!({ "status": "revoked", "id": id })))
}

async fn get_settings(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    settings_json(&state).await
}

/// Partial update: only the provided fields are validated and written.
#[derive(Debug, Deserialize)]
struct UpdateSettingsRequest {
    deposit_expiry_minutes: Option<i64>,
    api_key_prefix: Option<String>,
}

async fn update_settings(
    State(state): State<AppState>,
    Json(body): Json<UpdateSettingsRequest>,
) -> Result<Json<Value>, ApiError> {
    if let Some(minutes) = body.deposit_expiry_minutes {
        if !(1..=config::DEPOSIT_EXPIRY_MAX_MINUTES).contains(&minutes) {
            return Err(ApiError::bad_request(
                "deposit_expiry_minutes must be between 1 and 1440",
            ));
        }
        state
            .db
            .set_setting(db::DEPOSIT_EXPIRY_SETTING, &minutes.to_string())
            .await?;
    }

    if let Some(prefix) = body.api_key_prefix.as_deref() {
        let prefix = prefix.trim();
        // Empty string clears the tag; otherwise 1..=5 alphanumeric chars
        // ('_' is the key-format separator, so it can never appear here).
        let valid = prefix.is_empty()
            || (prefix.len() <= auth::API_KEY_TAG_MAX_LEN
                && prefix.chars().all(|c| c.is_ascii_alphanumeric()));
        if !valid {
            return Err(ApiError::bad_request(
                "api_key_prefix must be 1-5 alphanumeric characters (empty to disable)",
            ));
        }
        state.db.set_setting(db::API_KEY_PREFIX_SETTING, prefix).await?;
    }

    settings_json(&state).await
}

async fn settings_json(state: &AppState) -> Result<Json<Value>, ApiError> {
    let minutes = state.db.deposit_expiry_minutes().await?;
    let api_key_prefix = state.db.api_key_prefix().await?;
    Ok(Json(json!({
        "deposit_expiry_minutes": minutes,
        "api_key_prefix": api_key_prefix,
    })))
}

async fn list_webhook_endpoints(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let rows = state.db.list_webhook_endpoints().await?;
    let data = rows
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "url": e.url,
                "is_active": e.is_active,
                "created_by": e.created_by_name,
                "created_at": e.created_at.to_rfc3339(),
                "updated_at": e.updated_at.to_rfc3339(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "data": data })))
}

#[derive(Debug, Deserialize)]
struct CreateWebhookEndpointRequest {
    url: String,
}

async fn create_webhook_endpoint(
    State(state): State<AppState>,
    Extension(current): Extension<CurrentStaff>,
    Json(body): Json<CreateWebhookEndpointRequest>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let url = validate_webhook_url(&body.url)?;

    if state.db.find_webhook_endpoint_by_url(&url).await?.is_some() {
        return Err(ApiError::conflict(
            "A webhook endpoint with this URL already exists",
        ));
    }

    let row = state.db.create_webhook_endpoint(&url, &current.id).await?;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": row.id,
            "url": row.url,
            "is_active": row.is_active,
            "created_at": row.created_at.to_rfc3339(),
            "updated_at": row.updated_at.to_rfc3339(),
        })),
    ))
}

#[derive(Debug, Deserialize)]
struct UpdateWebhookEndpointRequest {
    is_active: bool,
}

async fn update_webhook_endpoint(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<UpdateWebhookEndpointRequest>,
) -> Result<Json<Value>, ApiError> {
    let Some(row) = state
        .db
        .set_webhook_endpoint_active(&id, body.is_active)
        .await?
    else {
        return Err(ApiError::not_found(format!(
            "Webhook endpoint not found: {id}"
        )));
    };
    Ok(Json(json!({
        "id": row.id,
        "url": row.url,
        "is_active": row.is_active,
        "updated_at": row.updated_at.to_rfc3339(),
    })))
}

async fn delete_webhook_endpoint(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    match state.db.delete_webhook_endpoint_if_unused(&id).await? {
        None => Err(ApiError::not_found(format!(
            "Webhook endpoint not found: {id}"
        ))),
        Some(false) => Err(ApiError::conflict(
            "This endpoint has delivery history — deactivate it instead",
        )),
        Some(true) => Ok(Json(json!({ "status": "deleted", "id": id }))),
    }
}

fn validate_webhook_url(url: &str) -> Result<String, ApiError> {
    let url = url.trim();
    let host = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"));
    let valid = matches!(host, Some(host) if !host.is_empty())
        && url.len() <= 2048
        && !url.chars().any(|c| c.is_whitespace() || c.is_control());
    if !valid {
        return Err(ApiError::bad_request(
            "Invalid webhook URL: must start with http:// or https:// and contain no spaces",
        ));
    }
    Ok(url.to_string())
}

async fn list_webhooks(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, ApiError> {
    let status = query
        .status
        .as_deref()
        .map(WebhookDeliveryStatus::parse_api)
        .transpose()
        .map_err(ApiError::from)?;
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let offset = query.offset.unwrap_or(0).max(0);

    let (rows, total) = state
        .db
        .list_webhook_deliveries_detailed(status, limit, offset)
        .await?;

    let data = rows
        .iter()
        .map(|w| {
            json!({
                "id": w.id,
                // deposits.id — what the retry endpoint expects.
                "deposit_id": w.deposit_id,
                // deposit_addresses.id — what the webhook payload calls deposit_id.
                "deposit_address_id": w.deposit_address_id,
                // NULL on deliveries from before per-endpoint tracking.
                "webhook_endpoint_id": w.webhook_endpoint_id,
                "endpoint_url": w.endpoint_url,
                "tx_hash": w.tx_hash,
                "address": w.address,
                "network": domain::Network::from_db(&w.network).map(|n| n.as_api()).unwrap_or("unknown"),
                "amount": w.amount.normalize().to_string(),
                "deposit_status": domain::DepositStatus::from_db(&w.deposit_status).map(|s| s.as_api()).unwrap_or("detected"),
                "status": WebhookDeliveryStatus::from_db(&w.status).map(|s| s.as_api()).unwrap_or("pending"),
                "attempts": w.attempts,
                "last_response": w.last_response,
                "last_error": w.last_error,
                "created_at": w.created_at.to_rfc3339(),
                "updated_at": w.updated_at.to_rfc3339(),
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(json!({
        "data": data,
        "total": total,
        "limit": limit,
        "offset": offset,
    })))
}

async fn retry_failed(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    match retrigger_failed_webhooks(&state).await {
        Ok(retried) => Ok(Json(json!({ "retried": retried }))),
        Err(RetriggerError::NoActiveEndpoints) => Err(ApiError::service_unavailable(
            "No active webhook endpoints configured".into(),
        )),
        Err(err) => Err(ApiError::internal(format!("{err:?}"))),
    }
}

async fn stats(State(state): State<AppState>) -> Result<Json<Value>, ApiError> {
    let s = state.db.admin_stats().await?;
    Ok(Json(json!({
        "addresses": {
            "pending": s.addresses_pending,
            "paid": s.addresses_paid,
            "expired": s.addresses_expired,
        },
        "deposits": {
            "detected": s.deposits_detected,
            "confirmed": s.deposits_confirmed,
        },
        "confirmed_volume": s.confirmed_volume.normalize().to_string(),
        "webhooks": {
            "pending": s.webhooks_pending,
            "delivered": s.webhooks_delivered,
            "failed": s.webhooks_failed,
        },
    })))
}

#[derive(Debug, Deserialize)]
struct TimeseriesQuery {
    days: Option<i32>,
}

async fn deposits_timeseries(
    State(state): State<AppState>,
    Query(query): Query<TimeseriesQuery>,
) -> Result<Json<Value>, ApiError> {
    let days = query.days.unwrap_or(90).clamp(1, 365);
    let rows = state.db.deposits_timeseries(days).await?;
    let data = rows
        .iter()
        .map(|r| {
            json!({
                "date": r.date,
                "count": r.count,
                "volume": r.volume.normalize().to_string(),
            })
        })
        .collect::<Vec<_>>();
    Ok(Json(json!({ "days": days, "data": data })))
}

async fn issue_token_pair(state: &AppState, staff: &StaffUserRow) -> Result<Value, ApiError> {
    let Some(secret) = state.config.jwt_secret.as_deref() else {
        return Err(ApiError::service_unavailable(
            "JWT_SECRET is not configured".into(),
        ));
    };
    let role =
        StaffRole::from_db(&staff.role).map_err(|e| ApiError::internal(e.to_string()))?;
    let access_token = auth::issue_access_token(
        secret,
        &staff.id,
        &staff.email,
        role,
        state.config.access_token_ttl_minutes,
    )
    .map_err(|e| ApiError::internal(e.to_string()))?;

    let (refresh_token, refresh_hash) = auth::generate_refresh_token();
    let key = state.config.redis_key(&format!("auth:refresh:{refresh_hash}"));
    let ttl_secs = (state.config.refresh_token_ttl_days.max(1) as u64) * 86_400;
    let mut redis = state.redis.clone();
    let _: () = redis.set_ex(key, &staff.id, ttl_secs).await?;

    Ok(json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "staff": staff_to_json(staff),
    }))
}

fn staff_to_json(row: &StaffUserRow) -> Value {
    json!({
        "id": row.id,
        "email": row.email,
        "first_name": row.first_name,
        "last_name": row.last_name,
        "role": StaffRole::from_db(&row.role).map(|r| r.as_api()).unwrap_or("operator"),
        "is_active": row.is_active,
        "created_at": row.created_at.to_rfc3339(),
    })
}

fn validate_email(email: &str) -> Result<String, ApiError> {
    let email = email.trim().to_lowercase();
    let valid = email.len() >= 5
        && email.split('@').count() == 2
        && !email.starts_with('@')
        && !email.ends_with('@');
    if !valid {
        return Err(ApiError::bad_request("Invalid email address"));
    }
    Ok(email)
}

fn validate_names(first_name: &str, last_name: &str) -> Result<(String, String), ApiError> {
    let (first_name, last_name) = (first_name.trim(), last_name.trim());
    if first_name.is_empty() || last_name.is_empty() {
        return Err(ApiError::bad_request("First name and last name are required"));
    }
    Ok((first_name.to_string(), last_name.to_string()))
}

fn validate_password(password: &str) -> Result<(), ApiError> {
    if password.len() < MIN_PASSWORD_LEN {
        return Err(ApiError::bad_request(
            "Password must be at least 8 characters",
        ));
    }
    Ok(())
}
