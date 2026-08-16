use anyhow::Result;
use chrono::{DateTime, Utc};
use domain::StaffRole;
use sqlx::FromRow;
use uuid::Uuid;

use crate::Db;

const STAFF_USER_COLUMNS: &str = r#"id, email, "firstName", "lastName", "passwordHash", role::text AS role, "isActive", "createdAt" AT TIME ZONE 'UTC' AS "createdAt", "updatedAt" AT TIME ZONE 'UTC' AS "updatedAt""#;

#[derive(Debug, Clone, FromRow)]
pub struct StaffUserRow {
    pub id: String,
    pub email: String,
    #[sqlx(rename = "firstName")]
    pub first_name: String,
    #[sqlx(rename = "lastName")]
    pub last_name: String,
    #[sqlx(rename = "passwordHash")]
    pub password_hash: String,
    pub role: String,
    #[sqlx(rename = "isActive")]
    pub is_active: bool,
    #[sqlx(rename = "createdAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,
}

impl Db {
    pub async fn count_staff(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM staff_users")
            .fetch_one(&self.pool)
            .await?;
        Ok(count)
    }

    pub async fn count_active_admins(&self) -> Result<i64> {
        let (count,): (i64,) = sqlx::query_as(
            r#"SELECT COUNT(*) FROM staff_users WHERE role = 'ADMIN'::"StaffRole" AND "isActive" = true"#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    pub async fn create_staff_user(
        &self,
        email: &str,
        first_name: &str,
        last_name: &str,
        password_hash: &str,
        role: StaffRole,
    ) -> Result<StaffUserRow> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, StaffUserRow>(&format!(
            r#"
            INSERT INTO staff_users
              (id, email, "firstName", "lastName", "passwordHash", role, "isActive", "createdAt", "updatedAt")
            VALUES
              ($1, $2, $3, $4, $5, $6::"StaffRole", true, $7, $7)
            RETURNING {STAFF_USER_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(email)
        .bind(first_name)
        .bind(last_name)
        .bind(password_hash)
        .bind(role.as_db())
        .bind(now)
        .fetch_one(&self.pool)
        .await?;
        Ok(row)
    }

    /// Insert the first admin only while the table is still empty, in one
    /// guarded statement so two concurrent setup calls cannot both succeed.
    /// Returns `None` when a staff user already exists.
    pub async fn create_first_admin(
        &self,
        email: &str,
        first_name: &str,
        last_name: &str,
        password_hash: &str,
    ) -> Result<Option<StaffUserRow>> {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let row = sqlx::query_as::<_, StaffUserRow>(&format!(
            r#"
            INSERT INTO staff_users
              (id, email, "firstName", "lastName", "passwordHash", role, "isActive", "createdAt", "updatedAt")
            SELECT $1, $2, $3, $4, $5, 'ADMIN'::"StaffRole", true, $6, $6
            WHERE NOT EXISTS (SELECT 1 FROM staff_users)
            RETURNING {STAFF_USER_COLUMNS}
            "#
        ))
        .bind(&id)
        .bind(email)
        .bind(first_name)
        .bind(last_name)
        .bind(password_hash)
        .bind(now)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_staff_by_email(&self, email: &str) -> Result<Option<StaffUserRow>> {
        let row = sqlx::query_as::<_, StaffUserRow>(&format!(
            "SELECT {STAFF_USER_COLUMNS} FROM staff_users WHERE lower(email) = lower($1)"
        ))
        .bind(email)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn find_staff_by_id(&self, id: &str) -> Result<Option<StaffUserRow>> {
        let row = sqlx::query_as::<_, StaffUserRow>(&format!(
            "SELECT {STAFF_USER_COLUMNS} FROM staff_users WHERE id = $1"
        ))
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn list_staff(&self, limit: i64, offset: i64) -> Result<(Vec<StaffUserRow>, i64)> {
        let rows = sqlx::query_as::<_, StaffUserRow>(&format!(
            r#"SELECT {STAFF_USER_COLUMNS} FROM staff_users ORDER BY "createdAt" ASC LIMIT $1 OFFSET $2"#
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(&self.pool)
        .await?;

        let total = self.count_staff().await?;
        Ok((rows, total))
    }

    pub async fn update_staff(
        &self,
        id: &str,
        first_name: Option<&str>,
        last_name: Option<&str>,
        role: Option<StaffRole>,
        is_active: Option<bool>,
        password_hash: Option<&str>,
    ) -> Result<Option<StaffUserRow>> {
        let row = sqlx::query_as::<_, StaffUserRow>(&format!(
            r#"
            UPDATE staff_users
            SET "firstName" = COALESCE($2, "firstName"),
                "lastName" = COALESCE($3, "lastName"),
                role = COALESCE($4::"StaffRole", role),
                "isActive" = COALESCE($5, "isActive"),
                "passwordHash" = COALESCE($6, "passwordHash"),
                "updatedAt" = NOW()
            WHERE id = $1
            RETURNING {STAFF_USER_COLUMNS}
            "#
        ))
        .bind(id)
        .bind(first_name)
        .bind(last_name)
        .bind(role.map(|r| r.as_db()))
        .bind(is_active)
        .bind(password_hash)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }
}
