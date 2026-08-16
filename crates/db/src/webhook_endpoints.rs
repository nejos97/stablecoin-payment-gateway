use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::Db;

const WEBHOOK_ENDPOINT_COLUMNS: &str = r#"id, url, "isActive", "createdById", "createdAt" AT TIME ZONE 'UTC' AS "createdAt", "updatedAt" AT TIME ZONE 'UTC' AS "updatedAt""#;

#[derive(Debug, Clone, FromRow)]
pub struct WebhookEndpointRow {
    pub id: String,
    pub url: String,
    #[sqlx(rename = "isActive")]
    pub is_active: bool,
    #[sqlx(rename = "createdById")]
    pub created_by_id: String,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

/// List row: `WebhookEndpointRow` plus the creator's display name.
#[derive(Debug, Clone, FromRow)]
pub struct WebhookEndpointListRow {
    pub id: String,
    pub url: String,
    #[sqlx(rename = "isActive")]
    pub is_active: bool,
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
        created_by_id: &str,
    ) -> Result<WebhookEndpointRow> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, WebhookEndpointRow>(&format!(
            r#"
            INSERT INTO webhook_endpoints
              (id, url, "isActive", "createdById", "createdAt", "updatedAt")
            VALUES
              ($1, $2, true, $3, $4, $4)
            RETURNING {WEBHOOK_ENDPOINT_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(url)
        .bind(created_by_id)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_webhook_endpoints(&self) -> Result<Vec<WebhookEndpointListRow>> {
        let rows = sqlx::query_as::<_, WebhookEndpointListRow>(
            r#"
            SELECT e.id, e.url, e."isActive", e."createdById",
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
