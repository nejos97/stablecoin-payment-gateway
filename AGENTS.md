# AGENTS.md

Guidance for AI coding agents working in this repository.

## What this project is

A Rust service that accepts **USDT deposits** on **Tron (TRC-20)**, **Ethereum (ERC-20)**, and **Solana (SPL)**. It derives a unique deposit address per payment from a single custodial HD wallet (`WALLET_MNEMONIC`), polls the blockchains for incoming transfers, confirms deposits, and notifies **every active webhook endpoint** (managed in the dashboard, stored in `webhook_endpoints` — no env var). Storage is PostgreSQL (sqlx); Redis backs the webhook queue, cached wallet balances, and staff refresh tokens.

An **admin dashboard** lives in `apps/dashboard` (Vite + React 19 + TypeScript + Tailwind v4 + shadcn/ui): wallet balances, payment/webhook monitoring and retry, API key and staff management. It talks exclusively to `/api/admin/v1` with the acting staff member's JWT (`Authorization: Bearer`). Merchant endpoints under `/api/v1` require an `x-api-key` created from the dashboard.

Scope today: **inbound deposit detection + webhooks + admin dashboard**. No outbound transfers, no multi-tenancy. Webhook endpoints are stored in DB (multiple, each active/inactive); the default payment expiry lives in `app_settings` — both managed from Dashboard → Settings.

## Architecture

Cargo workspace of 9 crates under `crates/` (version, edition, and all dependency versions are set in the root `Cargo.toml` via `[workspace.package]` / `[workspace.dependencies]` — never pin versions in a member crate), plus the dashboard SPA under `apps/dashboard`:

| Crate | Role |
|---|---|
| `config` | `AppConfig::from_env()`, shared constants (USDT contracts, `WEBHOOK_MAX_ATTEMPTS = 5`, decimals), RPC URL builders, Redis key prefixing |
| `domain` | Pure types: `Network`, `Token`, statuses, `StaffRole`, `DomainError`, amount conversion/tolerance helpers. No I/O, no async |
| `wallet` | HD derivation from BIP39 mnemonic. ETH/Tron via BIP44 `m/44'/60'/0'/0/{index}` (secp256k1); Solana via SLIP-0010 `m/44'/501'/{index}'/0'` (ed25519), returning the USDT **associated token account** address |
| `auth` | Pure auth primitives (no I/O, no async): argon2id password hashing, HS256 JWT issue/decode, refresh-token and API-key generation + SHA-256 hashing, constant-time comparison |
| `db` | `Db` wrapper over a `PgPool`, row structs, all SQL queries, sqlx migrations (`crates/db/migrations/`). Split into `lib.rs` (payments/webhooks) + `staff.rs`, `api_keys.rs`, `admin_queries.rs`, `settings.rs`, `webhook_endpoints.rs` |
| `chain` | HTTP clients for TronGrid, Alchemy (Ethereum JSON-RPC), Helius (Solana): transfer polling + USDT balance fetches, Tron rate-limit backoff |
| `jobs` | `AppState` (shared by API + workers), background workers (`spawn_workers`): one monitor loop per network, webhook delivery loop, hourly wallet-balance sync; deposit confirmation logic (`process_detected_transfer`); manual retriggers (`retrigger_webhook`, `retrigger_failed_webhooks`) |
| `api` | Axum router: `/healthz`, `/readyz`, `/api/v1/*` (guarded by `x-api-key`), `/api/admin/v1/*` (guarded by staff JWT; admin-only sub-routes for staff/API keys); `middleware.rs` (guards) + `admin.rs` (dashboard handlers); `ApiError` (`{"statusCode": ..., "message": ...}`) |
| `gateway` | The binary. Wires config → wallet → db (+ migrations) → redis → chain → workers → HTTP server |

Auth model: staff accounts live in `staff_users` (roles `ADMIN` / `OPERATOR`, argon2id hashes, `firstName`/`lastName`); merchant keys in `api_keys` (clear `prefix` for lookup + SHA-256 `keyHash`, revocation via `revokedAt`). Refresh tokens are stored hashed in Redis (`auth:refresh:{sha256}`, TTL `REFRESH_TOKEN_TTL_DAYS`, rotated on every refresh). The first admin is created through the dashboard's `/setup` wizard (guarded insert — only while `staff_users` is empty); there is no env-var seeding. `JWT_SECRET` (≥ 32 chars) is required or `/api/admin/v1` is not mounted.

Dependency direction: `domain` ← everything; `api` and `jobs` sit on top; only `gateway` produces a binary. Put logic in the right layer — e.g. queue/DB logic belongs in `jobs`/`db`, not in API handlers.

## Commands

```bash
# Run locally (needs .env — copy from .env.example; set JWT_SECRET for the admin API)
docker compose up -d postgres redis
cargo run -p gateway

# Dashboard dev server (proxies /api to localhost:3000)
cd apps/dashboard && npm install && npm run dev

# Tests — this is exactly what CI runs; keep it green
cargo test -p domain -p wallet -p config -p jobs -p auth

# Type-check + build the dashboard
cd apps/dashboard && npm run build

# Release build (also done in CI)
cargo build --release -p gateway

# Full stack in Docker: postgres + redis + api (:3000) + dashboard (:8080)
docker compose up --build
```

Migrations run automatically at boot via `sqlx::migrate!` (failure is logged as a warning, not fatal). There are **no integration tests** for `api`/`chain`/`jobs` yet — unit tests live only in `domain` and `wallet` (BIP39/HD golden vectors, amount tolerance, enum roundtrips). If you change derivation logic, the golden-vector tests in `crates/wallet/src/lib.rs` must still pass byte-for-byte.

