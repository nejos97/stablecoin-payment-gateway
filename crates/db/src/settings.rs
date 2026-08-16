use anyhow::Result;
use config::DEPOSIT_EXPIRY_DEFAULT_MINUTES;

use crate::Db;

pub const DEPOSIT_EXPIRY_SETTING: &str = "depositExpiryMinutes";
pub const API_KEY_PREFIX_SETTING: &str = "apiKeyPrefix";

impl Db {
    pub async fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as(r#"SELECT "value" FROM app_settings WHERE "key" = $1"#)
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(value,)| value))
    }

    pub async fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO app_settings ("key", "value", "updatedAt")
            VALUES ($1, $2, NOW())
            ON CONFLICT ("key") DO UPDATE SET "value" = $2, "updatedAt" = NOW()
            "#,
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Optional tag prepended to newly generated API keys (`{tag}_{8}_{40}`).
    /// `None` when unset or blank — keys are then generated as `{8}_{40}`.
    pub async fn api_key_prefix(&self) -> Result<Option<String>> {
        let value = self.get_setting(API_KEY_PREFIX_SETTING).await?;
        Ok(value
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty()))
    }

    /// Default payment-address expiry, admin-configurable from the dashboard.
    /// Falls back to the code default when the row is missing or unparsable.
    pub async fn deposit_expiry_minutes(&self) -> Result<i64> {
        let value = self.get_setting(DEPOSIT_EXPIRY_SETTING).await?;
        Ok(value
            .and_then(|v| v.parse::<i64>().ok())
            .filter(|v| *v > 0)
            .unwrap_or(DEPOSIT_EXPIRY_DEFAULT_MINUTES))
    }
}
