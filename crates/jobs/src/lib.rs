use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chain::ChainClients;
use config::{
    AppConfig, ADDRESS_REQUEST_DELAY_MS, API_KEY_EXPIRY_SWEEP_INTERVAL_SECS,
    WALLET_BALANCE_SYNC_INTERVAL_SECS, WEBHOOK_MAX_ATTEMPTS, WEBHOOK_RETRY_DELAYS_SECS,
};
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

/// Legacy LIST queue, only drained at startup (pre-backoff deploys).
const LEGACY_WEBHOOK_LIST_KEY: &str = "queue:webhook-delivery";
/// ZSET scored by next-attempt epoch seconds. New suffix on purpose: a Redis
/// key cannot change type in place (WRONGTYPE against the old LIST).
const WEBHOOK_ZSET_KEY: &str = "zset:webhook-delivery";
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

    // Fan out to every active endpoint; no endpoints configured is a no-op.
    if is_confirmed {
        for endpoint in state.db.active_webhook_endpoints().await? {
            if !state.db.has_delivered_webhook(&deposit.id, &endpoint.id).await? {
                enqueue_webhook(state, &deposit.id, &endpoint.id).await?;
            }
        }
    }

    Ok(())
}

/// ZSET member for a scheduled delivery. Both ids are UUIDs (`[0-9a-f-]`),
/// so `|` can never appear inside them.
pub fn encode_queue_member(deposit_id: &str, endpoint_id: &str) -> String {
    format!("{deposit_id}|{endpoint_id}")
}

/// `None` for legacy members written before per-endpoint deliveries (a bare
/// deposit id) — the webhook loop fans those out to all active endpoints.
pub fn decode_queue_member(member: &str) -> Option<(&str, &str)> {
    member
        .split_once('|')
        .filter(|(deposit, endpoint)| !deposit.is_empty() && !endpoint.is_empty())
}

/// Schedule a delivery attempt for "now". ZSET members are unique, so
/// re-enqueueing a pair that already has a scheduled retry simply moves it
/// forward instead of adding a duplicate.
pub async fn enqueue_webhook(state: &AppState, deposit_id: &str, endpoint_id: &str) -> Result<()> {
    let key = state.config.redis_key(WEBHOOK_ZSET_KEY);
    let mut redis = state.redis.clone();
    let _: () = redis
        .zadd(
            key,
            encode_queue_member(deposit_id, endpoint_id),
            chrono::Utc::now().timestamp(),
        )
        .await?;
    Ok(())
}