## Conventions

- **DB naming is Prisma-style**: tables snake_case, columns **camelCase** and quoted (`"derivationIndex"`, `"expiresAt"`), Postgres enums PascalCase (`"Network"`). Rust row structs map them with `#[sqlx(rename = "...")]`. New SQL must follow this style, and enum values must be cast (`$1::"Network"`).
- **Three casings for enums** — keep them straight: DB values are UPPERCASE (`TRON`, `PENDING`), API strings are lowercase (`tron`, `pending`). Always convert through the `as_db` / `as_api` / `from_db` / `parse_api` methods in `domain`; never hand-write status strings.
- **Amounts**: `amount_raw` is the integer string in token base units (USDT = 6 decimals); `amount` is a `Decimal`. Convert with `domain::raw_to_decimal*` — never through floats. Matching against `expected_amount` uses `is_amount_within_tolerance` (`AMOUNT_TOLERANCE_PERCENT`, default 1%).
- **Redis keys** are always built via `config.redis_key(suffix)` → `stablecoin-payment-service-{APP_ENV}:{suffix}`. Never hardcode a full key.
- **Config**: all tuning goes through env vars read in `crates/config/src/lib.rs` (with defaults) and documented in the README table. Missing provider API keys must *disable* the corresponding network with a startup warning, not crash.
- Errors: `anyhow` at boundaries, `thiserror` enums inside `domain`/`wallet`. API errors go through `ApiError` so the JSON shape stays `{"statusCode", "message"}`.
- Logging via `tracing`. **Never log the mnemonic or provider API keys.**

## Domain gotchas (easy to get wrong)

- **ID confusion in webhooks**: the webhook JSON field `data.deposit_id` is actually `deposit_addresses.id`, while the Redis queue and `webhook_deliveries.depositId` hold `deposits.id`. This is intentional and load-bearing for existing receivers — do not "fix" the payload.
- **Per-endpoint deliveries**: each delivery is tracked per (deposit, endpoint) pair; the authoritative row is the newest per pair (`DISTINCT ON ("depositId", "webhookEndpointId")`). Rows with a NULL `webhookEndpointId` are pre-multi-endpoint history. Redis ZSET members are `"{deposit_id}|{endpoint_id}"`; a bare member (no `|`) is legacy and gets fanned out to all active endpoints by `webhook_loop`. No active endpoints = webhooks silently disabled (503 only on manual retry).
- **Admin-managed settings**: default payment expiry is `app_settings.depositExpiryMinutes` (default 60, max 1440), not an env var.
- **API key format**: `[{tag}_]{8 alnum}_{40 alnum}`. The tag is `app_settings.apiKeyPrefix` (≤ 5 alphanumeric chars, empty = no tag) and only affects newly generated keys. `auth::parse_api_key_prefix` is deliberately tag-agnostic (splits on the last `_`) so keys issued under any past tag — including the historic `spg` — keep validating after the setting changes. Never re-tighten it to a specific tag.
- **API key expiry**: `api_keys.expiresAt` (NULL = never) with status machine `ACTIVE`/`REVOKED`/`EXPIRED`. Enforcement is two-level: `find_api_key_by_prefix` rejects past-deadline keys immediately (the security boundary), while `api_key_expiry_loop` (5 min, first run at boot) only materializes `EXPIRED` for listings/stats. List handlers derive the displayed status so a key can never show `active` after its deadline, even between sweeps.
- **Confirmation thresholds**: Tron 19, Ethereum 12 (env-overridable), Solana hardcoded to 1 (`domain::required_confirmations`).
- **Webhook delivery**: max 5 attempts (`WEBHOOK_MAX_ATTEMPTS`), ~2s loop, 10s HTTP timeout, no exponential backoff, only the *last* response/error is stored. A deposit's webhook is enqueued once at confirmation; already-`DELIVERED` deposits are skipped.
- Two independent status machines: deposit (`DETECTED` → `CONFIRMED`) tracks on-chain state; webhook (`PENDING`/`DELIVERED`/`FAILED`) tracks HTTP notification state. Never let one mutate the other.
- `deposits.txHash` is globally unique; derivation indices are allocated per network as `max + 1`.
- Polling loops sleep `ADDRESS_REQUEST_DELAY_MS` (300ms) between addresses to respect provider rate limits; TronGrid additionally has an explicit backoff on 429s in `chain`.

## Specs in `docs/`

Files in `docs/` may be full implementation specs. If a task relates to one, **read the whole spec first and follow it exactly**, including its "out of scope" list.

## Security rules

This is a **custodial hot-wallet** service — treat everything key-related as sensitive:

- Never commit `.env`, real mnemonics, API keys, or production credentials. `.env.example` only uses the well-known BIP39 test vector (`abandon … about`).
- Address derivation must remain deterministic — changing paths or hashing breaks recovery of funds on already-issued addresses.

## Git / CI / releases

- CI (`.github/workflows/ci.yml`) runs on every push/PR: `cargo test -p domain -p wallet` then `cargo build --release -p gateway`.
- Releases are **manual** via the "New App Version" workflow (`workflow_dispatch`): it bumps the workspace version with `cargo set-version`, commits `chore: release vX.Y.Z`, tags, and publishes the Docker image. Never bump the version or create tags by hand.
- Do not commit or push unless the user asks. Keep PRs scoped to one concern; add tests when changing `domain` or `wallet` logic.
