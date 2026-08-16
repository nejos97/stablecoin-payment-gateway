//! Pure auth primitives: password hashing, JWT access tokens, refresh-token
//! and API-key generation/hashing. No I/O, no async — storage and transport
//! live in `db` and `api`.

use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use chrono::Utc;
use domain::StaffRole;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use rand::distributions::Alphanumeric;
use rand::rngs::OsRng;
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use thiserror::Error;

/// API keys are `[{tag}_]{8 alphanum}_{40 alphanum}` — the optional tag is
/// admin-configurable (dashboard Settings), 1..=5 alphanumeric chars.
pub const API_KEY_TAG_MAX_LEN: usize = 5;
const API_KEY_PREFIX_RANDOM_LEN: usize = 8;
const API_KEY_SECRET_LEN: usize = 40;
/// Longest possible clear-text lookup prefix: `{tag}_{8}`.
const API_KEY_PREFIX_MAX_LEN: usize = API_KEY_TAG_MAX_LEN + 1 + API_KEY_PREFIX_RANDOM_LEN;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("Failed to hash password")]
    PasswordHash,
    #[error("Failed to issue token")]
    TokenIssue,
    #[error("Invalid or expired token")]
    InvalidToken,
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::PasswordHash)
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// JWT claims for staff access tokens. `role` uses the API casing
/// (`admin` / `operator`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub email: String,
    pub role: String,
    pub iat: i64,
    pub exp: i64,
}

pub fn issue_access_token(
    secret: &str,
    staff_id: &str,
    email: &str,
    role: StaffRole,
    ttl_minutes: i64,
) -> Result<String, AuthError> {
    let now = Utc::now().timestamp();
    let claims = Claims {
        sub: staff_id.to_string(),
        email: email.to_string(),
        role: role.as_api().to_string(),
        iat: now,
        exp: now + ttl_minutes * 60,
    };
    jsonwebtoken::encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|_| AuthError::TokenIssue)
}

pub fn decode_access_token(secret: &str, token: &str) -> Result<Claims, AuthError> {
    jsonwebtoken::decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::new(Algorithm::HS256),
    )
    .map(|data| data.claims)
    .map_err(|_| AuthError::InvalidToken)
}

pub fn sha256_hex(input: &str) -> String {
    hex::encode(Sha256::digest(input.as_bytes()))
}

/// Opaque refresh token: 32 random bytes hex-encoded. Returns
/// `(token, sha256_hex)` — only the hash is ever stored.
pub fn generate_refresh_token() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let hash = sha256_hex(&token);
    (token, hash)
}

#[derive(Debug, Clone)]
pub struct ApiKeyMaterial {
    /// The full key, shown to the caller exactly once.
    pub full_key: String,
    /// Clear-text lookup prefix (`spg_<8>`), stored in DB.
    pub prefix: String,
    /// SHA-256 hex of the full key, stored in DB.
    pub sha256_hex: String,
}

/// Generate a key with the given tag (assumed already validated: 1..=5
/// alphanumeric chars) or without one: `acme_{8}_{40}` / `{8}_{40}`.
pub fn generate_api_key(tag: Option<&str>) -> ApiKeyMaterial {
    let prefix_random = random_alphanumeric(API_KEY_PREFIX_RANDOM_LEN);
    let secret = random_alphanumeric(API_KEY_SECRET_LEN);
    let prefix = match tag {
        Some(tag) => format!("{tag}_{prefix_random}"),
        None => prefix_random,
    };
    let full_key = format!("{prefix}_{secret}");
    let hash = sha256_hex(&full_key);
    ApiKeyMaterial {
        full_key,
        prefix,
        sha256_hex: hash,
    }
}

/// Extract the storage prefix from a presented key, or `None` when the shape
/// is invalid. Deliberately tag-agnostic so keys issued under any past tag
/// setting (`spg_{8}`, `{tag}_{8}`, bare `{8}`) keep working: everything
/// before the last `_` is the stored prefix, the rest is the 40-char secret.
pub fn parse_api_key_prefix(key: &str) -> Option<&str> {
    let (prefix, secret) = key.rsplit_once('_')?;
    let prefix_valid = !prefix.is_empty()
        && prefix.len() <= API_KEY_PREFIX_MAX_LEN
        && prefix.split('_').count() <= 2
        && prefix
            .split('_')
            .all(|segment| !segment.is_empty() && segment.chars().all(|c| c.is_ascii_alphanumeric()));
    let secret_valid =
        secret.len() == API_KEY_SECRET_LEN && secret.chars().all(|c| c.is_ascii_alphanumeric());
    if !prefix_valid || !secret_valid {
        return None;
    }
    Some(prefix)
}

