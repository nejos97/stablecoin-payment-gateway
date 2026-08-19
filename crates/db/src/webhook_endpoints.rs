use anyhow::Result;
use chrono::{DateTime, Utc};
use domain::WebhookEventType;
use sqlx::FromRow;
use uuid::Uuid;

use crate::Db;

const WEBHOOK_ENDPOINT_COLUMNS: &str = r#"id, url, "isActive", events, secret, "createdById", "createdAt" AT TIME ZONE 'UTC' AS "createdAt", "updatedAt" AT TIME ZONE 'UTC' AS "updatedAt""#;

#[derive(Debug, Clone, FromRow)]
pub struct WebhookEndpointRow {
    pub id: String,
    pub url: String,
    #[sqlx(rename = "isActive")]
    pub is_active: bool,
    /// DB values of the subscribed `WebhookEventType`s.
    pub events: Vec<String>,
    /// Signing secret for X-Webhook-Signature; NULL = unsigned deliveries.
    /// Never expose through the API — only `has_secret`.
    pub secret: Option<String>,
    #[sqlx(rename = "createdById")]
    pub created_by_id: String,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

impl WebhookEndpointRow {
    pub fn has_secret(&self) -> bool {
        self.secret.as_deref().is_some_and(|s| !s.is_empty())
    }
}

impl WebhookEndpointRow {
    pub fn accepts_event(&self, event: WebhookEventType) -> bool {
        self.events.iter().any(|e| e == event.as_db())
    }
}

/// List row: `WebhookEndpointRow` plus the creator's display name.
#[derive(Debug, Clone, FromRow)]
pub struct WebhookEndpointListRow {
    pub id: String,
    pub url: String,
    #[sqlx(rename = "isActive")]
    pub is_active: bool,
    pub events: Vec<String>,
    /// The secret itself never leaves the delivery path.
    #[sqlx(rename = "hasSecret")]
    pub has_secret: bool,
    #[sqlx(rename = "createdById")]
    pub created_by_id: String,
    #[sqlx(rename = "createdByName")]
    pub created_by_name: Option<String>,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

impl Db {
    pub async fn create_webhook_endpoint(
        &self,
        url: &str,
        events: &[WebhookEventType],
        secret: Option<&str>,
        created_by_id: &str,
    ) -> Result<WebhookEndpointRow> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let events_db: Vec<String> = events.iter().map(|e| e.as_db().to_string()).collect();
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"
            INSERT INTO webhook_endpoints
              (id, url, "isActive", events, secret, "createdById", "createdAt", "updatedAt")
            VALUES
              ($1, $2, true, $3, $4, $5, $6, $6)
            RETURNING {WEBHOOK_ENDPOINT_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(url)
        .bind(&events_db)
        .bind(secret)
        .bind(created_by_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_webhook_endpoints(&self) -> Result<Vec<WebhookEndpointListRow>> {
        let rows = sqlx::query_as::<_, WebhookEndpointListRow>(
            r#"
            SELECT e.id, e.url, e."isActive", e.events,
                   (e.secret IS NOT NULL AND e.secret <> '') AS "hasSecret",
                   e."createdById",
                   s."firstName" || ' ' || s."lastName" AS "createdByName",
                   e."createdAt" AT TIME ZONE 'UTC' AS "createdAt",
                   e."updatedAt" AT TIME ZONE 'UTC' AS "updatedAt"
            FROM webhook_endpoints e
            LEFT JOIN staff_users s ON s.id = e."createdById"
            ORDER BY e."createdAt" ASC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn find_webhook_endpoint(&self, id: &str) -> Result<Option<WebhookEndpointRow>> {
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            "SELECT {WEBHOOK_ENDPOINT_COLUMNS} FROM webhook_endpoints WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_webhook_endpoint_by_url(
        &self,
        url: &str,
    ) -> Result<Option<WebhookEndpointRow>> {
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            "SELECT {WEBHOOK_ENDPOINT_COLUMNS} FROM webhook_endpoints WHERE url = $1"
        ))
        .bind(url)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn active_webhook_endpoints(&self) -> Result<Vec<WebhookEndpointRow>> {
        let rows = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"SELECT {WEBHOOK_ENDPOINT_COLUMNS} FROM webhook_endpoints WHERE "isActive" = true ORDER BY "createdAt" ASC"#
        ))
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Active endpoints subscribed to `event` — the fan-out set when that
    /// event fires.
    pub async fn active_webhook_endpoints_for_event(
        &self,
        event: WebhookEventType,
    ) -> Result<Vec<WebhookEndpointRow>> {
        let rows = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"
            SELECT {WEBHOOK_ENDPOINT_COLUMNS} FROM webhook_endpoints
            WHERE "isActive" = true AND $1 = ANY(events)
            ORDER BY "createdAt" ASC
            "#
        ))
        .bind(event.as_db())
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    pub async fn set_webhook_endpoint_active(
        &self,
        id: &str,
        is_active: bool,
    ) -> Result<Option<WebhookEndpointRow>> {
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"
            UPDATE webhook_endpoints
            SET "isActive" = $2, "updatedAt" = NOW()
            WHERE id = $1
            RETURNING {WEBHOOK_ENDPOINT_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(is_active)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn set_webhook_endpoint_events(
        &self,
        id: &str,
        events: &[WebhookEventType],
    ) -> Result<Option<WebhookEndpointRow>> {
        let events_db: Vec<String> = events.iter().map(|e| e.as_db().to_string()).collect();
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"
            UPDATE webhook_endpoints
            SET events = $2, "updatedAt" = NOW()
            WHERE id = $1
            RETURNING {WEBHOOK_ENDPOINT_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(&events_db)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Set (rotate) or clear the signing secret. `None` disables signing.
    pub async fn set_webhook_endpoint_secret(
        &self,
        id: &str,
        secret: Option<&str>,
    ) -> Result<Option<WebhookEndpointRow>> {
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"
            UPDATE webhook_endpoints
            SET secret = $2, "updatedAt" = NOW()
            WHERE id = $1
            RETURNING {WEBHOOK_ENDPOINT_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(secret)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Hard delete, allowed only when no delivery references the endpoint.
    /// Returns Ok(false) when deliveries exist (caller maps to 409).
    pub async fn delete_webhook_endpoint_if_unused(&self, id: &str) -> Result<Option<bool>> {
        if self.find_webhook_endpoint(id).await?.is_none() {
            return Ok(None);
        }
        let (used,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM webhook_deliveries WHERE "webhookEndpointId" = $1"#,
        )
        .bind(id)
        .fetch_one(&self.pool)
        .await?;
        if used > 0 {
            return Ok(Some(false));
        }
        sqlx::query("DELETE FROM webhook_endpoints WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(Some(true))
    }
}
