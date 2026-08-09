use bip39::{Language, Mnemonic};
use domain::Network;
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use k256::ecdsa::SigningKey as EthSigningKey;
use sha2::{Digest, Sha256, Sha512};
use sha3::Keccak256;
use solana_sdk::pubkey::Pubkey;
use spl_associated_token_account::get_associated_token_address;
use thiserror::Error;
use tiny_hderive::bip32::ExtendedPrivKey;

type HmacSha512 = Hmac<Sha512>;

const USDT_SOLANA: &str = "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB";

#[derive(Debug, Error)]
pub enum WalletError {
    #[error("WALLET_MNEMONIC must be exactly 12 or 24 words")]
    InvalidWordCount,
    #[error("WALLET_MNEMONIC is not a valid BIP39 phrase")]
    InvalidMnemonic,
    #[error("Failed to derive address: {0}")]
    Derivation(String),
}

#[derive(Clone)]
pub struct WalletService {
    mnemonic: String,
}

impl WalletService {
    pub fn from_mnemonic(mnemonic: &str) -> Result<Self, WalletError> {
        validate_mnemonic(mnemonic)?;
        Ok(Self {
            mnemonic: mnemonic.trim().to_string(),
        })
    }

    pub fn derive_address(&self, network: Network, index: u32) -> Result<String, WalletError> {
        match network {
            Network::Ethereum => self.derive_ethereum(index),
            Network::Tron => self.derive_tron(index),
            Network::Solana => self.derive_solana_ata(index),
        }
    }

    fn derive_ethereum(&self, index: u32) -> Result<String, WalletError> {
        let sk = self.derive_eth_signing_key(index)?;
        Ok(eth_address_checksummed(&sk))
    }

    fn derive_tron(&self, index: u32) -> Result<String, WalletError> {
        let sk = self.derive_eth_signing_key(index)?;
        Ok(tron_address_from_key(&sk))
    }

    fn derive_solana_ata(&self, index: u32) -> Result<String, WalletError> {
        let seed = Mnemonic::parse_in_normalized(Language::English, &self.mnemonic)
            .map_err(|_| WalletError::InvalidMnemonic)?
            .to_seed("");

        let path = format!("m/44'/501'/{index}'/0'");
        let derived = derive_ed25519_slip10(&seed, &path)?;
        let signing_key = SigningKey::from_bytes(&derived);
        let pubkey = Pubkey::new_from_array(signing_key.verifying_key().to_bytes());
        let mint: Pubkey = USDT_SOLANA
            .parse()
            .map_err(|e: solana_sdk::pubkey::ParsePubkeyError| {
                WalletError::Derivation(e.to_string())
            })?;
        let ata = get_associated_token_address(&pubkey, &mint);
        Ok(ata.to_string())
    }

    fn derive_eth_signing_key(&self, index: u32) -> Result<EthSigningKey, WalletError> {
        let seed = Mnemonic::parse_in_normalized(Language::English, &self.mnemonic)
            .map_err(|_| WalletError::InvalidMnemonic)?
            .to_seed("");

        let path = format!("m/44'/60'/0'/0/{index}");
        let xprv = ExtendedPrivKey::derive(&seed, path.as_str())
            .map_err(|e| WalletError::Derivation(format!("{e:?}")))?;
        EthSigningKey::from_slice(&xprv.secret())
            .map_err(|e| WalletError::Derivation(e.to_string()))
    }
}

pub fn validate_mnemonic(mnemonic: &str) -> Result<(), WalletError> {
    let normalized = mnemonic.trim();
    let words: Vec<_> = normalized.split_whitespace().collect();
    if words.len() != 12 && words.len() != 24 {
        return Err(WalletError::InvalidWordCount);
    }
    Mnemonic::parse_in_normalized(Language::English, normalized)
        .map(|_| ())
        .map_err(|_| WalletError::InvalidMnemonic)
}

fn eth_address_checksummed(sk: &EthSigningKey) -> String {
    let public = sk.verifying_key();
    let uncompressed = public.to_encoded_point(false);
    let bytes = uncompressed.as_bytes();
    let hash = Keccak256::digest(&bytes[1..]);
    let addr = hex::encode(&hash[12..]);
    to_checksum_address(&addr)
}

