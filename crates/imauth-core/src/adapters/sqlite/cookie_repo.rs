use crate::domain::session::{Cookie, Session, SessionState};
use crate::ports::encryption::EncryptionService;
use crate::ports::repository::CookieRepository;
use crate::{ImauthError, Result};
use async_trait::async_trait;
use chrono::DateTime;
use sqlx::{Sqlite, SqlitePool, Transaction};
use std::sync::Arc;

const ENCRYPTED_VALUE_PREFIX: &str = "enc:v1:";

pub struct SqliteCookieRepository {
    pool: SqlitePool,
    encryption: Arc<dyn EncryptionService>,
}

impl SqliteCookieRepository {
    pub fn new(pool: SqlitePool, encryption: Arc<dyn EncryptionService>) -> Self {
        Self { pool, encryption }
    }

    async fn cookie_from_row(&self, platform: &str, row: CookieRow) -> Result<Cookie> {
        let (name, stored_value, domain, path, expires, http_only, secure) = row;
        let value = match stored_value.strip_prefix(ENCRYPTED_VALUE_PREFIX) {
            Some(ciphertext) => self.encryption.decrypt(ciphertext)?,
            None => {
                self.migrate_plaintext(platform, &name, &domain, &stored_value)
                    .await?;
                stored_value
            }
        };

        Ok(Cookie {
            name,
            value,
            domain,
            path,
            expires: expires.and_then(|ts| DateTime::from_timestamp(ts, 0)),
            http_only: http_only != 0,
            secure: secure != 0,
        })
    }

    async fn migrate_plaintext(
        &self,
        platform: &str,
        name: &str,
        domain: &str,
        plaintext: &str,
    ) -> Result<()> {
        let ciphertext = self.encryption.encrypt(plaintext)?;
        sqlx::query(
            r#"
            UPDATE cookies
            SET value = ?1, updated_at = unixepoch()
            WHERE platform = ?2 AND name = ?3 AND domain = ?4 AND value = ?5
            "#,
        )
        .bind(format!("{ENCRYPTED_VALUE_PREFIX}{ciphertext}"))
        .bind(platform)
        .bind(name)
        .bind(domain)
        .bind(plaintext)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn save_cookies_in_transaction(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        platform: &str,
        cookies: &[Cookie],
    ) -> Result<()> {
        for cookie in cookies {
            let expires = cookie.expires.map(|dt| dt.timestamp());
            let encrypted_value = format!(
                "{ENCRYPTED_VALUE_PREFIX}{}",
                self.encryption.encrypt(&cookie.value)?
            );
            sqlx::query(
                r#"
                INSERT INTO cookies (platform, name, value, domain, path, expires, http_only, secure, updated_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())
                ON CONFLICT(platform, name, domain) DO UPDATE SET
                    value = excluded.value,
                    path = excluded.path,
                    expires = excluded.expires,
                    http_only = excluded.http_only,
                    secure = excluded.secure,
                    updated_at = unixepoch()
                "#,
            )
            .bind(platform)
            .bind(&cookie.name)
            .bind(encrypted_value)
            .bind(&cookie.domain)
            .bind(&cookie.path)
            .bind(expires)
            .bind(cookie.http_only as i32)
            .bind(cookie.secure as i32)
            .execute(&mut **tx)
            .await?;
        }
        Ok(())
    }
}

type CookieRow = (String, String, String, String, Option<i64>, i32, i32);

#[async_trait]
impl CookieRepository for SqliteCookieRepository {
    async fn save(&self, platform: &str, cookies: &[Cookie]) -> Result<()> {
        if cookies.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;
        self.save_cookies_in_transaction(&mut tx, platform, cookies)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    async fn save_login(&self, session: &Session, cookies: &[Cookie]) -> Result<()> {
        if session.state != SessionState::Connected
            || session.requires_input
            || session.input_type.is_some()
        {
            return Err(ImauthError::Database(
                "login persistence requires a connected session without pending input".into(),
            ));
        }

        let mut tx = self.pool.begin().await?;
        self.save_cookies_in_transaction(&mut tx, &session.platform, cookies)
            .await?;
        let update = sqlx::query(
            r#"
            UPDATE sessions SET
                status = ?2,
                message = ?3,
                requires_input = ?4,
                input_type = ?5,
                updated_at = ?6
            WHERE id = ?1
              AND platform = ?7
              AND status IN ('idle', 'loading', 'authenticating', 'waiting_for_user')
            "#,
        )
        .bind(&session.id)
        .bind(session.state.as_str())
        .bind(&session.message)
        .bind(session.requires_input as i32)
        .bind(&session.input_type)
        .bind(session.updated_at.timestamp())
        .bind(&session.platform)
        .execute(&mut *tx)
        .await?;
        if update.rows_affected() != 1 {
            return Err(ImauthError::Database(
                "login session was deleted, terminal, or belongs to another platform".into(),
            ));
        }
        tx.commit().await?;
        Ok(())
    }

    async fn get<'a>(
        &'a self,
        platform: &'a str,
        domains: Option<&'a [String]>,
    ) -> Result<Vec<Cookie>> {
        const SELECT: &str =
            "SELECT name, value, domain, path, expires, http_only, secure FROM cookies WHERE platform = ?";

        let rows: Vec<CookieRow> = match domains {
            Some([]) => return Ok(Vec::new()),
            Some(domains) => {
                let placeholders = domains.iter().map(|_| "?").collect::<Vec<_>>().join(",");
                let query = format!("{SELECT} AND domain IN ({placeholders})");
                let mut q = sqlx::query_as(&query).bind(platform);
                for d in domains {
                    q = q.bind(d);
                }
                q.fetch_all(&self.pool).await?
            }
            None => {
                sqlx::query_as(SELECT)
                    .bind(platform)
                    .fetch_all(&self.pool)
                    .await?
            }
        };

        let mut cookies = Vec::with_capacity(rows.len());
        for row in rows {
            cookies.push(self.cookie_from_row(platform, row).await?);
        }
        Ok(cookies)
    }

