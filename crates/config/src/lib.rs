use std::env;
use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

pub const APP_NAME: &str = "stablecoin-payment-service";
pub const USDT_DECIMALS: u32 = 6;
pub const WEBHOOK_MAX_ATTEMPTS: u32 = 5;
pub const WALLET_BALANCE_SYNC_INTERVAL_SECS: u64 = 60 * 60;
pub const ADDRESS_REQUEST_DELAY_MS: u64 = 300;

pub const USDT_ETH: &str = "0xdAC17F958D2ee523a2206206994597C13D831ec7";
pub const USDT_TRON: &str = "TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t";
pub const USDT_SOLANA: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub port: u16,
    pub app_env: String,
    pub log_level: String,
    pub api_secret: Option<String>,
    pub wallet_mnemonic: String,
    pub database_url: String,
    pub redis_url: String,
    pub redis_prefix: String,
    pub webhook_callback_url: Option<String>,
    pub deposit_expiry_minutes: u64,
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
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        load_dotenv();

        let app_env = env::var("APP_ENV").unwrap_or_else(|_| "development".into());
        let api_secret = optional("API_SECRET");
        if let Some(ref secret) = api_secret {
            if secret.len() < 8 {
                bail!("API_SECRET must be at least 8 characters when set");
            }
        }

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
            api_secret,
            wallet_mnemonic,
            database_url,
            redis_url,
            webhook_callback_url: optional("WEBHOOK_CALLBACK_URL"),
            deposit_expiry_minutes: parse_or("DEPOSIT_EXPIRY_MINUTES", 30),
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
