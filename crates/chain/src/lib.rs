use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use config::{AppConfig, USDT_ETH, USDT_SOLANA, USDT_TRON};
use domain::Network;
use serde::Deserialize;
use solana_sdk::pubkey::Pubkey;
use tracing::{debug, warn};

/// keccak256("Transfer(address,address,uint256)")
const TRANSFER_TOPIC: &str =
    "0xddf252ad1be2c89b69c2b068fc378daa952ba7f163c4a11628f55a4df523b3ef";

#[derive(Debug, Clone)]
pub struct DetectedOnChainTransfer {
    pub tx_hash: String,
    pub amount_raw: String,
    pub confirmations: i32,
}

#[derive(Clone)]
pub struct ChainClients {
    config: Arc<AppConfig>,
    http: reqwest::Client,
    tron_rate_limited_until: Arc<tokio::sync::Mutex<Option<Instant>>>,
}

impl ChainClients {
    pub fn new(config: Arc<AppConfig>) -> Self {
        Self {
            config,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .expect("http client"),
            tron_rate_limited_until: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    pub fn is_network_available(&self, network: Network) -> bool {
        match network {
            Network::Tron => self.config.trongrid_api_key.is_some(),
            Network::Ethereum => self.config.alchemy_api_key.is_some(),
            Network::Solana => self.config.helius_api_key.is_some(),
        }
    }

    pub fn has_any_network_available(&self) -> bool {
        self.is_network_available(Network::Tron)
            || self.is_network_available(Network::Ethereum)
            || self.is_network_available(Network::Solana)
    }

    pub fn missing_key_message(&self, network: Network) -> &'static str {
        match network {
            Network::Tron => "TRONGRID_API_KEY not configured",
            Network::Ethereum => "ALCHEMY_API_KEY not configured",
            Network::Solana => "HELIUS_API_KEY not configured",
        }
    }

    pub async fn fetch_usdt_balance(&self, network: Network, address: &str) -> Result<u128> {
        match network {
            Network::Tron => self.fetch_tron_balance(address).await,
            Network::Ethereum => self.fetch_eth_balance(address).await,
            Network::Solana => self.fetch_sol_balance(address).await,
        }
    }

    pub async fn poll_tron_address(&self, address: &str) -> Result<Vec<DetectedOnChainTransfer>> {
        let Some(api_key) = &self.config.trongrid_api_key else {
            debug!("Skipping Tron monitor — TRONGRID_API_KEY not set");
            return Ok(vec![]);
        };

        {
            let until = self.tron_rate_limited_until.lock().await;
            if let Some(until) = *until {
                if Instant::now() < until {
                    debug!("Tron monitor skipped — backing off after TronGrid rate limit");
                    return Ok(vec![]);
                }
            }
        }

        let current_block = match self.get_tron_block(api_key).await {
            Ok(b) => b,
            Err(e) => {
                if self.handle_tron_rate_limit(&e).await {
                    return Ok(vec![]);
                }
                return Err(e);
            }
        };

        let txs = match self.fetch_trc20(address, api_key).await {
            Ok(t) => t,
            Err(e) => {
                if self.handle_tron_rate_limit(&e).await {
                    return Ok(vec![]);
                }
                return Err(e);
            }
        };

        let mut out = Vec::new();
        for tx in txs {
            if tx.to.as_deref() != Some(address) {
                continue;
            }
            let tx_block = tx.block_number.unwrap_or(current_block);
            let confirmations = (current_block - tx_block + 1).max(1) as i32;
            out.push(DetectedOnChainTransfer {
                tx_hash: tx.transaction_id,
                amount_raw: tx.value,
                confirmations,
            });
        }
        Ok(out)
    }

    pub async fn poll_ethereum_address(
        &self,
        address: &str,
    ) -> Result<Vec<DetectedOnChainTransfer>> {
        let Some(api_key) = &self.config.alchemy_api_key else {
            debug!("Skipping Ethereum monitor — ALCHEMY_API_KEY not set");
            return Ok(vec![]);
        };

        let rpc = self.config.eth_rpc_url(api_key);
        let current_block = eth_block_number(&self.http, &rpc).await?;
        let padded = pad_address_topic(address)?;

        let logs: Vec<EthLog> = eth_rpc(
            &self.http,
            &rpc,
            "eth_getLogs",
            serde_json::json!([{
                "address": USDT_ETH,
                "topics": [TRANSFER_TOPIC, serde_json::Value::Null, padded],
                "fromBlock": format!("0x{:x}", current_block.saturating_sub(5000)),
                "toBlock": format!("0x{current_block:x}")
            }]),
        )
        .await?;

        let mut out = Vec::new();
        for log in logs {
            let Some(tx_hash) = log.transaction_hash else {
                continue;
            };
            let Some(block_hex) = log.block_number else {
                continue;
            };
            let block_number = parse_hex_u64(&block_hex)?;
            let value = parse_hex_u128(&log.data)?;
            let confirmations = (current_block - block_number + 1) as i32;
            out.push(DetectedOnChainTransfer {
                tx_hash,
                amount_raw: value.to_string(),
                confirmations,
            });
        }
        Ok(out)
    }

    pub async fn poll_solana_address(
        &self,
        address: &str,
    ) -> Result<Vec<DetectedOnChainTransfer>> {
        let Some(api_key) = &self.config.helius_api_key else {
            debug!("Skipping Solana monitor — HELIUS_API_KEY not set");
            return Ok(vec![]);
        };

        let rpc = self.config.helius_rpc_url(api_key);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getSignaturesForAddress",
            "params": [address, { "limit": 20 }]
        });
        let resp: serde_json::Value = self
            .http
            .post(&rpc)
            .json(&body)
            .send()
            .await?
            .json()
            .await?;

