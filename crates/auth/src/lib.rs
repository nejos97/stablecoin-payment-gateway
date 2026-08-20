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

/// `X-Webhook-Signature` value for an outgoing webhook: HMAC-SHA256 of the
/// exact body bytes sent, keyed by the endpoint's secret, GitHub-style
/// (`sha256=<hex>`). Receivers recompute it over the raw body and compare
/// constant-time.
pub fn webhook_signature(secret: &str, body: &[u8]) -> String {
    use hmac::{Hmac, Mac};
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .expect("HMAC accepts keys of any length");
    mac.update(body);
    format!("sha256={}", hex::encode(mac.finalize().into_bytes()))
}

/// TOTP parameters follow the authenticator-app defaults (Google
/// Authenticator, Authy, 1Password): SHA-1, 30-second period, 6 digits.
pub const TOTP_PERIOD_SECS: i64 = 30;
pub const TOTP_DIGITS: u32 = 6;
/// Accepted clock skew, in periods, on each side of "now".
pub const TOTP_SKEW_STEPS: i64 = 1;
const TOTP_SECRET_BYTES: usize = 20;

/// Fresh TOTP secret: 20 random bytes, RFC 4648 base32 without padding —
/// the format authenticator apps expect in `otpauth://` URLs.
pub fn generate_totp_secret() -> String {
    let mut bytes = [0u8; TOTP_SECRET_BYTES];
    OsRng.fill_bytes(&mut bytes);
    base32::encode(base32::Alphabet::Rfc4648 { padding: false }, &bytes)
}

/// Verify a 6-digit code against the secret at `unix_time`, allowing
/// ±`TOTP_SKEW_STEPS` periods of clock skew. Returns the matched timestep
/// counter — the caller persists it to reject replays within the skew
/// window — or `None` on mismatch, malformed code, or invalid secret.
pub fn verify_totp(secret_base32: &str, code: &str, unix_time: i64) -> Option<i64> {
    if code.len() != TOTP_DIGITS as usize || !code.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let counter = unix_time.div_euclid(TOTP_PERIOD_SECS);
    let mut matched = None;
    for candidate in (counter - TOTP_SKEW_STEPS)..=(counter + TOTP_SKEW_STEPS) {
        let expected = totp_code_at(secret_base32, candidate)?;
        if bool::from(expected.as_bytes().ct_eq(code.as_bytes())) {
            matched = Some(candidate);
        }
    }
    matched
}

/// The `otpauth://` provisioning URL encoded into the enrollment QR code.
pub fn totp_otpauth_url(secret_base32: &str, email: &str, issuer: &str) -> String {
    let issuer = totp_url_encode(issuer);
    let email = totp_url_encode(email);
    format!(
        "otpauth://totp/{issuer}:{email}?secret={secret_base32}&issuer={issuer}\
         &algorithm=SHA1&digits={TOTP_DIGITS}&period={TOTP_PERIOD_SECS}"
    )
}

/// RFC 6238: HOTP (RFC 4226 dynamic truncation of HMAC-SHA-1 over the
/// big-endian timestep counter) reduced mod 10^digits, zero-padded.
fn totp_code_at(secret_base32: &str, counter: i64) -> Option<String> {
    use hmac::{Hmac, Mac};
    let key = base32::decode(
        base32::Alphabet::Rfc4648 { padding: false },
        secret_base32.trim(),
    )?;
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&key).ok()?;
    mac.update(&counter.to_be_bytes());
    let digest = mac.finalize().into_bytes();
    let offset = (digest[19] & 0x0f) as usize;
    let binary = u32::from_be_bytes([
        digest[offset] & 0x7f,
        digest[offset + 1],
        digest[offset + 2],
        digest[offset + 3],
    ]);
    Some(format!(
        "{:01$}",
        binary % 10u32.pow(TOTP_DIGITS),
        TOTP_DIGITS as usize
    ))
}

/// Minimal percent-encoding for otpauth URL components (RFC 3986 unreserved
/// characters pass through).
fn totp_url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for byte in input.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
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

    /// "12345678901234567890" (the RFC 6238 SHA-1 test secret) in base32.
    const RFC6238_SECRET: &str = "GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ";

    #[test]
    fn totp_matches_rfc6238_appendix_b_vectors() {
        // RFC 6238 Appendix B SHA-1 vectors, truncated from 8 to 6 digits.
        for (time, code) in [
            (59, "287082"),
            (1_111_111_109, "081804"),
            (1_111_111_111, "050471"),
            (1_234_567_890, "005924"),
            (2_000_000_000, "279037"),
        ] {
            assert_eq!(
                verify_totp(RFC6238_SECRET, code, time),
                Some(time / TOTP_PERIOD_SECS),
                "T={time}"
            );
        }
    }

    #[test]
    fn totp_skew_window() {
        // T=1111111111 → counter 37037037, code 050471. The previous and next
        // periods accept it; ±2 periods reject it.
        let code = "050471";
        assert_eq!(
            verify_totp(RFC6238_SECRET, code, 1_111_111_111 + 30),
            Some(37_037_037)
        );
        assert_eq!(
            verify_totp(RFC6238_SECRET, code, 1_111_111_111 - 30),
            Some(37_037_037)
        );
        assert_eq!(verify_totp(RFC6238_SECRET, code, 1_111_111_111 + 60), None);
        assert_eq!(verify_totp(RFC6238_SECRET, code, 1_111_111_111 - 60), None);
    }

    #[test]
    fn totp_rejects_malformed_codes_and_secrets() {
        assert_eq!(verify_totp(RFC6238_SECRET, "05047", 1_111_111_111), None);
        assert_eq!(verify_totp(RFC6238_SECRET, "0504712", 1_111_111_111), None);
        assert_eq!(verify_totp(RFC6238_SECRET, "abcdef", 1_111_111_111), None);
        assert_eq!(verify_totp("not!base32", "050471", 1_111_111_111), None);
    }

    #[test]
    fn totp_secret_shape() {
        let secret = generate_totp_secret();
        assert_eq!(secret.len(), 32); // 20 bytes → 32 base32 chars, no padding
        assert_ne!(secret, generate_totp_secret());
        assert!(base32::decode(base32::Alphabet::Rfc4648 { padding: false }, &secret).is_some());
    }

    #[test]
    fn otpauth_url_is_encoded() {
        let url = totp_otpauth_url("ABC234", "user@example.com", "My Gateway");
        assert_eq!(
            url,
            "otpauth://totp/My%20Gateway:user%40example.com?secret=ABC234\
             &issuer=My%20Gateway&algorithm=SHA1&digits=6&period=30"
        );
    }

    #[test]
    fn webhook_signature_matches_rfc4231_vector() {
        // RFC 4231 test case 2 (HMAC-SHA-256).
        assert_eq!(
            webhook_signature("Jefe", b"what do ya want for nothing?"),
            "sha256=5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843"
        );
    }

    #[test]
    fn webhook_signature_depends_on_secret_and_body() {
        let body = br#"{"event_type":"deposit_confirmed"}"#;
        let sig = webhook_signature("secret-a", body);
        assert!(sig.starts_with("sha256="));
        assert_eq!(sig.len(), "sha256=".len() + 64);
        assert_ne!(sig, webhook_signature("secret-b", body));
        assert_ne!(sig, webhook_signature("secret-a", b"{}"));
    }
}
