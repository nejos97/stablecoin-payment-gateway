use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chain::ChainClients;
use config::{AppConfig, ADDRESS_REQUEST_DELAY_MS, WALLET_BALANCE_SYNC_INTERVAL_SECS, WEBHOOK_MAX_ATTEMPTS};
use db::Db;
use domain::{
    raw_to_decimal, raw_to_decimal_string, required_confirmations, DetectedTransfer,
    DepositAddressStatus, DepositStatus, Network, WebhookDeliveryStatus,
    is_amount_within_tolerance,
};
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};
use wallet::WalletService;

const WEBHOOK_QUEUE_KEY: &str = "queue:webhook-delivery";
const WALLET_BALANCES_KEY: &str = "wallet-balances";

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Db,
    pub wallet: WalletService,
    pub chain: ChainClients,
    pub redis: ConnectionManager,
}

impl AppState {
    pub async fn redis_key(&self, suffix: &str) -> String {
        self.config.redis_key(suffix)
    }
}

pub async fn process_detected_transfer(state: &AppState, transfer: DetectedTransfer) -> Result<()> {
    let address = match state.db.find_deposit_address(&transfer.deposit_address_id).await? {
        Some(a) => a,
        None => return Ok(()),
    };

    if address.status != DepositAddressStatus::Pending.as_db() {
        return Ok(());
    }

    let network = Network::from_db(&address.network)?;
    let required = required_confirmations(
        network,
        state.config.tron_confirmations,
        state.config.eth_confirmations,
    );

    if let Some(existing) = state.db.find_deposit_by_tx(&transfer.tx_hash).await? {
        if existing.status == DepositStatus::Confirmed.as_db() {
            return Ok(());
        }
    }

    let amount = raw_to_decimal(&transfer.amount_raw)?;
    let amount_str = amount.normalize().to_string();
    let is_confirmed = transfer.confirmations >= required;

    if !is_amount_within_tolerance(
        &amount_str,
        &address.expected_amount.to_string(),
        state.config.amount_tolerance_percent,
    ) {
        warn!(
            "Deposit amount {amount_str} outside tolerance for address {}",
            address.id
        );
        return Ok(());
    }

    let status = if is_confirmed {
        DepositStatus::Confirmed
    } else {
        DepositStatus::Detected
    };

    let deposit = state
        .db
        .upsert_deposit_and_maybe_pay(
            &transfer.deposit_address_id,
            &transfer.tx_hash,
            amount,
            &transfer.amount_raw,
            transfer.confirmations,
            status,
            is_confirmed,
        )
        .await?;

    if is_confirmed && state.config.webhook_callback_url.is_some() {
        if !state.db.has_delivered_webhook(&deposit.id).await? {
            enqueue_webhook(state, &deposit.id).await?;
        }
    }

    Ok(())
}

async fn enqueue_webhook(state: &AppState, deposit_id: &str) -> Result<()> {
    let key = state.redis_key(WEBHOOK_QUEUE_KEY).await;
    let mut redis = state.redis.clone();
    let _: () = redis.lpush(key, deposit_id).await?;
    Ok(())
}

