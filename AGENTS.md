# AGENTS.md

Guidance for AI coding agents working in this repository.

## What this project is

A Rust service that accepts **USDT deposits** on **Tron (TRC-20)**, **Ethereum (ERC-20)**, and **Solana (SPL)**. It derives a unique deposit address per payment from a single custodial HD wallet (`WALLET_MNEMONIC`), polls the blockchains for incoming transfers, confirms deposits, and notifies a global `WEBHOOK_CALLBACK_URL`. Storage is PostgreSQL (sqlx); Redis backs the webhook queue and cached wallet balances.

Scope today: **inbound deposit detection + webhooks only**. No outbound transfers, no multi-tenancy, single global webhook URL.

## Architecture

Cargo workspace of 8 crates under `crates/` (version, edition, and all dependency versions are set in the root `Cargo.toml` via `[workspace.package]` / `[workspace.dependencies]` — never pin versions in a member crate):

| Crate | Role |
|---|---|
| `config` | `AppConfig::from_env()`, shared constants (USDT contracts, `WEBHOOK_MAX_ATTEMPTS = 5`, decimals), RPC URL builders, Redis key prefixing |
| `domain` | Pure types: `Network`, `Token`, statuses, `DomainError`, amount conversion/tolerance helpers. No I/O, no async |
| `wallet` | HD derivation from BIP39 mnemonic. ETH/Tron via BIP44 `m/44'/60'/0'/0/{index}` (secp256k1); Solana via SLIP-0010 `m/44'/501'/{index}'/0'` (ed25519), returning the USDT **associated token account** address |
| `db` | `Db` wrapper over a `PgPool`, row structs, all SQL queries, sqlx migrations (`crates/db/migrations/`) |
| `chain` | HTTP clients for TronGrid, Alchemy (Ethereum JSON-RPC), Helius (Solana): transfer polling + USDT balance fetches, Tron rate-limit backoff |
| `jobs` | `AppState` (shared by API + workers), background workers (`spawn_workers`): one monitor loop per network, webhook delivery loop, hourly wallet-balance sync; deposit confirmation logic (`process_detected_transfer`) |
| `api` | Axum router: `/healthz`, `/readyz`, and `/api/v1/*` endpoints; `ApiError` (`{"statusCode": ..., "message": ...}`) |
| `gateway` | The binary. Wires config → wallet → db (+ migrations) → redis → chain → workers → HTTP server |

Dependency direction: `domain` ← everything; `api` and `jobs` sit on top; only `gateway` produces a binary. Put logic in the right layer — e.g. queue/DB logic belongs in `jobs`/`db`, not in API handlers.

## Commands

```bash
# Run locally (needs .env — copy from .env.example)
docker compose up -d postgres redis
cargo run -p gateway

# Tests — this is exactly what CI runs; keep it green
cargo test -p domain -p wallet

# Release build (also done in CI)
cargo build --release -p gateway
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
