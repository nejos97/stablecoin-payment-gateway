use anyhow::Result;
use chrono::{DateTime, Utc};
use domain::WebhookDeliveryStatus;
use rust_decimal::Decimal;
use sqlx::FromRow;

use crate::Db;

/// Authoritative delivery row (newest per logical key — (deposit, endpoint)
/// for PAID, (address, event, endpoint) for PENDING/EXPIRED) joined with its
/// deposit (if any), address and target endpoint. `deposit_id` is
/// `deposits.id` (what the deposit retry endpoint takes) and is NULL on
/// address-scoped events; `deposit_address_id` is `deposit_addresses.id`
/// (what the webhook payload calls `deposit_id` — intentional, see
/// AGENTS.md). `webhook_endpoint_id`/`endpoint_url` are NULL on legacy rows.
#[derive(Debug, Clone, FromRow)]
pub struct WebhookDeliveryDetailRow {
    pub id: String,
    #[sqlx(rename = "eventType")]
    pub event_type: String,
    #[sqlx(rename = "depositId")]
    pub deposit_id: Option<String>,
    #[sqlx(rename = "webhookEndpointId")]
    pub webhook_endpoint_id: Option<String>,
    #[sqlx(rename = "endpointUrl")]
    pub endpoint_url: Option<String>,
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
    #[sqlx(rename = "txHash")]
    pub tx_hash: Option<String>,
    pub amount: Option<Decimal>,
    #[sqlx(rename = "expectedAmount")]
    pub expected_amount: Decimal,
    #[sqlx(rename = "depositStatus")]
    pub deposit_status: Option<String>,
    #[sqlx(rename = "depositAddressId")]
    pub deposit_address_id: String,
    pub address: String,
    pub network: String,
}

const WEBHOOK_DETAIL_INNER: &str = r#"
    SELECT DISTINCT ON (w."depositAddressId", w."eventType", w."depositId", w."webhookEndpointId")
        w.id, w."eventType", w."depositId", w."webhookEndpointId", e.url AS "endpointUrl",
        w.status::text AS status, w.attempts,
        w."lastResponse", w."lastError",
        w."createdAt" AT TIME ZONE 'UTC' AS "createdAt",
        w."updatedAt" AT TIME ZONE 'UTC' AS "updatedAt",
        d."txHash", d.amount, d.status::text AS "depositStatus",
        a.id AS "depositAddressId", a.address, a.network::text AS network,
        a."expectedAmount"
    FROM webhook_deliveries w
    LEFT JOIN deposits d ON d.id = w."depositId"
    JOIN deposit_addresses a ON a.id = w."depositAddressId"
    LEFT JOIN webhook_endpoints e ON e.id = w."webhookEndpointId"
    ORDER BY w."depositAddressId", w."eventType", w."depositId", w."webhookEndpointId", w."createdAt" DESC
"#;

/// One (delivery key) eligible for a bulk manual retry. `deposit_id` is set
/// on PAID targets; `webhook_endpoint_id` is `None` on legacy rows (callers
/// fan those out to all active endpoints).
#[derive(Debug, Clone, FromRow)]
pub struct FailedWebhookTarget {
    #[sqlx(rename = "eventType")]
    pub event_type: String,
    #[sqlx(rename = "depositId")]
    pub deposit_id: Option<String>,
    #[sqlx(rename = "depositAddressId")]
    pub deposit_address_id: String,
    #[sqlx(rename = "webhookEndpointId")]
    pub webhook_endpoint_id: Option<String>,
}

/// One day of confirmed-deposit activity for the dashboard chart.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DailyDepositStat {
    pub date: String,
    pub count: i64,
    pub volume: Decimal,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct AdminStats {
    pub addresses_pending: i64,
    pub addresses_paid: i64,
    pub addresses_expired: i64,
    pub deposits_detected: i64,
    pub deposits_confirmed: i64,
    pub confirmed_volume: Decimal,
    pub webhooks_pending: i64,
    pub webhooks_delivered: i64,
    pub webhooks_failed: i64,
}

impl Db {
    pub async fn list_webhook_deliveries_detailed(
        &self,
        status: Option<WebhookDeliveryStatus>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<WebhookDeliveryDetailRow>, i64)> {
        let status_db = status.map(|s| s.as_db().to_string());

        let rows = sqlx::query_as::<_, WebhookDeliveryDetailRow>(&format!(
            r#"
            SELECT * FROM ({WEBHOOK_DETAIL_INNER}) t
            WHERE ($1::text IS NULL OR t.status = $1)
            ORDER BY t."updatedAt" DESC
            LIMIT $2 OFFSET $3
            "#
        ))
        .bind(status_db.as_deref())
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let (total,): (i64,) = sqlx::query_as(&format!(
            r#"
            SELECT COUNT(*) FROM ({WEBHOOK_DETAIL_INNER}) t
            WHERE ($1::text IS NULL OR t.status = $1)
            "#
        ))
        .bind(status_db.as_deref())
        .fetch_one(&self.pool)
        .await?;

        Ok((rows, total))
    }

