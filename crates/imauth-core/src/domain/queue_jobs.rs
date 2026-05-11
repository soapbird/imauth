use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthJob {
    pub job_id: String,
    pub platform: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshJob {
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJob {
    pub platform: String,
}
