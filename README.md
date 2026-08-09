# Stablecoin Payment Service

[![CI](https://github.com/nejos97/stablecoin-payment-gateway/actions/workflows/ci.yml/badge.svg)](https://github.com/nejos97/stablecoin-payment-gateway/actions/workflows/ci.yml)

Rust service for accepting USDT deposits on **Tron**, **Ethereum**, and **Solana**.

## Features

- Create unique deposit addresses per payment (`POST /api/v1/deposit-addresses`)
- Monitor blockchain transfers and confirm deposits
- Send webhooks to a global `WEBHOOK_CALLBACK_URL` when deposits are confirmed
- HD wallet derivation from `WALLET_MNEMONIC`
- Multi-network support: Tron (TRC-20), Ethereum (ERC-20), Solana (SPL)

## Quick start

```bash
cp .env.example .env
# Edit .env with your secrets and API keys

docker compose up -d postgres redis
cargo run -p gateway
```

On first boot the binary applies SQL migrations (`crates/db/migrations`) via sqlx.

API: `http://localhost:3000` — `GET /healthz`

### Docker

```bash
docker compose up --build
```

## Testing

Unit tests live in the `domain` and `wallet` crates (BIP39 / HD address golden vectors, amount tolerance, network roundtrip). CI runs the same suite:

```bash
cargo test -p domain -p wallet
```

There are no integration tests for the API or chain providers yet.

## Authentication

If `API_SECRET` is set, all endpoints except `GET /healthz` require:

```
X-API-SECRET: <API_SECRET>
```

If `API_SECRET` is empty or unset, authentication is disabled.
## API

### Create deposit address

```bash
curl -X POST http://localhost:3000/api/v1/deposit-addresses \
  -H "Content-Type: application/json" \
  -H "X-API-SECRET: change-me-to-a-long-random-secret" \
  -d '{
    "network": "tron",
    "token": "USDT",
    "expected_amount": "25.00"
  }'
```

### List deposit addresses

```bash
curl "http://localhost:3000/api/v1/deposit-addresses?status=pending&limit=20" \
  -H "X-API-SECRET: change-me-to-a-long-random-secret"
```

Query params (optional):

- `status`: `pending`, `paid`, `expired`
- `network`: `tron`, `solana`, `ethereum`
- `limit`: default `50`, max `100`
- `offset`: default `0`

### Get deposit address status

```bash
curl http://localhost:3000/api/v1/deposit-addresses/<id> \
  -H "X-API-SECRET: change-me-to-a-long-random-secret"
```

### Get USDT balances (deposit addresses in database)

```bash
curl http://localhost:3000/api/v1/balances \
  -H "X-API-SECRET: change-me-to-a-long-random-secret"
```

### Get HD wallet balances (cached, synced hourly)

```bash
curl http://localhost:3000/api/v1/wallet-balances \
  -H "X-API-SECRET: change-me-to-a-long-random-secret"
```

## Webhook payload

Sent to `WEBHOOK_CALLBACK_URL` on confirmed deposit:

```json
{
  "event_type": "deposit_confirmed",
  "data": {
    "deposit_id": "uuid",
    "address": "T...",
    "network": "tron",
    "token": "USDT",
    "amount": "25.00",
    "amount_raw": "25000000",
    "tx_hash": "abc...",
    "confirmations": 19,
    "status": "confirmed"
  }
}
```

If `WEBHOOK_CALLBACK_URL` is not set, a warning is logged at startup and webhooks are disabled.

## Security considerations

This service uses a **custodial hot wallet**: a single operator `WALLET_MNEMONIC` is loaded at boot and used to derive all deposit addresses (HD paths per network). Compromising the host or the environment secret means compromising the entire derivation chain and any funds sitting on those addresses.

Treat production deployments accordingly:

- Store the mnemonic in a secret manager or HSM/KMS — not in plain files on disk or in git
- Plan key rotation before you accumulate significant balances
- Always set a strong `API_SECRET` in production
- Never log the mnemonic; keep `.env` out of version control

[`.env.example`](.env.example) uses only the well-known BIP39 test vector (`abandon … about`). Do not reuse it with real funds.

Scope today: inbound USDT deposit detection and webhooks. This is not a multi-tenant or non-custodial vault.

To report a vulnerability, use [GitHub Security Advisories](https://github.com/nejos97/stablecoin-payment-gateway/security/advisories/new) rather than a public issue when possible.

## Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `API_SECRET` | No | — | If set (≥8 chars), requires `X-API-SECRET` header |
| `WALLET_MNEMONIC` | Yes | — | BIP39 mnemonic (12 or 24 words) |
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `REDIS_URL` | Yes | — | Redis connection string |
| `WEBHOOK_CALLBACK_URL` | No | — | Global webhook URL |
| `TRONGRID_API_KEY` | No* | — | TronGrid API key |
| `ALCHEMY_API_KEY` | No* | — | Alchemy API key (Ethereum) |
| `HELIUS_API_KEY` | No* | — | Helius API key (Solana) |
| `TRONGRID_BASE_URL` | No | `https://api.trongrid.io` | TronGrid API base URL |
| `ETH_RPC_BASE_URL` | No | `https://eth-mainnet.g.alchemy.com/v2` | Ethereum RPC base (API key appended) |
| `HELIUS_RPC_BASE_URL` | No | `https://mainnet.helius-rpc.com` | Helius Solana RPC base |
| `HELIUS_API_BASE_URL` | No | `https://api.helius.xyz` | Helius REST API base |
| `APP_ENV` | No | `development` | Redis key prefix / environment tag |
| `PORT` | No | `3000` | HTTP listen port |
| `LOG_LEVEL` | No | `info` | Tracing log level |

\* Required for monitoring on the respective network.

Advanced overrides (code defaults if unset): `DEPOSIT_EXPIRY_MINUTES` (30), `POLLING_INTERVAL_SECONDS` (10), `ETH_CONFIRMATIONS` (12), `TRON_CONFIRMATIONS` (19), `AMOUNT_TOLERANCE_PERCENT` (1).

## USDT contracts

| Network | Contract |
|---|---|
| Ethereum | `0xdAC17F958D2ee523a2206206994597C13D831ec7` |
| Tron | `TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t` |
| Solana | `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB` |
