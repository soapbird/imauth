use clap::{Parser, Subcommand};
use imauth_proto::generated::v1::{
    auth_service_client::AuthServiceClient, credential_service_client::CredentialServiceClient,
    session_service_client::SessionServiceClient, AuthStatus, CancelRequest,
    DeleteCredentialRequest, Empty, ExportRequest, GetCookiesRequest, GetCredentialRequest,
    LoginRequest, Platform as ProtoPlatform, SaveCredentialRequest, StatusRequest, ValidateRequest,
};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tonic::transport::{Certificate, Channel, ClientTlsConfig, Endpoint};

fn install_crypto_provider() -> Result<(), &'static str> {
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
struct Cli {
    #[arg(short, long, default_value = "http://localhost:6100")]
    server: String,

    #[arg(short = 'k', long, env = "IMAUTH_API_KEY")]
    api_key: Option<String>,

    #[arg(long, env = "IMAUTH_TLS_CA")]
    tls_ca: Option<PathBuf>,

    #[arg(long, env = "IMAUTH_TLS_DOMAIN")]
    tls_domain: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
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
}

#[derive(Subcommand)]
enum CredentialAction {
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

fn platform_to_proto(platform: &str) -> Result<i32, String> {
    match platform.to_lowercase().as_str() {
        "instagram" => Ok(ProtoPlatform::Instagram as i32),
        "threads" => Ok(ProtoPlatform::Threads as i32),
        "naver" => Ok(ProtoPlatform::Naver as i32),
        other => Err(format!(
            "Unknown platform '{other}'. Expected one of: instagram, threads, naver"
        )),
    }
}

fn resolve_password(
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

fn with_api_key<T>(
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

async fn connect_channel(
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_crypto_provider()?;
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let channel = connect_channel(
        &cli.server,
        cli.tls_ca.as_deref(),
        cli.tls_domain.as_deref(),
    )
    .await?;

    match cli.command {
        Commands::Login { platform } => {
            let mut client = AuthServiceClient::new(channel.clone());

            let request = with_api_key(
                tonic::Request::new(LoginRequest {
                    platform: platform_to_proto(&platform)?,
                }),
                &cli.api_key,
            )?;

            let mut stream = client.login(request).await?.into_inner();

            while let Some(event) = stream.message().await? {
                let status = AuthStatus::try_from(event.status).unwrap_or(AuthStatus::Unspecified);
                println!("Status: {}", status.as_str_name());
                println!("Message: {}", event.message);

                if !event.viewer_url.is_empty() {
                    println!();
                    println!("Open this URL in your browser to log in:");
                    println!("  {}", event.viewer_url);
                    println!();
                    print!("Waiting for login... ");
                    io::stdout().flush()?;
                }

                match status {
                    AuthStatus::Connected => {
                        println!("Login successful! Cookies: {}", event.cookies.len());
                        break;
                    }
                    AuthStatus::Failed => {
                        println!("Login failed: {}", event.message);
                        break;
                    }
                    _ => {}
                }
            }
        }
        Commands::Status { session_id } => {
            let mut client = AuthServiceClient::new(channel);
            let resp = client
                .get_status(with_api_key(
                    tonic::Request::new(StatusRequest { session_id }),
                    &cli.api_key,
                )?)
                .await?;
            println!("{:#?}", resp.into_inner());
        }
        Commands::Cancel { session_id } => {
            let mut client = AuthServiceClient::new(channel.clone());
            let response = client
                .cancel(with_api_key(
                    tonic::Request::new(CancelRequest { session_id }),
                    &cli.api_key,
                )?)
                .await?
                .into_inner();
            println!("{}", response.message);
        }
        Commands::Validate { platform } => {
            let mut client = SessionServiceClient::new(channel.clone());
            let response = client
                .validate_session(with_api_key(
                    tonic::Request::new(ValidateRequest {
                        platform: platform_to_proto(&platform)?,
                    }),
                    &cli.api_key,
                )?)
                .await?
                .into_inner();
            println!("Valid: {}", response.valid);
            println!("Session cookie: {}", response.session_cookie_name);
            println!("Expires at: {}", response.expires_at);
        }
        Commands::Connections => {
            let mut client = SessionServiceClient::new(channel.clone());
            let response = client
                .get_connection_status(with_api_key(tonic::Request::new(Empty {}), &cli.api_key)?)
                .await?
                .into_inner();
            let mut platforms: Vec<_> = response.platforms.into_iter().collect();
            platforms.sort_by(|left, right| left.0.cmp(&right.0));
            for (platform, connected) in platforms {
                println!("{platform}: {connected}");
            }
        }

        Commands::Cookies { platform, format } => {
            let mut client = SessionServiceClient::new(channel.clone());

            if format == "netscape" {
                let resp = client
                    .export_netscape(with_api_key(
                        tonic::Request::new(ExportRequest {
                            platform: platform_to_proto(&platform)?,
                        }),
                        &cli.api_key,
                    )?)
                    .await?;
                println!("{}", resp.into_inner().content);
            } else {
                let resp = client
                    .get_cookies(with_api_key(
                        tonic::Request::new(GetCookiesRequest {
                            platform: platform_to_proto(&platform)?,
                            domains: vec![],
                        }),
                        &cli.api_key,
                    )?)
                    .await?;
                let cookies = resp.into_inner().cookies;
                for c in cookies {
                    println!(
                        "{}={} (domain: {}, path: {})",
                        c.name, c.value, c.domain, c.path
                    );
                }
            }
        }

        Commands::Credentials { action } => {
            let mut client = CredentialServiceClient::new(channel);
            match action {
                CredentialAction::Save {
                    platform,
                    username,
                    password,
                    password_stdin,
                    twofa_method,
                } => {
                    let password = resolve_password(password, password_stdin)?;
                    let resp = client
                        .save(with_api_key(
                            tonic::Request::new(SaveCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                                username,
                                password,
                                twofa_method: twofa_method.unwrap_or_default(),
                            }),
                            &cli.api_key,
                        )?)
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
                CredentialAction::Get { platform } => {
                    let resp = client
                        .get(with_api_key(
                            tonic::Request::new(GetCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                            }),
                            &cli.api_key,
                        )?)
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
                CredentialAction::Delete { platform } => {
                    let resp = client
                        .delete(with_api_key(
                            tonic::Request::new(DeleteCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                            }),
                            &cli.api_key,
                        )?)
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        install_crypto_provider, platform_to_proto, resolve_password, with_api_key, Cli, Commands,
        CredentialAction,
    };
    use clap::Parser;
    use std::path::PathBuf;

    #[test]
    fn install_crypto_provider_sets_process_default() {
        // Given: the CLI binary is built with transitive rustls providers.
        // When: CLI startup selects its provider.
        install_crypto_provider().unwrap();

        // Then: TLS construction has an unambiguous process default.
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn cli_tls_options_are_absent_by_default() {
        // Given: a normal plaintext CLI invocation.
        let cli = Cli::try_parse_from(["imauth", "status", "--session-id", "session-1"]).unwrap();

        // When: TLS arguments are inspected.
        // Then: plaintext behavior remains the default.
        assert_eq!(cli.tls_ca, None);
        assert_eq!(cli.tls_domain, None);
    }

    #[test]
    fn cli_accepts_tls_ca_and_domain_options() {
        // Given: a CLI invocation opting into a private CA and domain override.
        let cli = Cli::try_parse_from([
            "imauth",
            "--server",
            "https://127.0.0.1:7443",
            "--tls-ca",
            "test-ca.pem",
            "--tls-domain",
            "imauth.internal",
            "status",
            "--session-id",
            "session-1",
        ])
        .unwrap();

        // When: TLS arguments are inspected.
        // Then: both options retain their exact values.
        assert_eq!(cli.tls_ca, Some(PathBuf::from("test-ca.pem")));
        assert_eq!(cli.tls_domain.as_deref(), Some("imauth.internal"));
    }

    #[test]
    fn cli_exposes_cancel_validate_and_connections_commands() {
        let cancel =
            Cli::try_parse_from(["imauth", "cancel", "--session-id", "session-1"]).unwrap();
        assert!(matches!(cancel.command, Commands::Cancel { .. }));

        let validate =
            Cli::try_parse_from(["imauth", "validate", "--platform", "instagram"]).unwrap();
        assert!(matches!(validate.command, Commands::Validate { .. }));

        let connections = Cli::try_parse_from(["imauth", "connections"]).unwrap();
        assert!(matches!(connections.command, Commands::Connections));
    }

    #[test]
    fn credential_save_accepts_password_from_environment_shape() {
        let cli = Cli::try_parse_from([
            "imauth",
            "credentials",
            "save",
            "--platform",
            "instagram",
            "--username",
            "user",
            "--password",
            "secret",
        ])
        .unwrap();

        assert!(matches!(
            cli.command,
            Commands::Credentials {
                action: CredentialAction::Save {
                    password: Some(password),
                    password_stdin: false,
                    ..
                }
            } if password == "secret"
        ));
        assert_eq!(
            resolve_password(Some("secret".into()), false).unwrap(),
            "secret"
        );
    }

    #[test]
    fn platform_to_proto_accepts_known_platforms_case_insensitive() {
        assert_eq!(platform_to_proto("instagram").unwrap(), 1);
        assert_eq!(platform_to_proto("Instagram").unwrap(), 1);
        assert_eq!(platform_to_proto("THREADS").unwrap(), 2);
        assert_eq!(platform_to_proto("naver").unwrap(), 3);
        assert_eq!(platform_to_proto("NAVER").unwrap(), 3);
    }

    #[test]
    fn platform_to_proto_rejects_unknown_platform_with_helpful_message() {
        let err = platform_to_proto("facebook").unwrap_err();
        assert!(err.contains("facebook"));
        assert!(err.contains("naver"));
    }

    #[test]
    fn with_api_key_does_not_set_authorization_when_key_is_none() {
        let req: tonic::Request<()> = tonic::Request::new(());
        let req = with_api_key(req, &None).unwrap();
        assert!(req.metadata().get("authorization").is_none());
    }

    #[test]
    fn with_api_key_sets_bearer_authorization_when_key_present() {
        let req: tonic::Request<()> = tonic::Request::new(());
        let req = with_api_key(req, &Some("secret-key".to_string())).unwrap();
        let val = req
            .metadata()
            .get("authorization")
            .expect("authorization header set")
            .to_str()
            .unwrap();
        assert_eq!(val, "Bearer secret-key");
    }
}
