use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_grpc_addr")]
    pub grpc_addr: String,
    #[serde(default)]
    pub metrics_port: Option<u16>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserConfig {
    #[serde(default = "default_cdp_url")]
    pub cdp_url: String,
    /// Comma-separated CDP URLs for pooled Chrome instances.
    /// When set, overrides `cdp_url`.
    #[serde(default)]
    pub cdp_urls: Option<String>,
    #[serde(default = "default_max_pool_size")]
    pub max_pool_size: usize,
    #[serde(default = "default_page_timeout_secs")]
    pub page_timeout_secs: u64,
    #[serde(default)]
    pub novnc_base_url: Option<String>,
    /// Comma-separated noVNC port numbers, one per Chrome instance.
    #[serde(default)]
    pub novnc_ports: Option<String>,
    #[serde(default = "default_login_timeout_secs")]
    pub login_timeout_secs: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StorageConfig {
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    #[serde(default)]
    pub database_url: Option<String>,
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

fn default_cdp_url() -> String {
    "http://127.0.0.1:9222".to_string()
}

fn default_max_pool_size() -> usize {
    3
}

fn default_page_timeout_secs() -> u64 {
    30
}

fn default_login_timeout_secs() -> u64 {
    300
}

fn default_metrics_port() -> u16 {
    9090
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
                metrics_port: None,
            },
            browser: BrowserConfig {
                cdp_url: default_cdp_url(),
                cdp_urls: None,
                max_pool_size: default_max_pool_size(),
                page_timeout_secs: default_page_timeout_secs(),
                novnc_base_url: None,
                novnc_ports: None,
                login_timeout_secs: default_login_timeout_secs(),
            },
            storage: StorageConfig {
                data_dir: default_data_dir(),
                database_url: None,
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
        if let Ok(url) = std::env::var("IMAUTH_CDP_URL") {
            self.browser.cdp_url = url;
        }
        if let Ok(urls) = std::env::var("IMAUTH_CDP_URLS") {
            self.browser.cdp_urls = Some(urls);
        }
        if let Ok(key) = std::env::var("IMAUTH_ENCRYPTION_KEY") {
            self.security.encryption_key = Some(key);
        }
        if let Ok(dir) = std::env::var("IMAUTH_DATA_DIR") {
            self.storage.data_dir = PathBuf::from(dir);
        }
        if let Ok(url) = std::env::var("IMAUTH_NOVNC_BASE_URL") {
            self.browser.novnc_base_url = Some(url);
        }
        if let Ok(ports) = std::env::var("IMAUTH_NOVNC_PORTS") {
            self.browser.novnc_ports = Some(ports);
        }
        if let Ok(secs) = std::env::var("IMAUTH_LOGIN_TIMEOUT_SECS") {
            if let Ok(s) = secs.parse() {
                self.browser.login_timeout_secs = s;
            }
        }
        if let Ok(url) = std::env::var("IMAUTH_DATABASE_URL") {
            self.storage.database_url = Some(url);
        }
        if let Ok(port) = std::env::var("IMAUTH_METRICS_PORT") {
            if let Ok(p) = port.parse() {
                self.server.metrics_port = Some(p);
            }
        }

        if self.security.encryption_key.as_deref() == Some("") {
            self.security.encryption_key = None;
        }
        self.storage.data_dir = expand_tilde(std::mem::take(&mut self.storage.data_dir));
    }

    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        cfg.apply_env_overrides();
        cfg
    }

    pub fn grpc_addr(&self) -> &str {
        &self.server.grpc_addr
    }

    pub fn cdp_url(&self) -> &str {
        &self.browser.cdp_url
    }

    /// Returns parsed CDP URLs for pooled Chrome instances.
    /// Falls back to single `cdp_url` if `cdp_urls` is not set.
    pub fn cdp_urls(&self) -> Vec<String> {
        if let Some(ref urls) = self.browser.cdp_urls {
            urls.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else {
            vec![self.browser.cdp_url.clone()]
        }
    }

    /// Returns parsed noVNC ports, one per Chrome instance.
    pub fn novnc_ports(&self) -> Vec<u16> {
        if let Some(ref ports) = self.browser.novnc_ports {
            ports
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect()
        } else {
            vec![]
        }
    }

    pub fn max_pool_size(&self) -> usize {
        self.browser.max_pool_size
    }

    pub fn page_timeout_secs(&self) -> u64 {
        self.browser.page_timeout_secs
    }

    pub fn novnc_base_url(&self) -> Option<String> {
        self.browser.novnc_base_url.clone()
    }

    pub fn login_timeout_secs(&self) -> u64 {
        self.browser.login_timeout_secs
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

    /// Returns the configured database URL, or a SQLite file path when unset.
    pub fn database_url(&self) -> String {
        if let Some(ref url) = self.storage.database_url {
            url.clone()
        } else {
            format!("sqlite:{}", self.db_path().display())
        }
    }

    pub fn metrics_port(&self) -> u16 {
        self.server.metrics_port.unwrap_or_else(default_metrics_port)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Env vars are process-global, and `apply_env_overrides` reads several at
    // once. Serialize tests that touch them so cargo's multi-threaded runner
    // doesn't cause flakes. We accept poisoned locks because a panicking test
    // shouldn't poison every later test in the same module.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock_env() -> std::sync::MutexGuard<'static, ()> {
        match ENV_LOCK.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        }
    }

    fn clear_env() {
        for k in [
            "IMAUTH_GRPC_ADDR",
            "IMAUTH_CDP_URL",
            "IMAUTH_ENCRYPTION_KEY",
            "IMAUTH_DATA_DIR",
            "IMAUTH_DATABASE_URL",
            "IMAUTH_METRICS_PORT",
        ] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn config_default_has_expected_values() {
        let cfg = Config::default();
        assert_eq!(cfg.grpc_addr(), "0.0.0.0:50051");
        assert_eq!(cfg.cdp_url(), "http://127.0.0.1:9222");
        assert_eq!(cfg.max_pool_size(), 3);
        assert_eq!(cfg.page_timeout_secs(), 30);
        assert!(cfg.encryption_key().is_none());
        assert_eq!(cfg.refresh_interval_secs(), 3600);
        assert_eq!(cfg.refresh_retry_max(), 3);
    }

    #[test]
    fn config_derived_paths_join_correctly() {
        let mut cfg = Config::default();
        cfg.storage.data_dir = PathBuf::from("/tmp/imauth-test");
        assert_eq!(cfg.db_path(), PathBuf::from("/tmp/imauth-test/imauth.db"));
        assert_eq!(
            cfg.cookies_dir(),
            PathBuf::from("/tmp/imauth-test/cookies")
        );
        assert_eq!(
            cfg.snapshot_dir(),
            PathBuf::from("/tmp/imauth-test/snapshots")
        );
    }

    #[test]
    fn from_file_with_missing_path_returns_defaults() {
        let _guard = lock_env();
        clear_env();

        let missing = std::env::temp_dir().join("imauth-does-not-exist-xyz.toml");
        let _ = std::fs::remove_file(&missing);

        let cfg = Config::from_file(&missing).unwrap();
        assert_eq!(cfg.grpc_addr(), "0.0.0.0:50051");
        assert_eq!(cfg.cdp_url(), "http://127.0.0.1:9222");
    }

    #[test]
    fn from_file_parses_partial_toml_and_fills_defaults() {
        let _guard = lock_env();
        clear_env();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[server]
grpc_addr = "127.0.0.1:9999"

[browser]
max_pool_size = 7
"#,
        )
        .unwrap();

        let cfg = Config::from_file(&path).unwrap();
        assert_eq!(cfg.grpc_addr(), "127.0.0.1:9999");
        assert_eq!(cfg.max_pool_size(), 7);
        // Unset fields within a present section fall back to per-field defaults.
        assert_eq!(cfg.cdp_url(), "http://127.0.0.1:9222");
        assert_eq!(cfg.page_timeout_secs(), 30);
    }

    #[test]
    fn from_file_reports_parse_error_as_config_error() {
        let _guard = lock_env();
        clear_env();

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bad.toml");
        std::fs::write(&path, "this is = not = valid = toml = at all").unwrap();

        let err = Config::from_file(&path).unwrap_err();
        match err {
            crate::ImauthError::Config(msg) => {
                assert!(
                    msg.contains("Failed to parse"),
                    "expected parse error, got: {msg}"
                );
            }
            other => panic!("expected Config error, got {other:?}"),
        }
    }

    #[test]
    fn env_overrides_apply_after_file_load() {
        let _guard = lock_env();
        clear_env();

        std::env::set_var("IMAUTH_GRPC_ADDR", "10.0.0.1:7000");
        std::env::set_var("IMAUTH_CDP_URL", "http://chrome:9223");
        std::env::set_var("IMAUTH_ENCRYPTION_KEY", "test-key-from-env");
        std::env::set_var("IMAUTH_DATA_DIR", "/var/lib/imauth-test");

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let cfg = Config::from_file(&missing).unwrap();

        assert_eq!(cfg.grpc_addr(), "10.0.0.1:7000");
        assert_eq!(cfg.cdp_url(), "http://chrome:9223");
        assert_eq!(cfg.encryption_key(), Some("test-key-from-env"));
        assert_eq!(
            cfg.storage.data_dir,
            PathBuf::from("/var/lib/imauth-test")
        );

        clear_env();
    }

    #[test]
    fn empty_encryption_key_env_is_normalized_to_none() {
        let _guard = lock_env();
        clear_env();

        std::env::set_var("IMAUTH_ENCRYPTION_KEY", "");

        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let cfg = Config::from_file(&missing).unwrap();

        // Empty string env var is treated as "not set".
        assert!(cfg.encryption_key().is_none());

        clear_env();
    }

    #[test]
    fn tilde_in_data_dir_is_expanded_to_home() {
        let _guard = lock_env();
        clear_env();

        std::env::set_var("IMAUTH_DATA_DIR", "~/imauth-tilde-test");
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let cfg = Config::from_file(&missing).unwrap();

        let expanded = cfg.storage.data_dir;
        if let Some(home) = dirs::home_dir() {
            assert_eq!(expanded, home.join("imauth-tilde-test"));
        } else {
            // No home dir available — fall through to the original path.
            assert_eq!(expanded, PathBuf::from("~/imauth-tilde-test"));
        }

        clear_env();
    }

    #[test]
    fn absolute_data_dir_is_left_alone() {
        let _guard = lock_env();
        clear_env();

        std::env::set_var("IMAUTH_DATA_DIR", "/srv/imauth-abs");
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.toml");
        let cfg = Config::from_file(&missing).unwrap();
        assert_eq!(cfg.storage.data_dir, PathBuf::from("/srv/imauth-abs"));

        clear_env();
    }

    #[test]
    fn config_round_trips_through_toml() {
        let original = Config::default();
        let serialized = toml::to_string_pretty(&original).unwrap();
        let parsed: Config = toml::from_str(&serialized).unwrap();
        assert_eq!(parsed.grpc_addr(), original.grpc_addr());
        assert_eq!(parsed.cdp_url(), original.cdp_url());
        assert_eq!(parsed.max_pool_size(), original.max_pool_size());
        assert_eq!(parsed.refresh_interval_secs(), original.refresh_interval_secs());
    }
}