    async fn export_netscape(&self, platform: &str) -> Result<String> {
        let cookies = self.get(platform, None).await?;
        let mut lines = vec![
            "# Netscape HTTP Cookie File".to_string(),
            "# This file was generated by imauth. Edit at your own risk.".to_string(),
        ];

        let flag = |b: bool| if b { "TRUE" } else { "FALSE" };
        for cookie in cookies {
            let expires = cookie.expires.map(|dt| dt.timestamp()).unwrap_or(0);
            let include_subdomains_flag = flag(cookie.domain.starts_with('.'));
            let secure_flag = flag(cookie.secure);
            lines.push(format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                cookie.domain,
                include_subdomains_flag,
                cookie.path,
                secure_flag,
                expires,
                cookie.name,
                cookie.value,
            ));
        }

        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::aes_gcm::AesGcmEncryptionService;
    use crate::adapters::sqlite::run_migrations;
    use crate::domain::session::{Session, SessionState};
    use crate::ports::encryption::EncryptionService;
    use chrono::Utc;
    use std::sync::Arc;

    const KEY: &str = "pZN6lLjwDGIpj/BUWeTFnsB7GUp9bSuwnUcS3gYkQ2A=";

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePool::connect("sqlite::memory:").await.unwrap();
        run_migrations(&pool).await.unwrap();
        pool
    }

    fn cookie(value: &str) -> Cookie {
        Cookie {
            name: "sessionid".to_string(),
            value: value.to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
            expires: Some(Utc::now()),
            http_only: true,
            secure: true,
        }
    }

    fn encryption() -> Arc<dyn EncryptionService> {
        Arc::new(AesGcmEncryptionService::from_key(KEY).unwrap())
    }

    #[tokio::test]
    async fn save_encrypts_cookie_value_at_rest() {
        // Given
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());

        // When
        repo.save("example", &[cookie("secret")]).await.unwrap();

