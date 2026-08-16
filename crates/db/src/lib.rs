use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use domain::{
    DepositAddressStatus, DepositStatus, Network, Token, WebhookDeliveryStatus,
};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use uuid::Uuid;

mod admin_queries;
mod api_keys;
mod settings;
mod staff;
mod webhook_endpoints;

pub use admin_queries::{AdminStats, DailyDepositStat, WebhookDeliveryDetailRow};
pub use api_keys::{ApiKeyListRow, ApiKeyRow};
pub use settings::{API_KEY_PREFIX_SETTING, DEPOSIT_EXPIRY_SETTING};
pub use staff::StaffUserRow;
pub use webhook_endpoints::{WebhookEndpointListRow, WebhookEndpointRow};

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

// sqlx refuses to decode a Postgres enum column into a Rust String, and a
// TIMESTAMP(3) column into DateTime<Utc>, so every query that materializes a
// row must cast enums to text and timestamps to timestamptz (stored values
// are UTC instants) instead of using SELECT * / RETURNING *.
const DEPOSIT_ADDRESS_COLUMNS: &str = r#"id, network::text AS network, token::text AS token, address, "derivationIndex", "expectedAmount", status::text AS status, "expiresAt" AT TIME ZONE 'UTC' AS "expiresAt", metadata, "createdAt" AT TIME ZONE 'UTC' AS "createdAt", "updatedAt" AT TIME ZONE 'UTC' AS "updatedAt""#;
const DEPOSIT_COLUMNS: &str = r#"id, "depositAddressId", "txHash", amount, "amountRaw", confirmations, status::text AS status, "detectedAt" AT TIME ZONE 'UTC' AS "detectedAt", "confirmedAt" AT TIME ZONE 'UTC' AS "confirmedAt""#;
const WEBHOOK_DELIVERY_COLUMNS: &str = r#"id, "depositId", "webhookEndpointId", status::text AS status, attempts, "lastResponse", "lastError", "createdAt" AT TIME ZONE 'UTC' AS "createdAt", "updatedAt" AT TIME ZONE 'UTC' AS "updatedAt""#;

#[derive(Debug, Clone, FromRow)]
pub struct DepositAddressRow {
    pub id: String,
    pub network: String,
    pub token: String,
    pub address: String,
    #[sqlx(rename = "derivationIndex")]
    pub derivation_index: i32,
    #[sqlx(rename = "expectedAmount")]
    pub expected_amount: Decimal,
    pub status: String,
    #[sqlx(rename = "expiresAt")]
    pub expires_at: DateTime<Utc>,
    pub metadata: Value,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow)]
pub struct DepositRow {
    pub id: String,
    #[sqlx(rename = "depositAddressId")]
    pub deposit_address_id: String,
    #[sqlx(rename = "txHash")]
    pub tx_hash: String,
    pub amount: Decimal,
    #[sqlx(rename = "amountRaw")]
    pub amount_raw: String,
    pub confirmations: i32,
    pub status: String,
    #[sqlx(rename = "detectedAt")]
    pub detected_at: DateTime<Utc>,
    #[sqlx(rename = "confirmedAt")]
    pub confirmed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, FromRow)]
pub struct WebhookDeliveryRow {
    pub id: String,
    #[sqlx(rename = "depositId")]
    pub deposit_id: String,
    /// NULL on rows written before per-endpoint deliveries existed.
    #[sqlx(rename = "webhookEndpointId")]
    pub webhook_endpoint_id: Option<String>,
    pub status: String,
    pub attempts: i32,
    #[sqlx(rename = "lastResponse")]
    pub last_response: Option<i32>,
    #[sqlx(rename = "lastError")]
    pub last_error: Option<String>,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

impl Db {
    pub async fn connect(database_url: &str) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await
            .context("Failed to connect to PostgreSQL")?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("Failed to run database migrations")?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ping(&self) -> Result<()> {
        sqlx::query("SELECT 1")
            .execute(&self.pool)
            .await
            .context("PostgreSQL ping failed")?;
        Ok(())
    }

