use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use domain::{
    DepositAddressStatus, DepositStatus, Network, Token, WebhookDeliveryStatus,
};
use rust_decimal::Decimal;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use uuid::Uuid;

#[derive(Clone)]
pub struct Db {
    pool: PgPool,
}

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
    pub status: String,
    pub attempts: i32,
    #[sqlx(rename = "lastResponse")]
    pub last_response: Option<i32>,
    #[sqlx(rename = "lastError")]
    pub last_error: Option<String>,
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
        let row = sqlx::query_as::<_, DepositAddressRow>(
            r#"
            INSERT INTO deposit_addresses
              (id, network, token, address, "derivationIndex", "expectedAmount", status, "expiresAt", metadata, "createdAt", "updatedAt")
            VALUES
              ($1, $2::"Network", $3::"Token", $4, $5, $6, 'PENDING'::"DepositAddressStatus", $7, $8, $9, $9)
            RETURNING *
            "#,
        )
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

        let rows = sqlx::query_as::<_, DepositAddressRow>(
            r#"
            SELECT * FROM deposit_addresses
            WHERE ($1::text IS NULL OR status = $1::"DepositAddressStatus")
              AND ($2::text IS NULL OR network = $2::"Network")
            ORDER BY "createdAt" DESC
            LIMIT $3 OFFSET $4
            "#,
        )
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
        let row = sqlx::query_as::<_, DepositAddressRow>(
            r#"SELECT * FROM deposit_addresses WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_deposits_for_address(&self, address_id: &str) -> Result<Vec<DepositRow>> {
        let rows = sqlx::query_as::<_, DepositRow>(
            r#"SELECT * FROM deposits WHERE "depositAddressId" = $1 ORDER BY "detectedAt" DESC"#,
        )
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
        let rows = sqlx::query_as::<_, DepositAddressRow>(
            r#"
            SELECT * FROM deposit_addresses
            WHERE network = $1::"Network"
              AND status = 'PENDING'::"DepositAddressStatus"
              AND "expiresAt" > NOW()
            "#,
        )
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
        let row = sqlx::query_as::<_, DepositRow>(
            r#"SELECT * FROM deposits WHERE "txHash" = $1"#,
        )
        .bind(tx_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_deposit(&self, id: &str) -> Result<Option<DepositRow>> {
        let row = sqlx::query_as::<_, DepositRow>(r#"SELECT * FROM deposits WHERE id = $1"#)
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

        let existing = sqlx::query_as::<_, DepositRow>(
            r#"SELECT * FROM deposits WHERE "txHash" = $1 FOR UPDATE"#,
        )
        .bind(tx_hash)
        .fetch_optional(&mut *tx)
        .await?;

        let deposit = if let Some(existing) = existing {
            let confirmed_at = if status == DepositStatus::Confirmed {
                Some(Utc::now())
            } else {
                existing.confirmed_at
            };
            sqlx::query_as::<_, DepositRow>(
                r#"
                UPDATE deposits
                SET confirmations = $2,
                    status = $3::"DepositStatus",
                    "confirmedAt" = $4
                WHERE id = $1
                RETURNING *
                "#,
            )
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
            sqlx::query_as::<_, DepositRow>(
                r#"
                INSERT INTO deposits
                  (id, "depositAddressId", "txHash", amount, "amountRaw", confirmations, status, "detectedAt", "confirmedAt")
                VALUES
                  ($1, $2, $3, $4, $5, $6, $7::"DepositStatus", NOW(), $8)
                RETURNING *
                "#,
            )
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

    pub async fn has_delivered_webhook(&self, deposit_id: &str) -> Result<bool> {
        let row: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT COUNT(*) FROM webhook_deliveries
            WHERE "depositId" = $1 AND status = 'DELIVERED'::"WebhookDeliveryStatus"
            "#,
        )
        .bind(deposit_id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|(c,)| c > 0).unwrap_or(false))
    }

    pub async fn get_or_create_webhook_delivery(
        &self,
        deposit_id: &str,
    ) -> Result<WebhookDeliveryRow> {
        if let Some(row) = sqlx::query_as::<_, WebhookDeliveryRow>(
            r#"SELECT * FROM webhook_deliveries WHERE "depositId" = $1 ORDER BY "createdAt" DESC LIMIT 1"#,
        )
        .bind(deposit_id)
        .fetch_optional(&self.pool)
        .await?
        {
            return Ok(row);
        }

        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, WebhookDeliveryRow>(
            r#"
            INSERT INTO webhook_deliveries
              (id, "depositId", status, attempts, "createdAt", "updatedAt")
            VALUES
              ($1, $2, 'PENDING'::"WebhookDeliveryStatus", 0, $3, $3)
            RETURNING id, "depositId", status, attempts, "lastResponse", "lastError"
            "#,
        )
        .bind(&id)
        .bind(deposit_id)
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