pub fn constant_time_eq_hex(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn random_alphanumeric(len: usize) -> String {
    OsRng
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_roundtrip() {
        let hash = hash_password("s3cret-password").unwrap();
        assert!(verify_password("s3cret-password", &hash));
        assert!(!verify_password("wrong-password", &hash));
        assert!(!verify_password("s3cret-password", "not-a-phc-string"));
    }

    #[test]
    fn jwt_roundtrip() {
        let token =
            issue_access_token("test-secret", "staff-1", "a@b.c", StaffRole::Admin, 15).unwrap();
        let claims = decode_access_token("test-secret", &token).unwrap();
        assert_eq!(claims.sub, "staff-1");
        assert_eq!(claims.email, "a@b.c");
        assert_eq!(claims.role, "admin");
        assert!(decode_access_token("other-secret", &token).is_err());
    }

    #[test]
    fn jwt_expired_is_rejected() {
        let token =
            issue_access_token("test-secret", "staff-1", "a@b.c", StaffRole::Operator, -5).unwrap();
        assert!(decode_access_token("test-secret", &token).is_err());
    }

    #[test]
    fn api_key_with_tag() {
        let material = generate_api_key(Some("acme"));
        assert!(material.full_key.starts_with("acme_"));
        assert_eq!(material.prefix.len(), "acme_".len() + 8);
        assert_eq!(
            parse_api_key_prefix(&material.full_key),
            Some(material.prefix.as_str())
        );
        assert_eq!(material.sha256_hex, sha256_hex(&material.full_key));
    }

    #[test]
    fn api_key_without_tag() {
        let material = generate_api_key(None);
        assert_eq!(material.prefix.len(), 8);
        assert!(!material.prefix.contains('_'));
        assert_eq!(material.full_key.len(), 8 + 1 + 40);
        assert_eq!(
            parse_api_key_prefix(&material.full_key),
            Some(material.prefix.as_str())
        );
    }

    #[test]
    fn parse_accepts_all_historic_shapes() {
        let secret = "s".repeat(40);
        // legacy spg tag, custom tag, and tagless keys all parse
        assert_eq!(
            parse_api_key_prefix(&format!("spg_AAAAAAAA_{secret}")),
            Some("spg_AAAAAAAA")
        );
        assert_eq!(
            parse_api_key_prefix(&format!("acme1_BBBBBBBB_{secret}")),
            Some("acme1_BBBBBBBB")
        );
        assert_eq!(
            parse_api_key_prefix(&format!("CCCCCCCC_{secret}")),
            Some("CCCCCCCC")
        );
    }

    #[test]
    fn parse_rejects_bad_shapes() {
        let secret = "s".repeat(40);
        assert_eq!(parse_api_key_prefix("nope"), None);
        assert_eq!(parse_api_key_prefix(""), None);
        assert_eq!(parse_api_key_prefix("spg_short_bad"), None); // secret != 40
        assert_eq!(parse_api_key_prefix(&format!("_{secret}")), None); // empty prefix
        assert_eq!(parse_api_key_prefix(&format!("a_b_c_{secret}")), None); // too many segments
        assert_eq!(parse_api_key_prefix(&format!("waytoolong_AAAAAAAA_{secret}")), None); // prefix too long
        assert_eq!(parse_api_key_prefix(&format!("sp g_AAAAAAAA_{secret}")), None); // whitespace
    }

    #[test]
    fn refresh_tokens_are_unique_and_hashed() {
        let (t1, h1) = generate_refresh_token();
        let (t2, h2) = generate_refresh_token();
        assert_ne!(t1, t2);
        assert_eq!(t1.len(), 64);
        assert_eq!(h1, sha256_hex(&t1));
        assert!(constant_time_eq_hex(&h1, &sha256_hex(&t1)));
        assert!(!constant_time_eq_hex(&h1, &h2));
    }
}
