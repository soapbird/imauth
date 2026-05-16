use crate::domain::Credential;
use crate::ports::encryption::EncryptionService;
use crate::ports::repository::CredentialRepository;
use crate::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::sync::Arc;

pub struct SqliteCredentialRepository {
    pool: SqlitePool,
    encryption: Arc<dyn EncryptionService>,
}

impl SqliteCredentialRepository {
    pub fn new(pool: SqlitePool, encryption: Arc<dyn EncryptionService>) -> Self {
        Self { pool, encryption }
    }
}

#[async_trait]
impl CredentialRepository for SqliteCredentialRepository {
    async fn save<'a>(
        &'a self,
        platform: &'a str,
        username: &'a str,
        password: &'a str,
        twofa_method: Option<&'a str>,
    ) -> Result<()> {
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
        .await?;
        Ok(())
    }

    async fn get(&self, platform: &str) -> Result<Option<Credential>> {
        let row: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT platform, username, password_encrypted, twofa_method FROM credentials WHERE platform = ?",
        )
        .bind(platform)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(
            |(platform, username, password_encrypted, twofa_method)| Credential {
                platform,
                username,
                password_encrypted,
                twofa_method,
            },
        ))
    }

    async fn delete(&self, platform: &str) -> Result<()> {
        sqlx::query("DELETE FROM credentials WHERE platform = ?")
            .bind(platform)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn get_decrypted_password(&self, platform: &str) -> Result<Option<(String, String)>> {
        let cred = self.get(platform).await?;
        match cred {
            Some(c) => {
                let password = self.encryption.decrypt(&c.password_encrypted)?;
                Ok(Some((c.username, password)))
            }
            None => Ok(None),
        }
    }
}