pub async fn deliver_webhook(state: &AppState, deposit_id: &str) -> Result<()> {
    let Some(webhook_url) = &state.config.webhook_callback_url else {
        warn!("Webhook delivery skipped — WEBHOOK_CALLBACK_URL is not set");
        return Ok(());
    };

    let Some(deposit) = state.db.find_deposit(deposit_id).await? else {
        anyhow::bail!("Deposit not found: {deposit_id}");
    };
    let Some(address) = state.db.find_deposit_address(&deposit.deposit_address_id).await? else {
        anyhow::bail!("Deposit address not found for deposit {deposit_id}");
    };

    let delivery = state.db.get_or_create_webhook_delivery(deposit_id).await?;
    if delivery.status == WebhookDeliveryStatus::Delivered.as_db() {
        return Ok(());
    }

    let network = Network::from_db(&address.network)?;
    let payload = WebhookPayload {
        event_type: "deposit_confirmed",
        data: WebhookData {
            deposit_id: deposit.deposit_address_id.clone(),
            address: address.address.clone(),
            network: network.as_api().to_string(),
            token: address.token.clone(),
            amount: deposit.amount.to_string(),
            amount_raw: deposit.amount_raw.clone(),
            tx_hash: deposit.tx_hash.clone(),
            confirmations: deposit.confirmations,
            status: "confirmed",
        },
    };

    let client = reqwest::Client::new();
    match client
        .post(webhook_url)
        .timeout(Duration::from_secs(10))
        .json(&payload)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status().as_u16() as i32;
            if resp.status().is_success() {
                state
                    .db
                    .update_webhook_delivery(
                        &delivery.id,
                        WebhookDeliveryStatus::Delivered,
                        delivery.attempts + 1,
                        Some(status),
                        None,
                    )
                    .await?;
                info!("Webhook delivered for deposit {deposit_id}");
                Ok(())
            } else {
                let attempts = delivery.attempts + 1;
                let final_status = if attempts >= WEBHOOK_MAX_ATTEMPTS as i32 {
                    WebhookDeliveryStatus::Failed
                } else {
                    WebhookDeliveryStatus::Pending
                };
                state
                    .db
                    .update_webhook_delivery(
                        &delivery.id,
                        final_status,
                        attempts,
                        Some(status),
                        Some(format!("HTTP {status}")),
                    )
                    .await?;
                if attempts < WEBHOOK_MAX_ATTEMPTS as i32 {
                    anyhow::bail!("Webhook HTTP {status}");
                }
                error!("Webhook delivery failed after {attempts} attempts for deposit {deposit_id}");
                Ok(())
            }
        }
        Err(err) => {
            let attempts = delivery.attempts + 1;
            let final_status = if attempts >= WEBHOOK_MAX_ATTEMPTS as i32 {
                WebhookDeliveryStatus::Failed
            } else {
                WebhookDeliveryStatus::Pending
            };
            state
                .db
                .update_webhook_delivery(
                    &delivery.id,
                    final_status,
                    attempts,
                    None,
                    Some(err.to_string()),
                )
                .await?;
            if attempts < WEBHOOK_MAX_ATTEMPTS as i32 {
                return Err(err.into());
            }
            error!("Webhook delivery failed after {attempts} attempts for deposit {deposit_id}");
            Ok(())
        }
    }
}

pub fn spawn_workers(state: AppState) {
    let poll_secs = state.config.polling_interval_seconds.max(1);
    let s1 = state.clone();
    tokio::spawn(async move { monitor_loop(s1, Network::Tron, poll_secs).await });
    let s2 = state.clone();
    tokio::spawn(async move { monitor_loop(s2, Network::Ethereum, poll_secs).await });
    let s3 = state.clone();
    tokio::spawn(async move { monitor_loop(s3, Network::Solana, poll_secs).await });
    let s4 = state.clone();
    tokio::spawn(async move { webhook_loop(s4).await });
    let s5 = state.clone();
    tokio::spawn(async move { wallet_balance_sync_loop(s5).await });
}

async fn monitor_loop(state: AppState, network: Network, poll_secs: u64) {
    let mut interval = tokio::time::interval(Duration::from_secs(poll_secs));
    loop {
        interval.tick().await;
        if let Err(err) = poll_network(&state, network).await {
            error!("{network:?} monitor error: {err:#}");
        }
    }
}

async fn poll_network(state: &AppState, network: Network) -> Result<()> {
    if !state.chain.is_network_available(network) {
        debug!(
            "Skipping {:?} monitor — {}",
            network,
            state.chain.missing_key_message(network)
        );
        return Ok(());
    }

    let _ = state.db.expire_stale_addresses().await?;
    let addresses = state.db.pending_addresses(network).await?;
    if addresses.is_empty() {
        return Ok(());
    }

    for (i, address) in addresses.iter().enumerate() {
        if i > 0 {
            tokio::time::sleep(Duration::from_millis(ADDRESS_REQUEST_DELAY_MS)).await;
        }
        let transfers = match network {
            Network::Tron => state.chain.poll_tron_address(&address.address).await,
            Network::Ethereum => state.chain.poll_ethereum_address(&address.address).await,
            Network::Solana => state.chain.poll_solana_address(&address.address).await,
        };

        match transfers {
            Ok(transfers) => {
                for t in transfers {
                    if let Err(err) = process_detected_transfer(
                        state,
                        DetectedTransfer {
                            deposit_address_id: address.id.clone(),
                            tx_hash: t.tx_hash,
                            amount_raw: t.amount_raw,
                            confirmations: t.confirmations,
                        },
                    )
                    .await
                    {
                        error!("process transfer error for {}: {err:#}", address.id);
                    }
                }
            }
            Err(err) => {
                error!("{:?} monitor error for {}: {err:#}", network, address.id);
            }
        }
    }
    Ok(())
}

