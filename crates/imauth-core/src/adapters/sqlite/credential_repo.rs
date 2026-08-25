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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::aes_gcm::AesGcmEncryptionService;
    use crate::adapters::sqlite::run_migrations;

    const KEY: &str = "pZN6lLjwDGIpj/BUWeTFnsB7GUp9bSuwnUcS3gYkQ2A=";

    async fn test_repository() -> (SqlitePool, SqliteCredentialRepository) {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        let encryption: Arc<dyn EncryptionService> =
            Arc::new(AesGcmEncryptionService::from_key(KEY).unwrap());
        let repo = SqliteCredentialRepository::new(pool.clone(), encryption);
        (pool, repo)
    }

    #[tokio::test]
    async fn save_encrypts_and_get_returns_persisted_credential() {
        // Given: a fresh credential repository and plaintext password.
        let (pool, repo) = test_repository().await;
        let plaintext = "correct horse battery staple";

        // When: a credential is saved.
        repo.save("instagram", "alice", plaintext, Some("totp"))
            .await
            .unwrap();

        // Then: storage is encrypted and the repository exposes the stored credential.
        let raw: String = sqlx::query_scalar(
            "SELECT password_encrypted FROM credentials WHERE platform = 'instagram'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let credential = repo.get("instagram").await.unwrap().unwrap();
        let encryption = AesGcmEncryptionService::from_key(KEY).unwrap();
        assert_ne!(raw, plaintext);
        assert_eq!(encryption.decrypt(&raw).unwrap(), plaintext);
        assert_eq!(credential.platform, "instagram");
        assert_eq!(credential.username, "alice");
        assert_eq!(credential.password_encrypted, raw);
        assert_eq!(credential.twofa_method.as_deref(), Some("totp"));
    }

    #[tokio::test]
    async fn save_updates_existing_platform_credential() {
        // Given: a credential already stored for a platform.
        let (pool, repo) = test_repository().await;
        repo.save("instagram", "old-user", "old-password", Some("sms"))
            .await
            .unwrap();

        // When: the same platform is saved with replacement values.
        repo.save("instagram", "new-user", "new-password", None)
            .await
            .unwrap();

        // Then: one updated encrypted record remains.
        let credential = repo.get("instagram").await.unwrap().unwrap();
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM credentials WHERE platform = 'instagram'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let encryption = AesGcmEncryptionService::from_key(KEY).unwrap();
        assert_eq!(count, 1);
        assert_eq!(credential.username, "new-user");
        assert_eq!(
            encryption.decrypt(&credential.password_encrypted).unwrap(),
            "new-password"
        );
        assert_eq!(credential.twofa_method, None);
    }

    #[tokio::test]
    async fn delete_removes_existing_credential() {
        // Given: an existing credential in a fresh repository.
        let (_pool, repo) = test_repository().await;
        repo.save("threads", "alice", "secret", None).await.unwrap();

        // When: the credential is deleted.
        repo.delete("threads").await.unwrap();

        // Then: subsequent lookup reports no credential.
        assert!(repo.get("threads").await.unwrap().is_none());
    }
}