fn to_checksum_address(addr_hex_lower: &str) -> String {
    let hash = Keccak256::digest(addr_hex_lower.as_bytes());
    let mut out = String::from("0x");
    for (i, c) in addr_hex_lower.chars().enumerate() {
        let byte = hash[i / 2];
        let nibble = if i % 2 == 0 { byte >> 4 } else { byte & 0x0f };
        if c.is_ascii_alphabetic() && nibble >= 8 {
            out.push(c.to_ascii_uppercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn tron_address_from_key(sk: &EthSigningKey) -> String {
    let public = sk.verifying_key();
    let uncompressed = public.to_encoded_point(false);
    let bytes = uncompressed.as_bytes();
    let hash = Keccak256::digest(&bytes[1..]);
    let mut payload = Vec::with_capacity(21);
    payload.push(0x41);
    payload.extend_from_slice(&hash[12..]);
    let checksum = Sha256::digest(Sha256::digest(&payload));
    payload.extend_from_slice(&checksum[..4]);
    bs58::encode(payload).into_string()
}

/// SLIP-0010 ed25519 derivation matching `ed25519-hd-key`.
fn derive_ed25519_slip10(seed: &[u8], path: &str) -> Result<[u8; 32], WalletError> {
    let mut mac = HmacSha512::new_from_slice(b"ed25519 seed")
        .map_err(|e| WalletError::Derivation(e.to_string()))?;
    mac.update(seed);
    let result = mac.finalize().into_bytes();
    let mut key = [0u8; 32];
    let mut chain = [0u8; 32];
    key.copy_from_slice(&result[..32]);
    chain.copy_from_slice(&result[32..]);

    for segment in path.trim_start_matches('m').split('/') {
        if segment.is_empty() {
            continue;
        }
        let hardened = segment.ends_with('\'');
        let index: u32 = segment
            .trim_end_matches('\'')
            .parse()
            .map_err(|e| WalletError::Derivation(format!("{e}")))?;
        if !hardened {
            return Err(WalletError::Derivation(
                "ed25519 slip10 only supports hardened derivation".into(),
            ));
        }
        let index = index | 0x8000_0000;

        let mut mac = HmacSha512::new_from_slice(&chain)
            .map_err(|e| WalletError::Derivation(e.to_string()))?;
        mac.update(&[0u8]);
        mac.update(&key);
        mac.update(&index.to_be_bytes());
        let result = mac.finalize().into_bytes();
        key.copy_from_slice(&result[..32]);
        chain.copy_from_slice(&result[32..]);
    }

    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_MNEMONIC: &str =
        "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn validates_word_count() {
        assert!(matches!(
            validate_mnemonic("short phrase"),
            Err(WalletError::InvalidWordCount)
        ));
    }

    #[test]
    fn derives_ethereum_golden() {
        let wallet = WalletService::from_mnemonic(TEST_MNEMONIC).unwrap();
        assert_eq!(
            wallet.derive_address(Network::Ethereum, 0).unwrap(),
            "0x9858EfFD232B4033E47d90003D41EC34EcaEda94"
        );
        assert_eq!(
            wallet.derive_address(Network::Ethereum, 1).unwrap(),
            "0x6Fac4D18c912343BF86fa7049364Dd4E424Ab9C0"
        );
    }

    #[test]
    fn derives_tron_golden() {
        let wallet = WalletService::from_mnemonic(TEST_MNEMONIC).unwrap();
        assert_eq!(
            wallet.derive_address(Network::Tron, 0).unwrap(),
            "TPrkFhZ8LH8Mruco8vXyA496TaeFBrbmeU"
        );
        assert_eq!(
            wallet.derive_address(Network::Tron, 1).unwrap(),
            "TL9gR3f4FgNa9VZAsPt6xuKXPrfWAUJ5MW"
        );
    }

    #[test]
    fn derives_solana_deterministic() {
        let wallet = WalletService::from_mnemonic(TEST_MNEMONIC).unwrap();
        let a0 = wallet.derive_address(Network::Solana, 0).unwrap();
        let a1 = wallet.derive_address(Network::Solana, 1).unwrap();
        assert_ne!(a0, a1);
        assert!(a0.len() > 30);
        assert_eq!(wallet.derive_address(Network::Solana, 0).unwrap(), a0);
    }
}