async fn webhook_loop(state: AppState) {
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        let key = state.config.redis_key(WEBHOOK_QUEUE_KEY);
        let mut redis = state.redis.clone();
        let deposit_id: Option<String> = match redis.rpop(key, None).await {
            Ok(v) => v,
            Err(err) => {
                error!("webhook queue pop error: {err}");
                continue;
            }
        };
        if let Some(deposit_id) = deposit_id {
            if let Err(err) = deliver_webhook(&state, &deposit_id).await {
                warn!("webhook retry enqueue for {deposit_id}: {err:#}");
                let key = state.config.redis_key(WEBHOOK_QUEUE_KEY);
                let _: Result<(), _> = redis.lpush(key, deposit_id).await;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        }
    }
}

async fn wallet_balance_sync_loop(state: AppState) {
    // run once shortly after boot, then hourly
    tokio::time::sleep(Duration::from_secs(5)).await;
    loop {
        if let Err(err) = sync_wallet_balances(&state).await {
            error!("wallet balance sync error: {err:#}");
        }
        tokio::time::sleep(Duration::from_secs(WALLET_BALANCE_SYNC_INTERVAL_SECS)).await;
    }
}

pub async fn sync_wallet_balances(state: &AppState) -> Result<()> {
    if !state.chain.has_any_network_available() {
        debug!("Skipping wallet balance sync — no blockchain API keys configured");
        return Ok(());
    }

    info!("Starting wallet HD balance sync");
    let mut networks = Vec::new();
    let mut total_raw: u128 = 0;

    for network in [Network::Tron, Network::Ethereum, Network::Solana] {
        let result = sync_network(state, network).await?;
        total_raw += result.amount_raw.parse::<u128>().unwrap_or(0);
        networks.push(result);
    }

    let payload = CachedWalletBalances {
        total_usdt: raw_to_decimal_string(&total_raw.to_string()).unwrap_or_else(|_| "0".into()),
        total_usdt_raw: total_raw.to_string(),
        networks,
        updated_at: chrono::Utc::now().to_rfc3339(),
    };

    let key = state.config.redis_key(WALLET_BALANCES_KEY);
    let mut redis = state.redis.clone();
    let _: () = redis.set(key, serde_json::to_string(&payload)?).await?;
    info!("Wallet HD balance sync completed — total USDT: {}", payload.total_usdt);
    Ok(())
}

async fn sync_network(state: &AppState, network: Network) -> Result<NetworkWalletBalance> {
    let network_name = network.as_api().to_string();
    if !state.chain.is_network_available(network) {
        return Ok(NetworkWalletBalance {
            network: network_name,
            token: "USDT".into(),
            amount: "0".into(),
            amount_raw: "0".into(),
            indices_scanned: 0,
            available: false,
            message: Some(state.chain.missing_key_message(network).to_string()),
        });
    }

    let max_index = state.db.max_derivation_index(network).await?;
    let mut total_raw: u128 = 0;
    for index in 0..=max_index {
        if index > 0 {
            tokio::time::sleep(Duration::from_millis(ADDRESS_REQUEST_DELAY_MS)).await;
        }
        let address = state.wallet.derive_address(network, index as u32)?;
        match state.chain.fetch_usdt_balance(network, &address).await {
            Ok(bal) => total_raw += bal,
            Err(err) => warn!(
                "Failed to fetch wallet balance for {network_name} index {index}: {err:#}"
            ),
        }
    }

    Ok(NetworkWalletBalance {
        network: network_name,
        token: "USDT".into(),
        amount: raw_to_decimal_string(&total_raw.to_string()).unwrap_or_else(|_| "0".into()),
        amount_raw: total_raw.to_string(),
        indices_scanned: max_index + 1,
        available: true,
        message: None,
    })
}

