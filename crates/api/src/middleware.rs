use axum::extract::{Request, State};
use axum::http::header::AUTHORIZATION;
use axum::middleware::Next;
use axum::response::Response;
use domain::StaffRole;
use jobs::AppState;

use crate::ApiError;

/// Authenticated staff member, inserted as a request extension by
/// [`require_jwt`]. Admin handlers act on behalf of this user.
#[derive(Debug, Clone)]
pub struct CurrentStaff {
    pub id: String,
    pub email: String,
    pub role: StaffRole,
}

/// Merchant-API guard: validates the `x-api-key` header against the hashed
/// `api_keys` table.
pub async fn require_api_key(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ApiError::unauthorized("Missing x-api-key header"))?;

    let prefix = auth::parse_api_key_prefix(key)
        .ok_or_else(|| ApiError::unauthorized("Invalid API key"))?;
    let Some(row) = state.db.find_api_key_by_prefix(prefix).await? else {
        return Err(ApiError::unauthorized("Invalid API key"));
    };
    if !auth::constant_time_eq_hex(&auth::sha256_hex(key), &row.key_hash) {
        return Err(ApiError::unauthorized("Invalid API key"));
    }

    // Fire-and-forget: never block the request on the usage timestamp.
    let db = state.db.clone();
    tokio::spawn(async move {
        if let Err(err) = db.touch_api_key_last_used(&row.id).await {
            tracing::warn!("Failed to update API key lastUsedAt: {err:#}");
        }
    });

    Ok(next.run(request).await)
}

/// Dashboard guard: validates the `Authorization: Bearer` access token and
/// loads the staff user (rejected when deactivated).
pub async fn require_jwt(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let Some(secret) = state.config.jwt_secret.as_deref() else {
        return Err(ApiError::service_unavailable(
            "JWT_SECRET is not configured".into(),
        ));
    };

    let token = request
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError::unauthorized("Missing bearer token"))?;

    let claims = auth::decode_access_token(secret, token)
        .map_err(|_| ApiError::unauthorized("Invalid or expired token"))?;

    let Some(staff) = state.db.find_staff_by_id(&claims.sub).await? else {
        return Err(ApiError::unauthorized("Unknown staff user"));
    };
    if !staff.is_active {
        return Err(ApiError::unauthorized("Account is deactivated"));
    }
    let role = StaffRole::from_db(&staff.role)
        .map_err(|e| ApiError::internal(e.to_string()))?;

    request.extensions_mut().insert(CurrentStaff {
        id: staff.id,
        email: staff.email,
        role,
    });
    Ok(next.run(request).await)
}

/// Must sit inside a [`require_jwt`] layer; rejects non-admin staff.
pub async fn require_admin(request: Request, next: Next) -> Result<Response, ApiError> {
    let staff = request
        .extensions()
        .get::<CurrentStaff>()
        .ok_or_else(|| ApiError::unauthorized("Not authenticated"))?;
    if staff.role != StaffRole::Admin {
        return Err(ApiError::forbidden("Admin role required"));
    }
    Ok(next.run(request).await)
}