        let signatures = resp["result"].as_array().cloned().unwrap_or_default();

        let mut out = Vec::new();
        for sig_info in signatures {
            let Some(signature) = sig_info["signature"].as_str() else {
                continue;
            };
            let Some(parsed) = self.fetch_helius_parsed(api_key, signature).await? else {
                continue;
            };
            for transfer in parsed.token_transfers.unwrap_or_default() {
                if transfer.mint.as_deref() != Some(USDT_SOLANA) {
                    continue;
                }
                let destination = transfer
                    .to_token_account
                    .as_deref()
                    .or(transfer.to_user_account.as_deref());
                if destination != Some(address) {
                    continue;
                }
                let amount_raw = transfer
                    .raw_token_amount
                    .and_then(|r| r.token_amount)
                    .unwrap_or_else(|| {
                        ((transfer.token_amount.unwrap_or(0.0) * 1_000_000.0).round() as i64)
                            .to_string()
                    });
                out.push(DetectedOnChainTransfer {
                    tx_hash: parsed.signature.clone(),
                    amount_raw,
                    confirmations: 1,
                });
            }
        }
        Ok(out)
    }

    async fn fetch_tron_balance(&self, address: &str) -> Result<u128> {
        let Some(api_key) = &self.config.trongrid_api_key else {
            return Ok(0);
        };
        let url = self.config.trongrid_url(&format!("v1/accounts/{address}"));
        let resp: TronAccountResponse = self
            .http
            .get(url)
            .header("TRON-PRO-API-KEY", api_key)
            .send()
            .await?
            .json()
            .await?;
        if let Some(entries) = resp.data.and_then(|d| d.into_iter().next()) {
            for map in entries.trc20.unwrap_or_default() {
                if let Some(balance) = map.get(USDT_TRON) {
                    return Ok(balance.parse().unwrap_or(0));
                }
            }
        }
        Ok(0)
    }

    async fn fetch_eth_balance(&self, address: &str) -> Result<u128> {
        let Some(api_key) = &self.config.alchemy_api_key else {
            return Ok(0);
        };
        let rpc = self.config.eth_rpc_url(api_key);
        // balanceOf(address) selector = 0x70a08231
        let data = format!("0x70a08231{}", pad_address_no_prefix(address)?);
        let result: String = eth_rpc(
            &self.http,
            &rpc,
            "eth_call",
            serde_json::json!([{ "to": USDT_ETH, "data": data }, "latest"]),
        )
        .await?;
        parse_hex_u128(&result)
    }

    async fn fetch_sol_balance(&self, address: &str) -> Result<u128> {
        let Some(api_key) = &self.config.helius_api_key else {
            return Ok(0);
        };
        let _pk: Pubkey = address.parse().context("invalid solana address")?;
        let rpc = self.config.helius_rpc_url(api_key);
        let body = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "getTokenAccountBalance",
            "params": [address]
        });
        let resp: serde_json::Value = self.http.post(rpc).json(&body).send().await?.json().await?;
        if let Some(amount) = resp["result"]["value"]["amount"].as_str() {
            return Ok(amount.parse().unwrap_or(0));
        }
        Ok(0)
    }

    async fn get_tron_block(&self, api_key: &str) -> Result<u64> {
        let resp: TronBlockResponse = self
            .http
            .post(self.config.trongrid_url("wallet/getnowblock"))
            .header("TRON-PRO-API-KEY", api_key)
            .json(&serde_json::json!({}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        resp.block_header
            .and_then(|h| h.raw_data)
            .and_then(|r| r.number)
            .context("Unable to fetch current Tron block number")
    }

    async fn fetch_trc20(&self, address: &str, api_key: &str) -> Result<Vec<TronTrc20Tx>> {
        let url = self
            .config
            .trongrid_url(&format!("v1/accounts/{address}/transactions/trc20"));
        let resp: TronTrc20Response = self
            .http
            .get(url)
            .header("TRON-PRO-API-KEY", api_key)
            .query(&[
                ("only_to", "true"),
                ("contract_address", USDT_TRON),
                ("limit", "50"),
            ])
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        Ok(resp.data.unwrap_or_default())
    }

    async fn fetch_helius_parsed(
        &self,
        api_key: &str,
        signature: &str,
    ) -> Result<Option<HeliusTransaction>> {
        let url = self.config.helius_transactions_url(api_key);
        let resp = self
            .http
            .post(url)
            .json(&serde_json::json!({ "transactions": [signature] }))
            .send()
            .await?;
        if !resp.status().is_success() {
            return Ok(None);
        }
        let data: Vec<HeliusTransaction> = resp.json().await?;
        Ok(data.into_iter().next())
    }

    async fn handle_tron_rate_limit(&self, err: &anyhow::Error) -> bool {
        let msg = format!("{err:#}");
        if msg.contains("429") {
            let secs = 60u64;
            warn!(
                "TronGrid rate limited (429) — backing off for {secs}s. Configure TRONGRID_API_KEY for higher limits."
            );
            *self.tron_rate_limited_until.lock().await =
                Some(Instant::now() + Duration::from_secs(secs));
            return true;
        }
        false
    }
}

async fn eth_block_number(http: &reqwest::Client, rpc: &str) -> Result<u64> {
    let hex: String = eth_rpc(http, rpc, "eth_blockNumber", serde_json::json!([])).await?;
    parse_hex_u64(&hex)
}

async fn eth_rpc<T: for<'de> Deserialize<'de>>(
    http: &reqwest::Client,
    rpc: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<T> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params
    });
    let resp: EthRpcResponse<T> = http.post(rpc).json(&body).send().await?.json().await?;
    resp.result.context(format!("RPC {method} returned no result"))
}

