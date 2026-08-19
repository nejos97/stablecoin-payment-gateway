# Stablecoin Payment Service

[![CI](https://github.com/nejos97/stablecoin-payment-gateway/actions/workflows/ci.yml/badge.svg)](https://github.com/nejos97/stablecoin-payment-gateway/actions/workflows/ci.yml)

Rust service for accepting USDT deposits on **Tron**, **Ethereum**, and **Solana**.

## Features

- Create unique deposit addresses per payment (`POST /api/v1/deposit-addresses`)
- Monitor blockchain transfers and confirm deposits
- Send webhooks for payment lifecycle events — `pending` (address created), `paid`
  (deposit confirmed), `expired` — to every active endpoint **subscribed to that event**;
  endpoints are managed from the dashboard (**Webhooks**, admin), with per-endpoint delivery state
- HD wallet derivation from `WALLET_MNEMONIC`
- Multi-network support: Tron (TRC-20), Ethereum (ERC-20), Solana (SPL)
- **Admin dashboard** (`apps/dashboard`, React + shadcn/ui): wallet balances, payments,
  webhook monitoring/retry, API key and staff management — served on its own port
- Merchant API protected by **API keys** (`x-api-key`) created from the dashboard;
  admin API (`/api/admin/v1`) protected by staff **JWT** (roles: admin / operator)

## Quick start

```bash
cp .env.example .env
# Edit .env with your secrets and API keys

docker compose up -d postgres redis
cargo run -p gateway
```

On first boot the binary applies SQL migrations (`crates/db/migrations`) via sqlx.

API: `http://localhost:3000` — `GET /healthz` (liveness), `GET /readyz` (Postgres + Redis)

### Dashboard (dev)

```bash
cd apps/dashboard
npm install
npm run dev   # http://localhost:5173, proxies /api to localhost:3000
```

Set `JWT_SECRET` (min 32 chars) in `.env` first — without it the admin API is disabled.
On first visit the dashboard walks you through creating the initial admin account
(first name, last name, email, password). Admins can then add staff (role `admin` or
`operator`), create API keys, generate deposit addresses, and monitor/retry webhooks.

### Docker

```bash
docker compose up --build
```

Starts postgres, redis, the API on `:3000` and the dashboard on `:8080`
(nginx serving the built SPA, proxying `/api` to the api service — no CORS needed).

## Testing

Unit tests live in the `domain`, `wallet`, `config`, and `jobs` crates (BIP39 / HD address golden vectors, amount tolerance, enum roundtrips, webhook retry schedule). CI runs the same suite:

```bash
cargo test -p domain -p wallet -p config -p jobs -p auth
```

There are no integration tests for the API or chain providers yet.

## API

> **Breaking change:** every `/api/v1` endpoint now requires an API key sent as
> `x-api-key: ...`. Create one from the dashboard (**API Keys → New API key**)
> before updating your integrations. `/healthz`, `/readyz` and `/api/admin/v1`
> are not affected. Keys are `[{tag}_]{8}_{40}` — the optional tag (max 5
> alphanumeric chars) is configurable in **Settings → API key prefix**; changing
> it only affects newly created keys, existing keys keep working.

### Create deposit address

```bash
curl -X POST http://localhost:3000/api/v1/deposit-addresses \
  -H "Content-Type: application/json" \
  -H "x-api-key: $API_KEY" \
  -d '{
    "network": "tron",
    "token": "USDT",
    "expected_amount": "25.00"
  }'
```

### List deposit addresses

```bash
curl "http://localhost:3000/api/v1/deposit-addresses?status=pending&limit=20"
```

Query params (optional):

- `status`: `pending`, `paid`, `expired`
- `network`: `tron`, `solana`, `ethereum`
- `limit`: default `50`, max `100`
- `offset`: default `0`

### Get deposit address status

```bash
curl http://localhost:3000/api/v1/deposit-addresses/<id>
```

Each entry in the returned `deposits[]` includes a `webhook` object with the delivery state (`status`: `pending` / `delivered` / `failed`, `attempts`, `last_response`, `last_error`, `updated_at`), or `null` if no delivery was attempted yet.

### Retry a webhook delivery

```bash
curl -X POST http://localhost:3000/api/v1/deposits/<deposit_id>/webhooks/retry
```

`<deposit_id>` is the `id` of an entry in `deposits[]` above. Retry resets the delivery attempts and re-queues the webhook towards **every active endpoint**; the payload is unchanged and the deposit status is unchanged. Errors: `404` unknown deposit, `400` deposit not confirmed, `503` no active webhook endpoints configured.

### Get USDT balances (deposit addresses in database)

```bash
curl http://localhost:3000/api/v1/balances
```

### Get HD wallet balances (cached, synced hourly)

```bash
curl http://localhost:3000/api/v1/wallet-balances
```

All the `curl` examples above also need `-H "x-api-key: $API_KEY"`.

## Admin API (`/api/admin/v1`)

Used exclusively by the dashboard; every request (except setup/login/refresh) carries
the acting staff member's `Authorization: Bearer <access_token>`. Requires `JWT_SECRET`.

| Method | Path | Access | Description |
|---|---|---|---|
| GET/POST | `/setup` | public | First-run check / create the initial admin (only while no staff exists) |
| POST | `/auth/login`, `/auth/refresh` | public | Email+password login; refresh-token rotation |
| POST | `/auth/logout` · GET `/auth/me` | JWT | Session management |
| GET/POST/PATCH | `/staff`, `/staff/{id}` | admin | List/create/update staff (roles `admin` / `operator`, activation, password) |
| GET/POST/DELETE | `/api-keys`, `/api-keys/{id}` | admin | List/create/revoke merchant API keys (secret shown once; optional `expires_in_days` 30/90/180/365, omit for no expiry — expired keys get status `expired` and stop working) |
| GET/PATCH | `/settings` | admin | Read/update system settings (`deposit_expiry_minutes` 1–1440, `api_key_prefix` ≤ 5 alphanum; PATCH is partial) |
| GET/POST/PATCH/DELETE | `/webhook-endpoints`, `/webhook-endpoints/{id}` | admin | Manage outgoing webhook endpoints (subscribed `events` — `pending`/`paid`/`expired` — activate/deactivate; delete only when unused) |
| GET | `/webhooks?status=&limit=&offset=` | JWT | Latest delivery per event key, joined with deposit (if any) + address |
| POST | `/webhooks/retry-failed` | JWT | Bulk-requeue all failed deliveries (paid ones only once their deposit is confirmed) |
| POST | `/webhook-deliveries/{id}/retry` | JWT | Replay one delivery row (works for every event type) |
| GET | `/stats` | JWT | Counters (addresses/deposits/webhooks by status) + confirmed volume |
| — | `/deposit-addresses*`, `/deposits/{id}/webhooks/retry`, `/balances`, `/wallet-balances` | JWT | Same handlers as `/api/v1`, JWT-authenticated for the dashboard |

## Webhook events & payloads

Webhook endpoints are managed in Dashboard → Webhooks (admin). Each endpoint subscribes to one or more payment events and only receives those:

| Event | Fired when | `event_type` in payload |
|---|---|---|
| `pending` | A deposit address is created | `payment_pending` |
| `paid` | A deposit is confirmed on-chain | `deposit_confirmed` |
| `expired` | A pending address passes its expiry unpaid | `payment_expired` |

Payload on confirmed deposit (`paid`, unchanged historical shape):

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

Payload for `pending` / `expired` (no deposit yet — `deposit_id` is the deposit-address id, like above):

```json
{
  "event_type": "payment_pending",
  "data": {
    "deposit_id": "uuid",
    "address": "T...",
    "network": "tron",
    "token": "USDT",
    "expected_amount": "25",
    "status": "pending",
    "expires_at": "2026-08-19T12:00:00+00:00",
    "created_at": "2026-08-19T11:00:00+00:00"
  }
}
```

### Signature (`X-Webhook-Signature`)

Each endpoint can optionally have a **signing secret** (set at creation or rotated later in Dashboard → Webhooks; write-only — the API only ever reports `has_secret`). When set, every delivery carries:

```text
X-Webhook-Signature: sha256=<hex(HMAC-SHA256(secret, raw_body))>
```

Verify it against the **raw request body** (before any JSON parsing), using a constant-time comparison:

```js
const crypto = require("node:crypto")

function verify(rawBody, header, secret) {
  const expected = "sha256=" +
    crypto.createHmac("sha256", secret).update(rawBody).digest("hex")
  return header.length === expected.length &&
    crypto.timingSafeEqual(Buffer.from(header), Buffer.from(expected))
}
```

No secret configured → no header. The signature covers the body only (no timestamp): if replay matters to you, deduplicate on `(event_type, data.deposit_id, data.tx_hash)` on your side.

If no active webhook endpoint is configured, a warning is logged at startup and webhooks are disabled. Each delivery is tracked per (deposit, endpoint) pair for `paid` and per (address, event, endpoint) for `pending`/`expired`; `deposits[].webhooks[]` in the deposit-address detail exposes the per-endpoint state, while `deposits[].webhook` keeps the historical single-object shape (most recent delivery).

Delivery is attempted up to 5 times with increasing gaps — 90s, 4m30, 13m30, 40m30 — so the 5th and final attempt starts ~1h after the first. After that the delivery is marked `failed` and can only be resent via the retry endpoint. Receivers must treat replays as the same event (idempotent on their side).

## Security considerations

This service uses a **custodial hot wallet**: a single operator `WALLET_MNEMONIC` is loaded at boot and used to derive all deposit addresses (HD paths per network). Compromising the host or the environment secret means compromising the entire derivation chain and any funds sitting on those addresses.

Treat production deployments accordingly:

- Store the mnemonic in a secret manager or HSM/KMS — not in plain files on disk or in git
- Plan key rotation before you accumulate significant balances
- Never log the mnemonic; keep `.env` out of version control

[`.env.example`](.env.example) uses only the well-known BIP39 test vector (`abandon … about`). Do not reuse it with real funds.

Scope today: inbound USDT deposit detection and webhooks. This is not a multi-tenant or non-custodial vault.

To report a vulnerability, use [GitHub Security Advisories](https://github.com/nejos97/stablecoin-payment-gateway/security/advisories/new) rather than a public issue when possible.

## Environment variables

| Variable | Required | Default | Description |
|---|---|---|---|
| `WALLET_MNEMONIC` | Yes | — | BIP39 mnemonic (12 or 24 words) |
| `DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `REDIS_URL` | Yes | — | Redis connection string |
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
| `JWT_SECRET` | No** | — | Secret for staff JWTs (min 32 chars); unset disables `/api/admin/v1` |
| `ACCESS_TOKEN_TTL_MINUTES` | No | `15` | Staff access-token lifetime |
| `REFRESH_TOKEN_TTL_DAYS` | No | `30` | Staff refresh-token lifetime (stored in Redis, rotated) |
| `DASHBOARD_PORT` | No | `8080` | Host port of the dashboard container (docker compose) |
| `CORS_ALLOWED_ORIGINS` | No | — | Comma-separated origins; only needed if the dashboard is not same-origin |

\* Required for monitoring on the respective network.
\** Required for the admin API and the dashboard.

Advanced overrides (code defaults if unset): `POLLING_INTERVAL_SECONDS` (10), `ETH_CONFIRMATIONS` (12), `TRON_CONFIRMATIONS` (19), `AMOUNT_TOLERANCE_PERCENT` (1).

Webhook endpoints (Dashboard → Webhooks) and the default payment expiry (60 min, max 24 h; Dashboard → Settings) are **not** environment variables — they are stored in the database and managed from the dashboard (admin only).

## USDT contracts

| Network | Contract |
|---|---|
| Ethereum | `0xdAC17F958D2ee523a2206206994597C13D831ec7` |
| Tron | `TR7NHqjeKQxGTCi8q8ZY4pL8otSzgjLj6t` |
| Solana | `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB` |
