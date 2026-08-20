use std::env;
use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;

pub const APP_NAME: &str = "stablecoin-payment-service";
pub const USDT_DECIMALS: u32 = 6;
pub const WEBHOOK_MAX_ATTEMPTS: u32 = 5;
/// Gaps between webhook attempts, indexed by `attempts - 1` after a failed
/// attempt. Geometric ratio 3; the gaps sum to 3600s so the 5th and final
/// attempt starts ~1h after the 1st. Length must stay WEBHOOK_MAX_ATTEMPTS - 1.
pub const WEBHOOK_RETRY_DELAYS_SECS: [u64; 4] = [90, 270, 810, 2430];
const _: () = assert!(WEBHOOK_RETRY_DELAYS_SECS.len() == WEBHOOK_MAX_ATTEMPTS as usize - 1);
pub const WALLET_BALANCE_SYNC_INTERVAL_SECS: u64 = 60 * 60;
pub const ADDRESS_REQUEST_DELAY_MS: u64 = 300;
/// Default payment-address expiry when the DB setting is absent; the value
/// itself lives in `app_settings` and is managed from the dashboard.
pub const DEPOSIT_EXPIRY_DEFAULT_MINUTES: i64 = 60;
pub const DEPOSIT_EXPIRY_MAX_MINUTES: i64 = 1440;
/// Allowed API-key lifetimes (1/3/6 months, 1 year); no expiry when omitted.
pub const API_KEY_EXPIRY_ALLOWED_DAYS: [i64; 4] = [30, 90, 180, 365];
/// Expired keys are rejected at lookup time immediately; this sweep only
/// materializes the EXPIRED status for listings and stats.
pub const API_KEY_EXPIRY_SWEEP_INTERVAL_SECS: u64 = 300;
/// Default issuer shown by authenticator apps for staff TOTP enrollments;
/// admins can override it from the dashboard Settings (`app_settings`).
pub const TOTP_ISSUER: &str = "Stablecoin Payment Gateway";

pub const USDT_ETH: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
pub const USDT_TRON: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
pub const USDT_SOLANA: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub app_env: String,
    pub log_level: String,
    pub wallet_mnemonic: String,
    pub database_url: String,
    pub redis_url: String,
    pub redis_prefix: String,
    pub polling_interval_seconds: u64,
    pub eth_confirmations: i32,
    pub tron_confirmations: i32,
    pub amount_tolerance_percent: f64,
    pub trongrid_api_key: Option<String>,
    pub alchemy_api_key: Option<String>,
    pub helius_api_key: Option<String>,
    pub trongrid_base_url: String,
    pub eth_rpc_base_url: String,
    pub helius_rpc_base_url: String,
    pub helius_api_base_url: String,
    pub jwt_secret: Option<String>,
    pub access_token_ttl_minutes: i64,
    pub refresh_token_ttl_days: i64,
    pub cors_allowed_origins: Option<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        load_dotenv();

        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "development".into());
        let wallet_mnemonic = required("WALLET_MNEMONIC")?;
        let database_url = required("DATABASE_URL")?;
        let redis_url = required("REDIS_URL")?;

        Ok(Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".into()),
            redis_prefix: format!("{APP_NAME}-{app_env}"),
            app_env,
            wallet_mnemonic,
            database_url,
            redis_url,
            polling_interval_seconds: parse_or("POLLING_INTERVAL_SECONDS", 10),
            eth_confirmations: parse_or("ETH_CONFIRMATIONS", 12),
            tron_confirmations: parse_or("TRON_CONFIRMATIONS", 19),
            amount_tolerance_percent: parse_or("AMOUNT_TOLERANCE_PERCENT", 1.0),
            trongrid_api_key: optional("TRONGRID_API_KEY"),
            alchemy_api_key: optional("ALCHEMY_API_KEY"),
            helius_api_key: optional("HELIUS_API_KEY"),
            trongrid_base_url: env_string(
                "TRONGRID_BASE_URL",
                "https://api.trongrid.io",
            ),
            eth_rpc_base_url: env_string(
                "ETH_RPC_BASE_URL",
                "https://eth-mainnet.g.alchemy.com/v2",
            ),
            helius_rpc_base_url: env_string(
                "HELIUS_RPC_BASE_URL",
                "https://mainnet.helius-rpc.com",
            ),
            helius_api_base_url: env_string(
                "HELIUS_API_BASE_URL",
                "https://api.helius.xyz",
            ),
            jwt_secret: optional("JWT_SECRET").filter(|s| s.len() >= 32),
            access_token_ttl_minutes: parse_or("ACCESS_TOKEN_TTL_MINUTES", 15),
            refresh_token_ttl_days: parse_or("REFRESH_TOKEN_TTL_DAYS", 30),
            cors_allowed_origins: optional("CORS_ALLOWED_ORIGINS"),
        })
    }

    pub fn redis_key(&self, suffix: &str) -> String {
        format!("{}:{}", self.redis_prefix, suffix)
    }

    pub fn eth_rpc_url(&self, api_key: &str) -> String {
        format!(
            "{}/{}",
            self.eth_rpc_base_url.trim_end_matches('/'),
            api_key
        )
    }

    pub fn helius_rpc_url(&self, api_key: &str) -> String {
        format!(
            "{}/?api-key={api_key}",
            self.helius_rpc_base_url.trim_end_matches('/')
        )
    }

    pub fn helius_transactions_url(&self, api_key: &str) -> String {
        format!(
            "{}/v0/transactions/?api-key={api_key}",
            self.helius_api_base_url.trim_end_matches('/')
        )
    }

    pub fn trongrid_url(&self, path: &str) -> String {
        let base = self.trongrid_base_url.trim_end_matches('/');
        let path = path.trim_start_matches('/');
        format!("{base}/{path}")
    }
}

/// Load `.env` from cwd / parents, then from the Cargo workspace root.
fn load_dotenv() {
    if dotenvy::dotenv().is_ok() {
        return;
    }

    // crates/config → workspace root/.env (works even if cwd is elsewhere)
    let workspace_env = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.env");
    if dotenvy::from_path(&workspace_env).is_ok() {
        return;
    }

    if let Ok(canonical) = workspace_env.canonicalize() {
        let _ = dotenvy::from_path(canonical);
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).with_context(|| {
        format!(
            "{key} must be defined in environment variables (check .env at the project root)"
        )
    })
}

fn optional(key: &str) -> Option<String> {
    env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn env_string(key: &str, default: &str) -> String {
    optional(key).unwrap_or_else(|| default.to_string())
}

fn parse_or<T: std::str::FromStr>(key: &str, default: T) -> T {
    env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn webhook_retry_delays_span_one_hour() {
        assert_eq!(
            WEBHOOK_RETRY_DELAYS_SECS.len(),
            WEBHOOK_MAX_ATTEMPTS as usize - 1
        );
        assert_eq!(WEBHOOK_RETRY_DELAYS_SECS.iter().sum::<u64>(), 3600);
        assert!(WEBHOOK_RETRY_DELAYS_SECS.windows(2).all(|w| w[0] < w[1]));
    }
}