fn pad_address_topic(address: &str) -> Result<String> {
    Ok(format!("0x{}", pad_address_no_prefix(address)?))
}

fn pad_address_no_prefix(address: &str) -> Result<String> {
    let hex = address.trim_start_matches("0x").to_lowercase();
    if hex.len() != 40 {
        anyhow::bail!("invalid eth address length");
    }
    Ok(format!("{:0>64}", hex))
}

fn parse_hex_u64(value: &str) -> Result<u64> {
    let hex = value.trim_start_matches("0x");
    Ok(u64::from_str_radix(hex, 16)?)
}

fn parse_hex_u128(value: &str) -> Result<u128> {
    let hex = value.trim_start_matches("0x");
    if hex.is_empty() {
        return Ok(0);
    }
    Ok(u128::from_str_radix(hex, 16)?)
}

#[derive(Debug, Deserialize)]
struct EthRpcResponse<T> {
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct EthLog {
    data: String,
    #[serde(rename = "transactionHash")]
    transaction_hash: Option<String>,
    #[serde(rename = "blockNumber")]
    block_number: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TronTrc20Response {
    data: Option<Vec<TronTrc20Tx>>,
}

#[derive(Debug, Deserialize)]
struct TronTrc20Tx {
    transaction_id: String,
    to: Option<String>,
    value: String,
    block_number: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TronBlockResponse {
    block_header: Option<TronBlockHeader>,
}

#[derive(Debug, Deserialize)]
struct TronBlockHeader {
    raw_data: Option<TronRawData>,
}

#[derive(Debug, Deserialize)]
struct TronRawData {
    number: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TronAccountResponse {
    data: Option<Vec<TronAccount>>,
}

#[derive(Debug, Deserialize)]
struct TronAccount {
    trc20: Option<Vec<std::collections::HashMap<String, String>>>,
}

#[derive(Debug, Deserialize)]
struct HeliusTransaction {
    signature: String,
    #[serde(rename = "tokenTransfers")]
    token_transfers: Option<Vec<HeliusTokenTransfer>>,
}

#[derive(Debug, Deserialize)]
struct HeliusTokenTransfer {
    mint: Option<String>,
    #[serde(rename = "toUserAccount")]
    to_user_account: Option<String>,
    #[serde(rename = "toTokenAccount")]
    to_token_account: Option<String>,
    #[serde(rename = "tokenAmount")]
    token_amount: Option<f64>,
    #[serde(rename = "rawTokenAmount")]
    raw_token_amount: Option<HeliusRawAmount>,
}

#[derive(Debug, Deserialize)]
struct HeliusRawAmount {
    #[serde(rename = "tokenAmount")]
    token_amount: Option<String>,
}
