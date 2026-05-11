pub mod encryption;

use crate::ImauthError;
use encryption::AesGcmEncryption;
use sqlx::SqlitePool;

pub struct Credential {
    pub platform: String,
    pub username: String,
    pub password_encrypted: String,
    pub twofa_method: Option<String>,
}

pub struct CredentialStore {
    pool: SqlitePool,
    encryption: AesGcmEncryption,
}

impl CredentialStore {
    pub fn new(pool: SqlitePool, encryption: AesGcmEncryption) -> Self {
        Self { pool, encryption }
    }

    pub async fn init(&self) -> crate::Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS credentials (
                platform TEXT PRIMARY KEY,
                username TEXT NOT NULL,
                password_encrypted TEXT NOT NULL,
                twofa_method TEXT,
                created_at INTEGER NOT NULL DEFAULT (unixepoch()),
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            )
            "#,
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ImauthError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn save(
        &self,
        platform: &str,
        username: &str,
        password: &str,
        twofa_method: Option<&str>,
    ) -> crate::Result<()> {
        let encrypted = self.encryption.encrypt(password)?;
        sqlx::query(
            r#"
            INSERT INTO credentials (platform, username, password_encrypted, twofa_method, updated_at)
            VALUES (?1, ?2, ?3, ?4, unixepoch())
            ON CONFLICT(platform) DO UPDATE SET
                username = excluded.username,
                password_encrypted = excluded.password_encrypted,
                twofa_method = excluded.twofa_method,
                updated_at = unixepoch()
            "#,
        )
        .bind(platform)
        .bind(username)
        .bind(&encrypted)
        .bind(twofa_method)
        .execute(&self.pool)
        .await
        .map_err(|e| ImauthError::Database(e.to_string()))?;
        Ok(())
    }

    pub async fn get(&self, platform: &str) -> crate::Result<Option<Credential>> {
        let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT platform, username, password_encrypted, twofa_method FROM credentials WHERE platform = ?",
        )
        .bind(platform)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ImauthError::Database(e.to_string()))?;

        Ok(row.map(
            |(platform, username, password_encrypted, twofa_method)| Credential {
                platform,
                username,
                password_encrypted,
                twofa_method,
            },
        ))
    }

    pub async fn get_decrypted_password(
        &self,
        platform: &str,
    ) -> crate::Result<Option<(String, String)>> {
        let cred = self.get(platform).await?;
        match cred {
            Some(c) => {
                let password = self.encryption.decrypt(&c.password_encrypted)?;
                Ok(Some((c.username, password)))
            }
            None => Ok(None),
        }
    }

    pub async fn delete(&self, platform: &str) -> crate::Result<()> {
        sqlx::query("DELETE FROM credentials WHERE platform = ?")
            .bind(platform)
            .execute(&self.pool)
            .await
            .map_err(|e| ImauthError::Database(e.to_string()))?;
        Ok(())
    }
}