pub async fn get_cached_wallet_balances(state: &AppState) -> Result<WalletBalancesResponse> {
    let key = state.config.redis_key(WALLET_BALANCES_KEY);
    let mut redis = state.redis.clone();
    let raw: Option<String> = redis.get(key).await?;
    if let Some(raw) = raw {
        let cached: CachedWalletBalances = serde_json::from_str(&raw)?;
        return Ok(WalletBalancesResponse {
            total_usdt: cached.total_usdt,
            networks: cached.networks,
            updated_at: Some(cached.updated_at),
        });
    }
    Ok(WalletBalancesResponse {
        total_usdt: "0".into(),
        networks: vec![
            empty_wallet_network("tron"),
            empty_wallet_network("ethereum"),
            empty_wallet_network("solana"),
        ],
        updated_at: None,
    })
}

pub async fn get_live_balances(state: &AppState) -> Result<BalancesResponse> {
    let mut networks = Vec::new();
    let mut total_raw: u128 = 0;
    for network in [Network::Tron, Network::Ethereum, Network::Solana] {
        let result = live_network_balance(state, network).await?;
        total_raw += result.amount_raw.parse::<u128>().unwrap_or(0);
        networks.push(result);
    }
    Ok(BalancesResponse {
        total_usdt: raw_to_decimal_string(&total_raw.to_string()).unwrap_or_else(|_| "0".into()),
        networks,
    })
}

async fn live_network_balance(state: &AppState, network: Network) -> Result<NetworkBalance> {
    let network_name = network.as_api().to_string();
    if !state.chain.is_network_available(network) {
        return Ok(NetworkBalance {
            network: network_name,
            token: "USDT".into(),
            amount: "0".into(),
            amount_raw: "0".into(),
            addresses_checked: 0,
            available: false,
            message: Some(state.chain.missing_key_message(network).to_string()),
        });
    }

    let addresses = state.db.addresses_for_network(network).await?;
    let mut total_raw: u128 = 0;
    for address in &addresses {
        match state.chain.fetch_usdt_balance(network, address).await {
            Ok(bal) => total_raw += bal,
            Err(err) => warn!("Failed to fetch {network_name} balance for {address}: {err:#}"),
        }
    }

    Ok(NetworkBalance {
        network: network_name,
        token: "USDT".into(),
        amount: raw_to_decimal_string(&total_raw.to_string()).unwrap_or_else(|_| "0".into()),
        amount_raw: total_raw.to_string(),
        addresses_checked: addresses.len() as i32,
        available: true,
        message: None,
    })
}

fn empty_wallet_network(name: &str) -> NetworkWalletBalance {
    NetworkWalletBalance {
        network: name.into(),
        token: "USDT".into(),
        amount: "0".into(),
        amount_raw: "0".into(),
        indices_scanned: 0,
        available: false,
        message: None,
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CachedWalletBalances {
    pub total_usdt: String,
    pub total_usdt_raw: String,
    pub networks: Vec<NetworkWalletBalance>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkWalletBalance {
    pub network: String,
    pub token: String,
    pub amount: String,
    pub amount_raw: String,
    pub indices_scanned: i32,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WalletBalancesResponse {
    pub total_usdt: String,
    pub networks: Vec<NetworkWalletBalance>,
    pub updated_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BalancesResponse {
    pub total_usdt: String,
    pub networks: Vec<NetworkBalance>,
}

#[derive(Debug, Serialize)]
pub struct NetworkBalance {
    pub network: String,
    pub token: String,
    pub amount: String,
    pub amount_raw: String,
    pub addresses_checked: i32,
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Serialize)]
struct WebhookPayload {
    event_type: &'static str,
    data: WebhookData,
}

#[derive(Debug, Serialize)]
struct WebhookData {
    deposit_id: String,
    address: String,
    network: String,
    token: String,
    amount: String,
    amount_raw: String,
    tx_hash: String,
    confirmations: i32,
    status: &'static str,
}