/// Gap before the next attempt once `attempts` HTTP attempts have been made.
/// `None` when the retry budget is exhausted (or `attempts` is out of range).
pub fn next_retry_delay_secs(attempts: i32) -> Option<u64> {
    if attempts < 1 {
        return None;
    }
    WEBHOOK_RETRY_DELAYS_SECS.get(attempts as usize - 1).copied()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryOutcome {
    /// 2xx received now, or the row was already DELIVERED.
    Delivered,
    /// Attempt failed with retry budget left; `attempts` made so far.
    RetryAt { attempts: i32 },
    /// Attempt failed and the budget is exhausted; automatic retries stop.
    Failed,
    /// Nothing sent (target endpoint inactive or deleted).
    Skipped,
}

#[derive(Debug)]
pub enum RetriggerError {
    DepositNotFound,
    DepositNotConfirmed,
    NoActiveEndpoints,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for RetriggerError {
    fn from(value: anyhow::Error) -> Self {
        Self::Other(value)
    }
}

/// Manual replay for the dashboard: for every active endpoint, reset the
/// delivery row to PENDING / 0 attempts, then schedule an immediate attempt.
/// Never touches `deposits` or `deposit_addresses`.
pub async fn retrigger_webhook(state: &AppState, deposit_id: &str) -> Result<(), RetriggerError> {
    let deposit = state
        .db
        .find_deposit(deposit_id)
        .await?
        .ok_or(RetriggerError::DepositNotFound)?;
    let endpoints = state.db.active_webhook_endpoints().await?;
    if endpoints.is_empty() {
        return Err(RetriggerError::NoActiveEndpoints);
    }
    if deposit.status != DepositStatus::Confirmed.as_db() {
        return Err(RetriggerError::DepositNotConfirmed);
    }

    for endpoint in &endpoints {
        state.db.reset_webhook_delivery(&deposit.id, &endpoint.id).await?;
        enqueue_webhook(state, &deposit.id, &endpoint.id).await?;
    }
    info!(
        "Webhook retriggered for deposit {deposit_id} towards {} endpoint(s)",
        endpoints.len()
    );
    Ok(())
}

/// Bulk manual replay: re-queue every (deposit, endpoint) pair whose
/// authoritative delivery is FAILED; legacy rows (no endpoint) fan out to all
/// active endpoints. Individual failures are logged and skipped; returns the
/// number of pairs successfully re-queued.
pub async fn retrigger_failed_webhooks(state: &AppState) -> Result<usize, RetriggerError> {
    let endpoints = state.db.active_webhook_endpoints().await?;
    if endpoints.is_empty() {
        return Err(RetriggerError::NoActiveEndpoints);
    }

    let targets = state.db.failed_webhook_targets().await?;
    let mut retried = 0usize;
    for (deposit_id, endpoint_id) in &targets {
        let target_endpoints: Vec<&str> = match endpoint_id {
            Some(id) => vec![id.as_str()],
            // Legacy failure — replay towards every active endpoint.
            None => endpoints.iter().map(|e| e.id.as_str()).collect(),
        };
        for endpoint_id in target_endpoints {
            let result: Result<()> = async {
                state.db.reset_webhook_delivery(deposit_id, endpoint_id).await?;
                enqueue_webhook(state, deposit_id, endpoint_id).await
            }
            .await;
            match result {
                Ok(()) => retried += 1,
                Err(err) => {
                    warn!("Bulk webhook retry skipped deposit {deposit_id} endpoint {endpoint_id}: {err:#}")
                }
            }
        }
    }
    info!("Bulk webhook retry: {retried} delivery target(s) re-queued");
    Ok(retried)
}

pub async fn deliver_webhook(
    state: &AppState,
    deposit_id: &str,
    endpoint_id: &str,
) -> Result<DeliveryOutcome> {
    // Reload the endpoint on every attempt so a deactivation takes effect
    // immediately: the queued member is consumed without being rescheduled.
    let endpoint = match state.db.find_webhook_endpoint(endpoint_id).await? {
        Some(endpoint) if endpoint.is_active => endpoint,
        _ => {
            warn!("Webhook delivery skipped for deposit {deposit_id} — endpoint {endpoint_id} is inactive or gone");
            return Ok(DeliveryOutcome::Skipped);
        }
    };
    let webhook_url = &endpoint.url;

    let Some(deposit) = state.db.find_deposit(deposit_id).await? else {
        anyhow::bail!("Deposit not found: {deposit_id}");
    };
    let Some(address) = state.db.find_deposit_address(&deposit.deposit_address_id).await? else {
        anyhow::bail!("Deposit address not found for deposit {deposit_id}");
    };

    let delivery = state
        .db
        .get_or_create_webhook_delivery(deposit_id, endpoint_id)
        .await?;
    if delivery.status == WebhookDeliveryStatus::Delivered.as_db() {
        return Ok(DeliveryOutcome::Delivered);
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
                Ok(DeliveryOutcome::Delivered)
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
                    warn!("Webhook HTTP {status} for deposit {deposit_id} (attempt {attempts})");
                    return Ok(DeliveryOutcome::RetryAt { attempts });
                }
                error!("Webhook delivery failed after {attempts} attempts for deposit {deposit_id}");
                Ok(DeliveryOutcome::Failed)
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
                warn!("Webhook error for deposit {deposit_id} (attempt {attempts}): {err}");
                return Ok(DeliveryOutcome::RetryAt { attempts });
            }
            error!("Webhook delivery failed after {attempts} attempts for deposit {deposit_id}");
            Ok(DeliveryOutcome::Failed)
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
    let s6 = state.clone();
    tokio::spawn(async move { api_key_expiry_loop(s6).await });
}

/// Materialize EXPIRED statuses for past-deadline API keys. Runs once right
/// at boot, then every `API_KEY_EXPIRY_SWEEP_INTERVAL_SECS`. Expired keys are
/// already rejected at lookup time — this keeps listings and stats honest.
async fn api_key_expiry_loop(state: AppState) {
    let mut interval =
        tokio::time::interval(Duration::from_secs(API_KEY_EXPIRY_SWEEP_INTERVAL_SECS));
    loop {
        interval.tick().await;
        match state.db.expire_stale_api_keys().await {
            Ok(0) => {}
            Ok(expired) => info!("Expired {expired} api key(s)"),
            Err(err) => error!("api key expiry sweep error: {err:#}"),
        }
    }
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
    if let Err(err) = drain_legacy_webhook_queue(&state).await {
        warn!("legacy webhook queue drain failed: {err:#}");
    }

    let key = state.config.redis_key(WEBHOOK_ZSET_KEY);
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    loop {
        interval.tick().await;
        let mut redis = state.redis.clone();
        let now = chrono::Utc::now().timestamp();

        let due: Vec<String> = match redis
            .zrangebyscore_limit(&key, "-inf", now, 0, 1)
            .await
        {
            Ok(v) => v,
            Err(err) => {
                error!("webhook queue read error: {err}");
                continue;
            }
        };
        let Some(member) = due.into_iter().next() else {
            continue;
        };
        if let Err(err) = redis.zrem::<_, _, ()>(&key, &member).await {
            error!("webhook queue remove error: {err}");
            continue;
        }

        let Some((deposit_id, endpoint_id)) = decode_queue_member(&member) else {
            // Legacy member (bare deposit id, pre-multi-endpoint deploy):
            // fan it out to the current active endpoints.
            match state.db.active_webhook_endpoints().await {
                Ok(endpoints) if endpoints.is_empty() => {
                    warn!("Dropping legacy webhook queue member {member} — no active endpoints");
                }
                Ok(endpoints) => {
                    info!("Fanning out legacy webhook queue member {member} to {} endpoint(s)", endpoints.len());
                    for endpoint in endpoints {
                        if let Err(err) = enqueue_webhook(&state, &member, &endpoint.id).await {
                            error!("legacy member fan-out error for {member}: {err:#}");
                        }
                    }
                }
                Err(err) => {
                    warn!("legacy member fan-out read error for {member}, rescheduling: {err:#}");
                    let score = now + WEBHOOK_RETRY_DELAYS_SECS[0] as i64;
                    let _: Result<(), _> = redis.zadd(&key, &member, score).await;
                }
            }
            continue;
        };

        match deliver_webhook(&state, deposit_id, endpoint_id).await {
            Ok(DeliveryOutcome::RetryAt { attempts }) => {
                if let Some(delay) = next_retry_delay_secs(attempts) {
                    let score = now + delay as i64;
                    if let Err(err) = redis.zadd::<_, _, _, ()>(&key, &member, score).await {
                        error!("webhook retry schedule error for {member}: {err}");
                    } else {
                        info!("Webhook attempt {} for deposit {deposit_id} (endpoint {endpoint_id}) rescheduled in {delay}s", attempts + 1);
                    }
                }
            }
            Ok(_) => {}
            Err(err) => {
                // Infrastructure error (DB unreachable, row missing): retry
                // after the first backoff gap rather than spinning.
                warn!("webhook delivery error for {member}, rescheduling: {err:#}");
                let score = now + WEBHOOK_RETRY_DELAYS_SECS[0] as i64;
                let _: Result<(), _> = redis.zadd(&key, &member, score).await;
            }
        }
    }
}

/// One-shot migration: move ids from the pre-backoff LIST queue into the
/// scheduled ZSET so in-flight webhooks survive the deploy.
async fn drain_legacy_webhook_queue(state: &AppState) -> Result<()> {
    let list_key = state.config.redis_key(LEGACY_WEBHOOK_LIST_KEY);
    let zset_key = state.config.redis_key(WEBHOOK_ZSET_KEY);
    let mut redis = state.redis.clone();
    let mut drained = 0u32;
    loop {
        let deposit_id: Option<String> = redis.rpop(&list_key, None).await?;
        let Some(deposit_id) = deposit_id else { break };
        let _: () = redis
            .zadd(&zset_key, &deposit_id, chrono::Utc::now().timestamp())
            .await?;
        drained += 1;
    }
    if drained > 0 {
        info!("Drained {drained} webhook id(s) from the legacy list queue");
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_member_roundtrip() {
        let member = encode_queue_member("dep-123", "ep-456");
        assert_eq!(member, "dep-123|ep-456");
        assert_eq!(decode_queue_member(&member), Some(("dep-123", "ep-456")));
    }

    #[test]
    fn legacy_queue_member_is_detected() {
        assert_eq!(decode_queue_member("a1b2c3-uuid-without-separator"), None);
        assert_eq!(decode_queue_member(""), None);
        assert_eq!(decode_queue_member("|"), None);
        assert_eq!(decode_queue_member("dep|"), None);
        assert_eq!(decode_queue_member("|ep"), None);
    }

    #[test]
    fn retry_delays_follow_schedule() {
        assert_eq!(next_retry_delay_secs(1), Some(90));
        assert_eq!(next_retry_delay_secs(2), Some(270));
        assert_eq!(next_retry_delay_secs(3), Some(810));
        assert_eq!(next_retry_delay_secs(4), Some(2430));
    }

    #[test]
    fn retries_stop_at_max_attempts() {
        assert_eq!(next_retry_delay_secs(0), None);
        assert_eq!(next_retry_delay_secs(-1), None);
        assert_eq!(next_retry_delay_secs(WEBHOOK_MAX_ATTEMPTS as i32), None);
        assert_eq!(next_retry_delay_secs(WEBHOOK_MAX_ATTEMPTS as i32 + 1), None);
    }

    #[test]
    fn last_attempt_starts_one_hour_after_first() {
        let total: u64 = (1..WEBHOOK_MAX_ATTEMPTS as i32)
            .map(|attempts| next_retry_delay_secs(attempts).unwrap())
            .sum();
        assert_eq!(total, 3600);
    }
}
