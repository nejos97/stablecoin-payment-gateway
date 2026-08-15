use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub const USDT_DECIMALS: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Network {
    Tron,
    Solana,
    Ethereum,
}

impl Network {
    pub fn as_db(&self) -> &'static str {
        match self {
            Self::Tron => "TRON",
            Self::Solana => "SOLANA",
            Self::Ethereum => "ETHEREUM",
        }
    }

    pub fn as_api(&self) -> &'static str {
        match self {
            Self::Tron => "tron",
            Self::Solana => "solana",
            Self::Ethereum => "ethereum",
        }
    }

    pub fn parse_api(value: &str) -> Result<Self, DomainError> {
        match value.to_lowercase().as_str() {
            "tron" => Ok(Self::Tron),
            "solana" => Ok(Self::Solana),
            "ethereum" => Ok(Self::Ethereum),
            _ => Err(DomainError::UnsupportedNetwork(value.to_string())),
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "TRON" => Ok(Self::Tron),
            "SOLANA" => Ok(Self::Solana),
            "ETHEREUM" => Ok(Self::Ethereum),
            _ => Err(DomainError::UnsupportedNetwork(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Token {
    Usdt,
}

impl Token {
    pub fn as_db(&self) -> &'static str {
        "USDT"
    }

    pub fn as_api(&self) -> &'static str {
        "USDT"
    }

    pub fn parse_api(value: &str) -> Result<Self, DomainError> {
        if value.eq_ignore_ascii_case("USDT") {
            Ok(Self::Usdt)
        } else {
            Err(DomainError::UnsupportedToken(value.to_string()))
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        Self::parse_api(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepositAddressStatus {
    Pending,
    Paid,
    Expired,
}

impl DepositAddressStatus {
    pub fn as_db(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Paid => "PAID",
            Self::Expired => "EXPIRED",
        }
    }

    pub fn as_api(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Paid => "paid",
            Self::Expired => "expired",
        }
    }

    pub fn parse_api(value: &str) -> Result<Self, DomainError> {
        match value.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "paid" => Ok(Self::Paid),
            "expired" => Ok(Self::Expired),
            _ => Err(DomainError::InvalidStatus(value.to_string())),
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "PAID" => Ok(Self::Paid),
            "EXPIRED" => Ok(Self::Expired),
            _ => Err(DomainError::InvalidStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DepositStatus {
    Detected,
    Confirmed,
}

impl DepositStatus {
    pub fn as_db(&self) -> &'static str {
        match self {
            Self::Detected => "DETECTED",
            Self::Confirmed => "CONFIRMED",
        }
    }

    pub fn as_api(&self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Confirmed => "confirmed",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "DETECTED" => Ok(Self::Detected),
            "CONFIRMED" => Ok(Self::Confirmed),
            _ => Err(DomainError::InvalidStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WebhookDeliveryStatus {
    Pending,
    Delivered,
    Failed,
}

impl WebhookDeliveryStatus {
    pub fn as_db(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Delivered => "DELIVERED",
            Self::Failed => "FAILED",
        }
    }

    pub fn as_api(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Delivered => "delivered",
            Self::Failed => "failed",
        }
    }

    pub fn from_db(value: &str) -> Result<Self, DomainError> {
        match value {
            "PENDING" => Ok(Self::Pending),
            "DELIVERED" => Ok(Self::Delivered),
            "FAILED" => Ok(Self::Failed),
            _ => Err(DomainError::InvalidStatus(value.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DetectedTransfer {
    pub deposit_address_id: String,
    pub tx_hash: String,
    pub amount_raw: String,
    pub confirmations: i32,
}

#[derive(Debug, Error)]
pub enum DomainError {
    #[error("Unsupported network: {0}")]
    UnsupportedNetwork(String),
    #[error("Unsupported token: {0}. Only USDT is supported.")]
    UnsupportedToken(String),
    #[error("Invalid status: {0}")]
    InvalidStatus(String),
    #[error("Invalid amount: {0}")]
    InvalidAmount(String),
}

pub fn raw_to_decimal(raw: &str) -> Result<Decimal, DomainError> {
    let raw_int: i128 = raw
        .parse()
        .map_err(|_| DomainError::InvalidAmount(raw.to_string()))?;
    Ok(Decimal::from_i128_with_scale(raw_int, USDT_DECIMALS))
}

/// Format raw integer string (6 decimals) as decimal string without float.
pub fn raw_to_decimal_string(raw: &str) -> Result<String, DomainError> {
    let value = raw_to_decimal(raw)?;
    Ok(trim_decimal(value))
}

pub fn is_amount_within_tolerance(
    actual: &str,
    expected: &str,
    tolerance_percent: f64,
) -> bool {
    let Ok(actual_value) = actual.parse::<Decimal>() else {
        return false;
    };
    let Ok(expected_value) = expected.parse::<Decimal>() else {
        return false;
    };

    let tolerance_ratio = Decimal::try_from(tolerance_percent / 100.0).unwrap_or(Decimal::ZERO);
    let tolerance = expected_value * tolerance_ratio;
    let diff = if actual_value > expected_value {
        actual_value - expected_value
    } else {
        expected_value - actual_value
    };
    diff <= tolerance
}

pub fn required_confirmations(
    network: Network,
    tron: i32,
    eth: i32,
) -> i32 {
    match network {
        Network::Tron => tron,
        Network::Ethereum => eth,
        Network::Solana => 1,
    }
}

fn trim_decimal(value: Decimal) -> String {
    let s = value.normalize().to_string();
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn amount_within_tolerance() {
        assert!(is_amount_within_tolerance("25", "25.00", 1.0));
        assert!(is_amount_within_tolerance("25.20", "25.00", 1.0));
        assert!(!is_amount_within_tolerance("26", "25.00", 1.0));
    }

    #[test]
    fn raw_conversion() {
        assert_eq!(raw_to_decimal("25000000").unwrap(), dec!(25));
        assert_eq!(raw_to_decimal_string("25000000").unwrap(), "25");
    }

    #[test]
    fn network_roundtrip() {
        assert_eq!(Network::parse_api("tron").unwrap().as_db(), "TRON");
        assert_eq!(Network::from_db("ETHEREUM").unwrap().as_api(), "ethereum");
    }

    #[test]
    fn webhook_status_roundtrip() {
        assert_eq!(
            WebhookDeliveryStatus::from_db("PENDING").unwrap().as_api(),
            "pending"
        );
        assert_eq!(
            WebhookDeliveryStatus::from_db("DELIVERED").unwrap().as_api(),
            "delivered"
        );
        assert_eq!(
            WebhookDeliveryStatus::from_db("FAILED").unwrap().as_api(),
            "failed"
        );
        assert!(WebhookDeliveryStatus::from_db("delivered").is_err());
    }
}
