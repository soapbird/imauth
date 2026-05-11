use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NatsConfig {
    #[serde(default = "default_nats_url")]
    pub url: String,
    #[serde(default = "default_stream_name")]
    pub stream_name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserConfig {
    #[serde(default = "default_cdp_url")]
    pub cdp_url: String,
    #[serde(default = "default_max_pool_size")]
    pub max_pool_size: usize,
    #[serde(default = "default_page_timeout_secs")]
    pub page_timeout_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub encryption_key: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RefreshConfig {
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_retry_max")]
    pub retry_max: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub server: ServerConfig,
    #[serde(default)]
    pub nats: NatsConfig,
    #[serde(default)]
    pub browser: BrowserConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub refresh: RefreshConfig,
}

fn default_grpc_addr() -> String {
    "0.0.0.0:50051".to_string()
}

fn default_nats_url() -> String {
    "nats://localhost:4222".to_string()
}

fn default_stream_name() -> String {
    "imauth".to_string()
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

fn expand_tilde(path: PathBuf) -> PathBuf {
    let Some(s) = path.to_str() else { return path };
    let Some(rest) = s.strip_prefix("~/") else {
        return path;
    };
    match dirs::home_dir() {
        Some(home) => home.join(rest),
        None => path,
    }
}

fn default_interval_secs() -> u64 {
    3600
}

fn default_retry_max() -> u32 {
    3
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                grpc_addr: default_grpc_addr(),
            },
            nats: NatsConfig {
                url: default_nats_url(),
                stream_name: default_stream_name(),
            },
            browser: BrowserConfig {
                cdp_url: default_cdp_url(),
                max_pool_size: default_max_pool_size(),
                page_timeout_secs: default_page_timeout_secs(),
            },
            storage: StorageConfig {
                data_dir: default_data_dir(),
            },
            security: SecurityConfig {
                encryption_key: None,
            },
            refresh: RefreshConfig {
                interval_secs: default_interval_secs(),
                retry_max: default_retry_max(),
            },
        }
    }
}

impl Config {
    pub fn from_file(path: &std::path::Path) -> crate::Result<Self> {
        let mut cfg = if path.exists() {
            let content = std::fs::read_to_string(path).map_err(|e| {
                crate::ImauthError::Config(format!("Failed to read config file: {e}"))
            })?;
            toml::from_str(&content)
                .map_err(|e| crate::ImauthError::Config(format!("Failed to parse config: {e}")))?
        } else {
            Self::default()
        };

        cfg.apply_env_overrides();
        Ok(cfg)
    }

    fn apply_env_overrides(&mut self) {
        if let Ok(addr) = std::env::var("IMAUTH_GRPC_ADDR") {
            self.server.grpc_addr = addr;
        }
        if let Ok(url) = std::env::var("IMAUTH_NATS_URL") {
            self.nats.url = url;
        }
        if let Ok(url) = std::env::var("IMAUTH_CDP_URL") {
            self.browser.cdp_url = url;
        }
        if let Ok(key) = std::env::var("IMAUTH_ENCRYPTION_KEY") {
            self.security.encryption_key = Some(key);
        }
        if let Ok(dir) = std::env::var("IMAUTH_DATA_DIR") {
            self.storage.data_dir = PathBuf::from(dir);
        }

        if self.security.encryption_key.as_deref() == Some("") {
            self.security.encryption_key = None;
        }
        self.storage.data_dir = expand_tilde(std::mem::take(&mut self.storage.data_dir));
    }

    pub fn load() -> crate::Result<Self> {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let dir = home.join(".imauth");
        let path = dir.join("config.toml");

        if !path.exists() {
            std::fs::create_dir_all(&dir).map_err(|e| {
                crate::ImauthError::Config(format!(
                    "Failed to create config dir {}: {e}",
                    dir.display()
                ))
            })?;

            let toml = toml::to_string_pretty(&Self::default()).map_err(|e| {
                crate::ImauthError::Config(format!("Failed to serialize config: {e}"))
            })?;
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(toml.as_bytes()).map_err(|e| {
                        crate::ImauthError::Config(format!("Failed to write default config: {e}"))
                    })?;
                    tracing::info!("Created default config at {}", path.display());
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => {
                    return Err(crate::ImauthError::Config(format!(
                        "Failed to create default config: {e}"
                    )))
                }
            }
        }

        Self::from_file(&path)
    }

    pub fn grpc_addr(&self) -> &str {
        &self.server.grpc_addr
    }

    pub fn nats_url(&self) -> &str {
        &self.nats.url
    }

    pub fn cdp_url(&self) -> &str {
        &self.browser.cdp_url
    }

    pub fn max_pool_size(&self) -> usize {
        self.browser.max_pool_size
    }

    pub fn page_timeout_secs(&self) -> u64 {
        self.browser.page_timeout_secs
    }

    pub fn db_path(&self) -> PathBuf {
        self.storage.data_dir.join("imauth.db")
    }

    pub fn cookies_dir(&self) -> PathBuf {
        self.storage.data_dir.join("cookies")
    }

    pub fn snapshot_dir(&self) -> PathBuf {
        self.storage.data_dir.join("snapshots")
    }

    pub fn encryption_key(&self) -> Option<&str> {
        self.security.encryption_key.as_deref()
    }

    pub fn refresh_interval_secs(&self) -> u64 {
        self.refresh.interval_secs
    }

    pub fn refresh_retry_max(&self) -> u32 {
        self.refresh.retry_max
    }
}
