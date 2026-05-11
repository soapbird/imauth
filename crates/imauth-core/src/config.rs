use serde::{Deserialize, Serialize};
use std::path::PathBuf;

fn default_grpc_addr() -> String {
    "0.0.0.0:50051".to_string()
}

fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_cdp_url() -> String {
    "http://localhost:9222".to_string()
}

fn default_max_pool_size() -> usize {
    3
}

fn default_page_timeout_secs() -> u64 {
    30
}

fn default_data_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".imauth")
        .join("data")
}

fn default_interval_secs() -> u64 {
    3600
}

fn default_retry_max() -> u32 {
    3
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default = "default_nats_url")]
    pub nats_url: String,
    #[serde(default = "default_cdp_url")]
    pub cdp_url: String,
    #[serde(default = "default_max_pool_size")]
    pub max_pool_size: usize,
    #[serde(default = "default_page_timeout_secs")]
    pub page_timeout_secs: u64,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    pub encryption_key: Option<String>,
    #[serde(default = "default_interval_secs")]
    pub refresh_interval_secs: u64,
    #[serde(default = "default_retry_max")]
    pub refresh_retry_max: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            grpc_addr: default_grpc_addr(),
            nats_url: default_nats_url(),
            cdp_url: default_cdp_url(),
            max_pool_size: default_max_pool_size(),
            page_timeout_secs: default_page_timeout_secs(),
            data_dir: default_data_dir(),
            encryption_key: None,
            refresh_interval_secs: default_interval_secs(),
            refresh_retry_max: default_retry_max(),
        }
    }
}

impl Config {
    pub fn from_file(path: &std::path::Path) -> crate::Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::ImauthError::Config(format!("Failed to read config file: {e}")))?;
        let mut cfg: Config = toml::from_str(&content)
            .map_err(|e| crate::ImauthError::Config(format!("Failed to parse config: {e}")))?;

        // Override with env vars
        if let Ok(addr) = std::env::var("IMAUTH_GRPC_ADDR") {
            cfg.grpc_addr = addr;
        }
        if let Ok(url) = std::env::var("IMAUTH_NATS_URL") {
            cfg.nats_url = url;
        }
        if let Ok(url) = std::env::var("IMAUTH_CDP_URL") {
            cfg.cdp_url = url;
        }
        if let Ok(key) = std::env::var("IMAUTH_ENCRYPTION_KEY") {
            cfg.encryption_key = Some(key);
        }
        if let Ok(dir) = std::env::var("IMAUTH_DATA_DIR") {
            cfg.data_dir = PathBuf::from(dir);
        }

        Ok(cfg)
    }

    pub fn load() -> crate::Result<Self> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let path = home.join(".imauth").join("config.toml");
        Self::from_file(&path)
    }

    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("imauth.db")
    }

    pub fn cookies_dir(&self) -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        home.join(".imauth").join("cookies")
    }
}