    pub async fn next_derivation_index(&self, network: Network) -> Result<i32> {
        let row: Option<(i32,)> = sqlx::query_as(
            r#"SELECT "derivationIndex" FROM deposit_addresses WHERE network = $1::"Network" ORDER BY "derivationIndex" DESC LIMIT 1"#,
        )
        .bind(network.as_db())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(i,)| i + 1).unwrap_or(0))
    }

    pub async fn max_derivation_index(&self, network: Network) -> Result<i32> {
        let row: Option<(i32,)> = sqlx::query_as(
            r#"SELECT "derivationIndex" FROM deposit_addresses WHERE network = $1::"Network" ORDER BY "derivationIndex" DESC LIMIT 1"#,
        )
        .bind(network.as_db())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(i,)| i).unwrap_or(0))
    }

    pub async fn create_deposit_address(
        &self,
        network: Network,
        token: Token,
        address: &str,
        derivation_index: i32,
        expected_amount: Decimal,
        expires_at: DateTime<Utc>,
        metadata: Value,
    ) -> Result<DepositAddressRow> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, DepositAddressRow>(&format!(
            r#"
            INSERT INTO deposit_addresses
              (id, network, token, address, "derivationIndex", "expectedAmount", status, "expiresAt", metadata, "createdAt", "updatedAt")
            VALUES
              ($1, $2::"Network", $3::"Token", $4, $5, $6, 'PENDING'::"DepositAddressStatus", $7, $8, $9, $9)
            RETURNING {DEPOSIT_ADDRESS_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(network.as_db())
        .bind(token.as_db())
        .bind(address)
        .bind(derivation_index)
        .bind(expected_amount)
        .bind(expires_at)
        .bind(metadata)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_deposit_addresses(
        &self,
        status: Option<DepositAddressStatus>,
        network: Option<Network>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<DepositAddressRow>, i64)> {
        let status_db = status.map(|s| s.as_db().to_string());
        let network_db = network.map(|n| n.as_db().to_string());

        let rows = sqlx::query_as::<_, DepositAddressRow>(&format!(
            r#"
            SELECT {DEPOSIT_ADDRESS_COLUMNS} FROM deposit_addresses
            WHERE ($1::text IS NULL OR status = $1::"DepositAddressStatus")
              AND ($2::text IS NULL OR network = $2::"Network")
            ORDER BY "createdAt" DESC
            LIMIT $3 OFFSET $4
            "#
        ))
        .bind(status_db.as_deref())
        .bind(network_db.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let (total,): (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM deposit_addresses
            WHERE ($1::text IS NULL OR status = $1::"DepositAddressStatus")
              AND ($2::text IS NULL OR network = $2::"Network")
            "#,
        )
        .bind(status_db.as_deref())
        .bind(network_db.as_deref())
        .fetch_one(&self.pool)
        .await?;

        Ok((rows, total))
    }

    pub async fn find_deposit_address(&self, id: &str) -> Result<Option<DepositAddressRow>> {
        let row = sqlx::query_as::<_, DepositAddressRow>(&format!(
            "SELECT {DEPOSIT_ADDRESS_COLUMNS} FROM deposit_addresses WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_deposits_for_address(&self, address_id: &str) -> Result<Vec<DepositRow>> {
        let rows = sqlx::query_as::<_, DepositRow>(&format!(
            r#"SELECT {DEPOSIT_COLUMNS} FROM deposits WHERE "depositAddressId" = $1 ORDER BY "detectedAt" DESC"#
        ))
        .bind(address_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn expire_stale_addresses(&self) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE deposit_addresses
            SET status = 'EXPIRED'::"DepositAddressStatus", "updatedAt" = NOW()
            WHERE status = 'PENDING'::"DepositAddressStatus" AND "expiresAt" < NOW()
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn pending_addresses(&self, network: Network) -> Result<Vec<DepositAddressRow>> {
        let rows = sqlx::query_as::<_, DepositAddressRow>(&format!(
            r#"
            SELECT {DEPOSIT_ADDRESS_COLUMNS} FROM deposit_addresses
            WHERE network = $1::"Network"
              AND status = 'PENDING'::"DepositAddressStatus"
              AND "expiresAt" > NOW()
            "#
        ))
        .bind(network.as_db())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn addresses_for_network(&self, network: Network) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            r#"SELECT DISTINCT address FROM deposit_addresses WHERE network = $1::"Network""#,
        )
        .bind(network.as_db())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(|(a,)| a).collect())
    }

    pub async fn find_deposit_by_tx(&self, tx_hash: &str) -> Result<Option<DepositRow>> {
        let row = sqlx::query_as::<_, DepositRow>(&format!(
            r#"SELECT {DEPOSIT_COLUMNS} FROM deposits WHERE "txHash" = $1"#
        ))
        .bind(tx_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_deposit(&self, id: &str) -> Result<Option<DepositRow>> {
        let row = sqlx::query_as::<_, DepositRow>(&format!(
            "SELECT {DEPOSIT_COLUMNS} FROM deposits WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn upsert_deposit_and_maybe_pay(
        &self,
        deposit_address_id: &str,
        tx_hash: &str,
        amount: Decimal,
        amount_raw: &str,
        confirmations: i32,
        status: DepositStatus,
        mark_paid: bool,
    ) -> Result<DepositRow> {
        let mut tx = self.pool.begin().await?;

        let existing = sqlx::query_as::<_, DepositRow>(&format!(
            r#"SELECT {DEPOSIT_COLUMNS} FROM deposits WHERE "txHash" = $1 FOR UPDATE"#
        ))
        .bind(tx_hash)
        .fetch_optional(&mut *tx)
        .await?;

        let deposit = if let Some(existing) = existing {
            let confirmed_at = if status == DepositStatus::Confirmed {
                Some(Utc::now())
            } else {
                existing.confirmed_at
            };
            sqlx::query_as::<_, DepositRow>(&format!(
                r#"
                UPDATE deposits
                SET confirmations = $2,
                    status = $3::"DepositStatus",
                    "confirmedAt" = $4
                WHERE id = $1
                RETURNING {DEPOSIT_COLUMNS}
                "#
            ))
            .bind(&existing.id)
            .bind(confirmations)
            .bind(status.as_db())
            .bind(confirmed_at)
            .fetch_one(&mut *tx)
            .await?
        } else {
            let id = Uuid::new_v4().to_string();
            let confirmed_at = if status == DepositStatus::Confirmed {
                Some(Utc::now())
            } else {
                None
            };
            sqlx::query_as::<_, DepositRow>(&format!(
                r#"
                INSERT INTO deposits
                  (id, "depositAddressId", "txHash", amount, "amountRaw", confirmations, status, "detectedAt", "confirmedAt")
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7::"DepositStatus", NOW(), $8)
                RETURNING {DEPOSIT_COLUMNS}
                "#
            ))
            .bind(&id)
            .bind(deposit_address_id)
            .bind(tx_hash)
            .bind(amount)
            .bind(amount_raw)
            .bind(confirmations)
            .bind(status.as_db())
            .bind(confirmed_at)
            .fetch_one(&mut *tx)
            .await?
        };

        if mark_paid {
            sqlx::query(
                r#"
                UPDATE deposit_addresses
                SET status = 'PAID'::"DepositAddressStatus", "updatedAt" = NOW()
                WHERE id = $1
                "#,
            )
            .bind(deposit_address_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(deposit)
    }

    pub async fn has_delivered_webhook(&self, deposit_id: &str, endpoint_id: &str) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM webhook_deliveries
            WHERE "depositId" = $1
              AND "webhookEndpointId" = $2
              AND status = 'DELIVERED'::"WebhookDeliveryStatus"
            "#,
        )
        .bind(deposit_id)
        .bind(endpoint_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    pub async fn get_or_create_webhook_delivery(
        &self,
        deposit_id: &str,
        endpoint_id: &str,
    ) -> Result<WebhookDeliveryRow> {
        if let Some(row) = self.find_webhook_delivery(deposit_id, endpoint_id).await? {
            return Ok(row);
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, WebhookDeliveryRow>(&format!(
            r#"
            INSERT INTO webhook_deliveries
              (id, "depositId", "webhookEndpointId", status, attempts, "createdAt", "updatedAt")
            VALUES
              ($1, $2, $3, 'PENDING'::"WebhookDeliveryStatus", 0, $4, $4)
            RETURNING {WEBHOOK_DELIVERY_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(deposit_id)
        .bind(endpoint_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Most recent `webhook_deliveries` row for a (deposit, endpoint) pair.
    /// There is no UNIQUE constraint, so several rows may exist; the newest
    /// one is the authoritative one. Legacy rows (NULL endpoint) are excluded.
    pub async fn find_webhook_delivery(
        &self,
        deposit_id: &str,
        endpoint_id: &str,
    ) -> Result<Option<WebhookDeliveryRow>> {
        let row = sqlx::query_as::<_, WebhookDeliveryRow>(&format!(
            r#"SELECT {WEBHOOK_DELIVERY_COLUMNS} FROM webhook_deliveries WHERE "depositId" = $1 AND "webhookEndpointId" = $2 ORDER BY "createdAt" DESC LIMIT 1"#
        ))
        .bind(deposit_id)
        .bind(endpoint_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// All delivery rows for a set of deposits, newest first. Callers keep the
    /// first row seen per deposit to get the authoritative one.
    pub async fn find_webhook_deliveries_for_deposits(
        &self,
        deposit_ids: &[String],
    ) -> Result<Vec<WebhookDeliveryRow>> {
        if deposit_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query_as::<_, WebhookDeliveryRow>(&format!(
            r#"SELECT {WEBHOOK_DELIVERY_COLUMNS} FROM webhook_deliveries WHERE "depositId" = ANY($1) ORDER BY "createdAt" DESC"#
        ))
        .bind(deposit_ids)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Reset the most recent delivery row of a (deposit, endpoint) pair to
    /// PENDING / 0 attempts (creating it if none exists) so a manual
    /// retrigger restarts a full retry cycle. Older rows are left untouched.
    pub async fn reset_webhook_delivery(
        &self,
        deposit_id: &str,
        endpoint_id: &str,
    ) -> Result<WebhookDeliveryRow> {
        if let Some(existing) = self.find_webhook_delivery(deposit_id, endpoint_id).await? {
            let row = sqlx::query_as::<_, WebhookDeliveryRow>(&format!(
                r#"
                UPDATE webhook_deliveries
                SET status = 'PENDING'::"WebhookDeliveryStatus",
                    attempts = 0,
                    "lastResponse" = NULL,
                    "lastError" = NULL,
                    "updatedAt" = NOW()
                WHERE id = $1
                RETURNING {WEBHOOK_DELIVERY_COLUMNS}
                "#
            ))
            .bind(&existing.id)
            .fetch_one(&self.pool)
            .await?;
            return Ok(row);
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, WebhookDeliveryRow>(&format!(
            r#"
            INSERT INTO webhook_deliveries
              (id, "depositId", "webhookEndpointId", status, attempts, "createdAt", "updatedAt")
            VALUES
              ($1, $2, $3, 'PENDING'::"WebhookDeliveryStatus", 0, $4, $4)
            RETURNING {WEBHOOK_DELIVERY_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(deposit_id)
        .bind(endpoint_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn update_webhook_delivery(
        &self,
        id: &str,
        status: WebhookDeliveryStatus,
        attempts: i32,
        last_response: Option<i32>,
        last_error: Option<String>,
    ) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE webhook_deliveries
            SET status = $2::"WebhookDeliveryStatus",
                attempts = $3,
                "lastResponse" = $4,
                "lastError" = $5,
                "updatedAt" = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status.as_db())
        .bind(attempts)
        .bind(last_response)
        .bind(last_error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