    /// Delivery keys whose authoritative delivery is FAILED — the set
    /// eligible for a bulk manual retry. PAID targets require a CONFIRMED
    /// deposit; inactive endpoints are excluded.
    pub async fn failed_webhook_targets(&self) -> Result<Vec<FailedWebhookTarget>> {
        let rows = sqlx::query_as::<_, FailedWebhookTarget>(
            r#"
            SELECT t."eventType", t."depositId", t."depositAddressId", t."webhookEndpointId" FROM (
                SELECT DISTINCT ON (w."depositAddressId", w."eventType", w."depositId", w."webhookEndpointId")
                    w."eventType", w."depositId", w."depositAddressId", w."webhookEndpointId",
                    w.status::text AS status
                FROM webhook_deliveries w
                LEFT JOIN deposits d ON d.id = w."depositId"
                WHERE w."depositId" IS NULL OR d.status = 'CONFIRMED'::"DepositStatus"
                ORDER BY w."depositAddressId", w."eventType", w."depositId", w."webhookEndpointId", w."createdAt" DESC
            ) t
            LEFT JOIN webhook_endpoints e ON e.id = t."webhookEndpointId"
            WHERE t.status = 'FAILED'
              AND (t."webhookEndpointId" IS NULL OR e."isActive" = true)
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Confirmed deposits per day over the trailing window (days with no
    /// deposits are absent — callers fill the gaps).
    pub async fn deposits_timeseries(&self, days: i32) -> Result<Vec<DailyDepositStat>> {
        let rows: Vec<(String, i64, Decimal)> = sqlx::query_as(
            r#"
            SELECT to_char(date_trunc('day', "confirmedAt"), 'YYYY-MM-DD') AS date,
                   COUNT(*), COALESCE(SUM(amount), 0)
            FROM deposits
            WHERE status = 'CONFIRMED'::"DepositStatus"
              AND "confirmedAt" >= (NOW() AT TIME ZONE 'UTC') - make_interval(days => $1)
            GROUP BY 1
            ORDER BY 1
            "#,
        )
        .bind(days)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|(date, count, volume)| DailyDepositStat { date, count, volume })
            .collect())
    }

    pub async fn admin_stats(&self) -> Result<AdminStats> {
        let mut stats = AdminStats::default();

        let address_counts: Vec<(String, i64)> = sqlx::query_as(
            "SELECT status::text, COUNT(*) FROM deposit_addresses GROUP BY status",
        )
        .fetch_all(&self.pool)
        .await?;
        for (status, count) in address_counts {
            match status.as_str() {
                "PENDING" => stats.addresses_pending = count,
                "PAID" => stats.addresses_paid = count,
                "EXPIRED" => stats.addresses_expired = count,
                _ => {}
            }
        }

        let deposit_counts: Vec<(String, i64)> =
            sqlx::query_as("SELECT status::text, COUNT(*) FROM deposits GROUP BY status")
                .fetch_all(&self.pool)
                .await?;
        for (status, count) in deposit_counts {
            match status.as_str() {
                "DETECTED" => stats.deposits_detected = count,
                "CONFIRMED" => stats.deposits_confirmed = count,
                _ => {}
            }
        }

        let (volume,): (Decimal,) = sqlx::query_as(
            r#"SELECT COALESCE(SUM(amount), 0) FROM deposits WHERE status = 'CONFIRMED'::"DepositStatus""#,
        )
        .fetch_one(&self.pool)
        .await?;
        stats.confirmed_volume = volume;

        // Count webhook statuses on authoritative rows only (newest per
        // logical delivery key).
        let webhook_counts: Vec<(String, i64)> = sqlx::query_as(
            r#"
            SELECT t.status, COUNT(*) FROM (
                SELECT DISTINCT ON ("depositAddressId", "eventType", "depositId", "webhookEndpointId") status::text AS status
                FROM webhook_deliveries
                ORDER BY "depositAddressId", "eventType", "depositId", "webhookEndpointId", "createdAt" DESC
            ) t GROUP BY t.status
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        for (status, count) in webhook_counts {
            match status.as_str() {
                "PENDING" => stats.webhooks_pending = count,
                "DELIVERED" => stats.webhooks_delivered = count,
                "FAILED" => stats.webhooks_failed = count,
                _ => {}
            }
        }

        Ok(stats)
    }
}
