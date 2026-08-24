use clap::{Parser, Subcommand};
use imauth_proto::generated::v1::Platform as ProtoPlatform;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

pub(crate) fn install_crypto_provider() -> Result<(), &'static str> {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) => Ok(()),
        Err(_) if rustls::crypto::CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => Err("failed to install rustls crypto provider"),
    }
}

#[derive(Parser)]
#[command(name = "imauth")]
#[command(about = "imauth CLI — social auth manager")]
#[command(version)]
pub(crate) struct Cli {
    #[arg(short, long, default_value = "http://localhost:6100")]
    pub(crate) server: String,

    #[arg(short = 'k', long, env = "IMAUTH_API_KEY")]
    pub(crate) api_key: Option<String>,

    #[arg(long, env = "IMAUTH_TLS_CA")]
    pub(crate) tls_ca: Option<PathBuf>,

    #[arg(long, env = "IMAUTH_TLS_DOMAIN")]
    pub(crate) tls_domain: Option<String>,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Log in to a platform (opens browser for user-driven login)
    Login {
        #[arg(short, long)]
        platform: String,
    },
    /// Check session status
    Status {
        #[arg(short, long)]
        session_id: String,
    },
    #[command(about = "Cancel an active login session")]
    Cancel {
        #[arg(short, long)]
        session_id: String,
    },
    #[command(about = "Validate the stored session cookie for a platform")]
    Validate {
        #[arg(short, long)]
        platform: String,
    },
    #[command(about = "Show connection status for every platform")]
    Connections,
    /// Manage cookies
    Cookies {
        #[arg(short, long)]
        platform: String,
        #[arg(short, long, default_value = "json")]
        format: String,
    },
    /// Manage credentials
    Credentials {
        #[command(subcommand)]
        action: CredentialAction,
    },
    #[command(about = "Record a browser session for provider onboarding")]
    Provider {
        #[command(subcommand)]
        action: ProviderAction,
    },
}

#[derive(Subcommand)]
pub(crate) enum CredentialAction {
    Save {
        #[arg(short, long)]
        platform: String,
        #[arg(short, long)]
        username: String,
        #[arg(
            short = 'w',
            long,
            env = "IMAUTH_PASSWORD",
            hide_env_values = true,
            help = "Password value; prefer IMAUTH_PASSWORD or --password-stdin"
        )]
        password: Option<String>,
        #[arg(
            long,
            conflicts_with = "password",
            help = "Read the password from stdin"
        )]
        password_stdin: bool,
        #[arg(long)]
        twofa_method: Option<String>,
    },
    Get {
        #[arg(short, long)]
        platform: String,
    },
    Delete {
        #[arg(short, long)]
        platform: String,
    },
}

#[derive(Subcommand)]
pub(crate) enum ProviderAction {
    #[command(about = "Capture browser evidence for provider authentication development")]
    Record {
        #[arg(long)]
        url: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        cdp_url: Option<String>,
        #[arg(long, default_value = "datasource/records")]
        output_root: PathBuf,
        #[arg(long)]
        headless: bool,
        #[arg(long, help = "Finish after one final checkpoint without prompting")]
        auto_finish: bool,
        #[arg(
            long,
            help = "Also capture trace, screenshots, raw data, and JavaScript bodies"
        )]
        deep: bool,
    },
}

pub(crate) fn platform_to_proto(platform: &str) -> Result<i32, String> {
    match platform.to_lowercase().as_str() {
        "instagram" => Ok(ProtoPlatform::Instagram as i32),
        "threads" => Ok(ProtoPlatform::Threads as i32),
        "naver" => Ok(ProtoPlatform::Naver as i32),
        "novelpia" => Ok(ProtoPlatform::Novelpia as i32),
        "munpia" => Ok(ProtoPlatform::Munpia as i32),
        other => Err(format!(
            "Unknown platform '{other}'. Expected one of: instagram, threads, naver, novelpia, munpia"
        )),
    }
}

pub(crate) fn resolve_password(
    password: Option<String>,
    password_stdin: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    if password_stdin {
        let mut value = String::new();
        io::stdin().read_to_string(&mut value)?;
        let trimmed_len = value.trim_end_matches(['\r', '\n']).len();
        value.truncate(trimmed_len);
        if value.is_empty() {
            return Err("Password from stdin must not be empty".into());
        }
        return Ok(value);
    }

    password.ok_or_else(|| {
        "Provide a password with IMAUTH_PASSWORD, --password-stdin, or --password".into()
    })
}

pub(crate) fn with_api_key<T>(
    req: tonic::Request<T>,
    key: &Option<String>,
) -> Result<tonic::Request<T>, tonic::metadata::errors::InvalidMetadataValue> {
    let mut req = req;
    if let Some(k) = key {
        req.metadata_mut()
            .insert("authorization", format!("Bearer {k}").parse()?);
    }
    Ok(req)
}

pub(crate) async fn connect_channel(
    server: &str,
    tls_ca: Option<&Path>,
    tls_domain: Option<&str>,
) -> Result<Channel, Box<dyn std::error::Error>> {
    let mut endpoint = Endpoint::from_shared(server.to_string())?;
    if server.starts_with("https://") || tls_ca.is_some() || tls_domain.is_some() {
        if !server.starts_with("https://") {
            return Err("TLS options require an https:// server URL".into());
        }

        let mut tls = ClientTlsConfig::new().with_enabled_roots();
        if let Some(path) = tls_ca {
            let pem = std::fs::read(path).map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!("Failed to read TLS CA {}: {error}", path.display()),
                )
            })?;
            tls = tls.ca_certificate(Certificate::from_pem(pem));
        }
        if let Some(domain) = tls_domain {
            tls = tls.domain_name(domain);
        }
        endpoint = endpoint.tls_config(tls)?;
    }
    Ok(endpoint.connect().await?)
}
