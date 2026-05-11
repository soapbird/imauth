use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthJob {
    pub job_id: String,
    pub platform: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthJobResult {
    pub job_id: String,
    pub success: bool,
    pub message: String,
    pub cookies: Vec<crate::session::state::Cookie>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshJob {
    pub platform: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateJob {
    pub platform: String,
}
