use std::sync::Arc;

use anyhow::Context;
use api::router;
use chain::ChainClients;
use config::AppConfig;
use db::Db;
use jobs::{spawn_workers, AppState};
use redis::aio::ConnectionManager;
use tracing::info;
use tracing_subscriber::EnvFilter;
use wallet::WalletService;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = Arc::new(AppConfig::from_env()?);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_new(format!("{},gateway=info,api=info,jobs=info", config.log_level))
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    if config.webhook_callback_url.is_none() {
        tracing::warn!("WEBHOOK_CALLBACK_URL is not set — webhooks disabled");
    }
    if config.trongrid_api_key.is_none() {
        tracing::warn!("TRONGRID_API_KEY is not set — Tron monitoring disabled");
    }
    if config.alchemy_api_key.is_none() {
        tracing::warn!("ALCHEMY_API_KEY is not set — Ethereum monitoring disabled");
    }
    if config.helius_api_key.is_none() {
        tracing::warn!("HELIUS_API_KEY is not set — Solana monitoring disabled");
    }

    let wallet = WalletService::from_mnemonic(&config.wallet_mnemonic)
        .context("Invalid WALLET_MNEMONIC")?;
    info!("Wallet mnemonic validated successfully");

    let db = Db::connect(&config.database_url).await?;
    if let Err(err) = db.migrate().await {
        tracing::warn!("sqlx migrate skipped or failed (schema may already exist): {err:#}");
    }

    let redis_client = redis::Client::open(config.redis_url.as_str())
        .context("Invalid REDIS_URL")?;
    let redis = ConnectionManager::new(redis_client)
        .await
        .context("Failed to connect to Redis")?;

    let chain = ChainClients::new(config.clone());
    let state = AppState {
        config: config.clone(),
        db,
        wallet,
        chain,
        redis,
    };

    spawn_workers(state.clone());

    let app = router(state);
    let addr = format!("0.0.0.0:{}", config.port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
