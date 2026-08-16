use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use crate::Db;

const API_KEY_COLUMNS: &str = r#"id, name, prefix, "keyHash", status::text AS status, "createdById", "lastUsedAt" AT TIME ZONE 'UTC' AS "lastUsedAt", "revokedAt" AT TIME ZONE 'UTC' AS "revokedAt", "expiresAt" AT TIME ZONE 'UTC' AS "expiresAt", "createdAt" AT TIME ZONE 'UTC' AS "createdAt""#;

#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyRow {
    pub id: String,
    pub name: String,
    pub prefix: String,
    #[sqlx(rename = "keyHash")]
    pub key_hash: String,
    pub status: String,
    #[sqlx(rename = "createdById")]
    pub created_by_id: String,
    #[sqlx(rename = "lastUsedAt")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[sqlx(rename = "revokedAt")]
    pub revoked_at: Option<DateTime<Utc>>,
    /// NULL = never expires (revocable only).
    #[sqlx(rename = "expiresAt")]
    pub expires_at: Option<DateTime<Utc>>,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

/// List row: `ApiKeyRow` plus the creator's display name.
#[derive(Debug, Clone, FromRow)]
pub struct ApiKeyListRow {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub status: String,
    #[sqlx(rename = "createdById")]
    pub created_by_id: String,
    #[sqlx(rename = "createdByName")]
    pub created_by_name: Option<String>,
    #[sqlx(rename = "lastUsedAt")]
    pub last_used_at: Option<DateTime<Utc>>,
    #[sqlx(rename = "revokedAt")]
    pub revoked_at: Option<DateTime<Utc>>,
    #[sqlx(rename = "expiresAt")]
    pub expires_at: Option<DateTime<Utc>>,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
}

impl Db {
    pub async fn create_api_key(
        &self,
        name: &str,
        prefix: &str,
        key_hash: &str,
        created_by_id: &str,
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<ApiKeyRow> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            r#"
            INSERT INTO api_keys
              (id, name, prefix, "keyHash", status, "createdById", "expiresAt", "createdAt")
            VALUES
              ($1, $2, $3, $4, 'ACTIVE'::"ApiKeyStatus", $5, $6, $7)
            RETURNING {API_KEY_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(name)
        .bind(prefix)
        .bind(key_hash)
        .bind(created_by_id)
        .bind(expires_at)
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeyListRow>> {
        let rows = sqlx::query_as::<_, ApiKeyListRow>(
            r#"
            SELECT k.id, k.name, k.prefix, k.status::text AS status, k."createdById",
                   s."firstName" || ' ' || s."lastName" AS "createdByName",
                   k."lastUsedAt" AT TIME ZONE 'UTC' AS "lastUsedAt",
                   k."revokedAt" AT TIME ZONE 'UTC' AS "revokedAt",
                   k."expiresAt" AT TIME ZONE 'UTC' AS "expiresAt",
                   k."createdAt" AT TIME ZONE 'UTC' AS "createdAt"
            FROM api_keys k
            LEFT JOIN staff_users s ON s.id = k."createdById"
            ORDER BY k."createdAt" DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// Usable key for a clear-text prefix. The expiry check here is what
    /// rejects a key the second its deadline passes — the periodic sweep
    /// only materializes the EXPIRED status afterwards.
    pub async fn find_api_key_by_prefix(&self, prefix: &str) -> Result<Option<ApiKeyRow>> {
        let row = sqlx::query_as::<_, ApiKeyRow>(&format!(
            r#"
            SELECT {API_KEY_COLUMNS} FROM api_keys
            WHERE prefix = $1
              AND status = 'ACTIVE'::"ApiKeyStatus"
              AND ("expiresAt" IS NULL OR "expiresAt" > NOW())
            "#
        ))
        .bind(prefix)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    /// Returns `false` when the key does not exist or is already revoked.
    pub async fn revoke_api_key(&self, id: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE api_keys
            SET status = 'REVOKED'::"ApiKeyStatus", "revokedAt" = NOW()
            WHERE id = $1 AND "revokedAt" IS NULL
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    /// Materialize the EXPIRED status for keys whose deadline has passed.
    /// Lookup already rejects them; this keeps listings and stats honest.
    pub async fn expire_stale_api_keys(&self) -> Result<u64> {
        let result = sqlx::query(
            r#"
            UPDATE api_keys
            SET status = 'EXPIRED'::"ApiKeyStatus"
            WHERE status = 'ACTIVE'::"ApiKeyStatus" AND "expiresAt" <= NOW()
            "#,
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn touch_api_key_last_used(&self, id: &str) -> Result<()> {
        sqlx::query(r#"UPDATE api_keys SET "lastUsedAt" = NOW() WHERE id = $1"#)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}
