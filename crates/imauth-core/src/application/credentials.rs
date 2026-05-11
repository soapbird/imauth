use crate::Result;
use crate::domain::{Credential, Platform};
use crate::ports::repository::CredentialRepository;
use std::sync::Arc;

pub struct SaveCredentialUseCase {
    credentials: Arc<dyn CredentialRepository>,
}

impl SaveCredentialUseCase {
    pub fn new(credentials: Arc<dyn CredentialRepository>) -> Self {
        Self { credentials }
    }

    pub async fn execute(
        &self,
        platform: Platform,
        username: &str,
        password: &str,
        twofa_method: Option<&str>,
    ) -> Result<()> {
        self.credentials
            .save(platform.as_str(), username, password, twofa_method)
            .await
    }
}

pub struct GetCredentialUseCase {
    credentials: Arc<dyn CredentialRepository>,
}

impl GetCredentialUseCase {
    pub fn new(credentials: Arc<dyn CredentialRepository>) -> Self {
        Self { credentials }
    }

    pub async fn execute(&self, platform: Platform) -> Result<Option<Credential>> {
        self.credentials.get(platform.as_str()).await
    }
}

pub struct DeleteCredentialUseCase {
    credentials: Arc<dyn CredentialRepository>,
}

impl DeleteCredentialUseCase {
    pub fn new(credentials: Arc<dyn CredentialRepository>) -> Self {
        Self { credentials }
    }

    pub async fn execute(&self, platform: Platform) -> Result<()> {
        self.credentials.delete(platform.as_str()).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::repository::MockCredentialRepository;

    #[tokio::test]
    async fn save_forwards_platform_string() {
        let mut repo = MockCredentialRepository::new();
        repo.expect_save()
            .withf(|p, u, pw, m| p == "instagram" && u == "user" && pw == "secret" && m.is_none())
            .times(1)
            .returning(|_, _, _, _| Ok(()));
        let uc = SaveCredentialUseCase::new(Arc::new(repo));
        uc.execute(Platform::Instagram, "user", "secret", None)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_returns_credential() {
        let mut repo = MockCredentialRepository::new();
        repo.expect_get().return_once(|_| {
            Ok(Some(Credential {
                platform: "instagram".into(),
                username: "u".into(),
                password_encrypted: "enc".into(),
                twofa_method: Some("totp".into()),
            }))
        });
        let uc = GetCredentialUseCase::new(Arc::new(repo));
        let cred = uc.execute(Platform::Instagram).await.unwrap().unwrap();
        assert_eq!(cred.username, "u");
        assert_eq!(cred.twofa_method.as_deref(), Some("totp"));
    }

    #[tokio::test]
    async fn delete_forwards_platform_string() {
        let mut repo = MockCredentialRepository::new();
        repo.expect_delete()
            .withf(|p| p == "threads")
            .times(1)
            .returning(|_| Ok(()));
        let uc = DeleteCredentialUseCase::new(Arc::new(repo));
        uc.execute(Platform::Threads).await.unwrap();
    }
}