        // Then
        let stored = stored_value(&pool).await;
        assert!(stored.starts_with("enc:v1:"));
        assert!(!stored.contains("secret"));
    }

    #[tokio::test]
    async fn get_decrypts_encrypted_cookie_value() {
        // Given
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool, encryption());
        repo.save("example", &[cookie("secret")]).await.unwrap();

        // When
        let cookies = repo.get("example", None).await.unwrap();

        // Then
        assert_eq!(cookies[0].value, "secret");
    }

    #[tokio::test]
    async fn get_migrates_plaintext_cookie_value() {
        // Given
        let pool = test_pool().await;
        insert_cookie_value(&pool, "plaintext").await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());

        // When
        let cookies = repo.get("example", None).await.unwrap();

        // Then
        assert_eq!(cookies[0].value, "plaintext");
        let stored = stored_value(&pool).await;
        assert!(stored.starts_with("enc:v1:"));
    }

    #[tokio::test]
    async fn plaintext_migration_does_not_overwrite_concurrent_newer_value() {
        // Given
        let pool = test_pool().await;
        insert_cookie_value(&pool, "old").await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());
        sqlx::query("UPDATE cookies SET value = 'newer' WHERE platform = 'example'")
            .execute(&pool)
            .await
            .unwrap();

        // When
        repo.migrate_plaintext("example", "sessionid", ".example.com", "old")
            .await
            .unwrap();

        // Then
        let stored = stored_value(&pool).await;
        assert_eq!(stored, "newer");
    }

    #[tokio::test]
    async fn get_errors_when_prefixed_cookie_value_is_corrupt() {
        // Given
        let pool = test_pool().await;
        insert_cookie_value(&pool, "enc:v1:not-ciphertext").await;
        let repo = SqliteCookieRepository::new(pool, encryption());

        // When
        let result = repo.get("example", None).await;

        // Then
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn export_netscape_marks_dotted_domains_for_subdomain_matching() {
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool, encryption());
        repo.save("example", &[cookie("secret")]).await.unwrap();

        let exported = repo.export_netscape("example").await.unwrap();

        assert!(exported.lines().any(|line| {
            line.starts_with(".example.com\tTRUE\t/\tTRUE\t")
                && line.ends_with("\tsessionid\tsecret")
        }));
    }

    #[tokio::test]
    async fn save_login_commits_connected_session_and_encrypted_cookies_together() {
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());
        let mut existing = session("login-1", "example");
        existing.transition(SessionState::WaitingForUser, None);
        insert_session(&pool, &existing).await;
        let mut connected = existing.clone();
        connected.transition(SessionState::Connected, Some("Login successful".into()));

        repo.save_login(&connected, &[cookie("secret")])
            .await
            .unwrap();

        assert_eq!(stored_status(&pool, &connected.id).await, "connected");
        let stored = stored_value(&pool).await;
        assert!(stored.starts_with("enc:v1:"));
        assert!(!stored.contains("secret"));
    }

    #[tokio::test]
    async fn save_login_for_deleted_session_writes_neither_cookies_nor_state() {
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());
        let mut connected = session("deleted", "example");
        connected.transition(SessionState::Connected, None);

        assert!(repo
            .save_login(&connected, &[cookie("secret")])
            .await
            .is_err());

        assert_eq!(cookie_count(&pool).await, 0);
        assert_eq!(session_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn save_login_rolls_back_cookie_upserts_when_connected_state_update_fails() {
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());
        let mut existing = session("login-2", "example");
        existing.transition(SessionState::WaitingForUser, None);
        insert_session(&pool, &existing).await;
        repo.save("example", &[cookie("old")]).await.unwrap();
        sqlx::query(
            "CREATE TRIGGER reject_connected_state BEFORE UPDATE OF status ON sessions \
             WHEN NEW.status = 'connected' BEGIN SELECT RAISE(ABORT, 'reject connected'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut connected = existing.clone();
        connected.transition(SessionState::Connected, None);

        assert!(repo.save_login(&connected, &[cookie("new")]).await.is_err());

        assert_eq!(
            stored_status(&pool, &connected.id).await,
            "waiting_for_user"
        );
        assert_eq!(repo.get("example", None).await.unwrap()[0].value, "old");
    }

    #[tokio::test]
    async fn save_login_does_not_mark_session_connected_when_cookie_insert_fails() {
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());
        let mut existing = session("login-3", "example");
        existing.transition(SessionState::WaitingForUser, None);
        insert_session(&pool, &existing).await;
        sqlx::query(
            "CREATE TRIGGER reject_cookie BEFORE INSERT ON cookies \
             WHEN NEW.name = 'reject' BEGIN SELECT RAISE(ABORT, 'reject cookie'); END",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut connected = existing.clone();
        connected.transition(SessionState::Connected, None);
        let mut rejected = cookie("secret");
        rejected.name = "reject".into();

        assert!(repo.save_login(&connected, &[rejected]).await.is_err());

        assert_eq!(
            stored_status(&pool, &connected.id).await,
            "waiting_for_user"
        );
        assert_eq!(cookie_count(&pool).await, 0);
    }

    #[tokio::test]
    async fn save_login_rejects_a_terminal_session_without_cookie_writes() {
        let pool = test_pool().await;
        let repo = SqliteCookieRepository::new(pool.clone(), encryption());
        let mut existing = session("login-4", "example");
        existing.transition(SessionState::Failed, Some("cancelled".into()));
        insert_session(&pool, &existing).await;
        let mut connected = existing.clone();
        connected.transition(SessionState::Connected, None);

        assert!(repo
            .save_login(&connected, &[cookie("secret")])
            .await
            .is_err());

        assert_eq!(stored_status(&pool, &connected.id).await, "failed");
        assert_eq!(cookie_count(&pool).await, 0);
    }

    async fn insert_cookie_value(pool: &SqlitePool, value: &str) {
        sqlx::query(
            "INSERT INTO cookies (platform, name, value, domain) VALUES ('example', 'sessionid', ?1, '.example.com')",
        )
        .bind(value)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn stored_value(pool: &SqlitePool) -> String {
        sqlx::query_scalar("SELECT value FROM cookies WHERE platform = 'example'")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    fn session(id: &str, platform: &str) -> Session {
        Session::new(id.to_string(), platform.to_string())
    }

    async fn insert_session(pool: &SqlitePool, session: &Session) {
        sqlx::query(
            "INSERT INTO sessions (id, platform, status, message, requires_input, input_type, created_at, updated_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        )
        .bind(&session.id)
        .bind(&session.platform)
        .bind(session.state.as_str())
        .bind(&session.message)
        .bind(session.requires_input as i32)
        .bind(&session.input_type)
        .bind(session.created_at.timestamp())
        .bind(session.updated_at.timestamp())
        .execute(pool)
        .await
        .unwrap();
    }

    async fn stored_status(pool: &SqlitePool, id: &str) -> String {
        sqlx::query_scalar("SELECT status FROM sessions WHERE id = ?1")
            .bind(id)
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn cookie_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM cookies")
            .fetch_one(pool)
            .await
            .unwrap()
    }

    async fn session_count(pool: &SqlitePool) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(pool)
            .await
            .unwrap()
    }
}
