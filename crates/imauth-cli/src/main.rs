use clap::{Parser, Subcommand};
use imauth_proto::generated::v1::{
    auth_service_client::AuthServiceClient, credential_service_client::CredentialServiceClient,
    session_service_client::SessionServiceClient, AuthStatus, DeleteCredentialRequest,
    ExportRequest, GetCookiesRequest, GetCredentialRequest, LoginRequest, SaveCredentialRequest,
    StatusRequest,
};
use std::io::{self, Write};
use tonic::transport::Channel;

#[derive(Parser)]
#[command(name = "imauth")]
#[command(about = "imauth CLI — social auth manager")]
struct Cli {
    #[arg(short, long, default_value = "http://localhost:6100")]
    server: String,

    #[arg(short = 'k', long, env = "IMAUTH_API_KEY")]
    api_key: Option<String>,

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
    /// Refresh session
    Refresh {
        #[arg(short, long)]
        platform: String,
    },
}

#[derive(Subcommand)]
enum CredentialAction {
    Save {
        #[arg(short, long)]
        platform: String,
        #[arg(short, long)]
        username: String,
        #[arg(short = 'w', long)]
        password: String,
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
        "instagram" => Ok(1),
        "threads" => Ok(2),
        "naver" => Ok(3),
        other => Err(format!(
            "Unknown platform '{other}'. Expected one of: instagram, threads, naver"
        )),
    }
}

fn with_api_key<T>(req: tonic::Request<T>, key: &Option<String>) -> tonic::Request<T> {
    let mut req = req;
    if let Some(k) = key {
        req.metadata_mut().insert(
            "authorization",
            format!("Bearer {}", k)
                .parse()
                .expect("valid ascii api key"),
        );
    }
    req
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let channel = Channel::from_shared(cli.server.clone())?.connect().await?;

    match cli.command {
        Commands::Login { platform } => {
            let mut client = AuthServiceClient::new(channel.clone());

            let request = with_api_key(
                tonic::Request::new(LoginRequest {
                    platform: platform_to_proto(&platform)?,
                }),
                &cli.api_key,
            );

            let mut stream = client.login(request).await?.into_inner();

            while let Some(event) = stream.message().await? {
                println!("Status: {:?}", event.status);
                println!("Message: {}", event.message);

                if !event.viewer_url.is_empty() {
                    println!();
                    println!("Open this URL in your browser to log in:");
                    println!("  {}", event.viewer_url);
                    println!();
                    print!("Waiting for login... ");
                    io::stdout().flush()?;
                }

                match AuthStatus::try_from(event.status) {
                    Ok(AuthStatus::Connected) => {
                        println!("Login successful! Cookies: {}", event.cookies.len());
                        break;
                    }
                    Ok(AuthStatus::Failed) => {
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
                ))
                .await?;
            println!("{:#?}", resp.into_inner());
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
                    ))
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
                    ))
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
                    twofa_method,
                } => {
                    let resp = client
                        .save(with_api_key(
                            tonic::Request::new(SaveCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                                username,
                                password,
                                twofa_method: twofa_method.unwrap_or_default(),
                            }),
                            &cli.api_key,
                        ))
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
                        ))
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
                        ))
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
            }
        }

        Commands::Refresh { platform } => {
            println!("Refresh not yet implemented for {}", platform);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{platform_to_proto, with_api_key};

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
        let req = with_api_key(req, &None);
        assert!(req.metadata().get("authorization").is_none());
    }

    #[test]
    fn with_api_key_sets_bearer_authorization_when_key_present() {
        let req: tonic::Request<()> = tonic::Request::new(());
        let req = with_api_key(req, &Some("secret-key".to_string()));
        let val = req
            .metadata()
            .get("authorization")
            .expect("authorization header set")
            .to_str()
            .unwrap();
        assert_eq!(val, "Bearer secret-key");
    }
}
