mod generated;

use clap::{Parser, Subcommand};
use generated::v1::{
    auth_service_client::AuthServiceClient,
    credential_service_client::CredentialServiceClient,
    session_service_client::SessionServiceClient,
    AuthEvent, DeleteCredentialRequest, ExportRequest, GetCookiesRequest, GetCredentialRequest,
    LoginRequest, Platform as ProtoPlatform, SaveCredentialRequest, StatusRequest,
    Submit2FaRequest,
};
use tonic::transport::Channel;

#[derive(Parser)]
#[command(name = "imauth")]
#[command(about = "imauth CLI — social auth manager")]
struct Cli {
    #[arg(short, long, default_value = "http://localhost:50051")]
    server: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Log in to a platform
    Login {
        #[arg(short, long)]
        platform: String,
        #[arg(short, long)]
        username: String,
        #[arg(short, long)]
        password: String,
        #[arg(long)]
        twofa: Option<String>,
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
        #[arg(short, long)]
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

fn platform_to_proto(platform: &str) -> i32 {
    match platform.to_lowercase().as_str() {
        "instagram" => 1,
        "threads" => 2,
        _ => 0,
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    let channel = Channel::from_shared(cli.server.clone())?.connect().await?;

    match cli.command {
        Commands::Login {
            platform,
            username,
            password,
            twofa,
        } => {
            let mut client = AuthServiceClient::new(channel.clone());

            let request = tonic::Request::new(LoginRequest {
                platform: platform_to_proto(&platform),
                username,
                password,
            });

            let mut stream = client.login(request).await?.into_inner();

            while let Some(event) = stream.message().await? {
                println!("Status: {:?}", event.status);
                println!("Message: {}", event.message);

                if event.requires_input {
                    match event.input_type.as_str() {
                        "2fa_code" => {
                            if let Some(code) = &twofa {
                                let mut client = AuthServiceClient::new(channel.clone());
                                let resp = client
                                    .submit2_fa(tonic::Request::new(Submit2FaRequest {
                                        session_id: event
                                            .status
                                            .to_string(),
                                        code: code.clone(),
                                    }))
                                    .await?;
                                println!("2FA response: {:?}", resp.into_inner());
                            } else {
                                println!("2FA required but no code provided. Use --2fa");
                                break;
                            }
                        }
                        "captcha" => {
                            println!("Captcha required — not yet supported in CLI");
                            break;
                        }
                        _ => {}
                    }
                }

                if event.status == 7 {
                    // Connected
                    println!("Login successful! Cookies: {}", event.cookies.len());
                    break;
                }
                if event.status == 8 {
                    // Failed
                    println!("Login failed: {}", event.message);
                    break;
                }
            }
        }

        Commands::Status { session_id } => {
            let mut client = AuthServiceClient::new(channel);
            let resp = client
                .get_status(tonic::Request::new(StatusRequest { session_id }))
                .await?;
            println!("{:#?}", resp.into_inner());
        }

        Commands::Cookies { platform, format } => {
            let mut client = SessionServiceClient::new(channel.clone());

            if format == "netscape" {
                let resp = client
                    .export_netscape(tonic::Request::new(ExportRequest {
                        platform: platform_to_proto(&platform),
                    }))
                    .await?;
                println!("{}", resp.into_inner().content);
            } else {
                let resp = client
                    .get_cookies(tonic::Request::new(GetCookiesRequest {
                        platform: platform_to_proto(&platform),
                        domains: vec![],
                    }))
                    .await?;
                let cookies = resp.into_inner().cookies;
                for c in cookies {
                    println!("{}={} (domain: {}, path: {})", c.name, c.value, c.domain, c.path);
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
                        .save(tonic::Request::new(SaveCredentialRequest {
                            platform: platform_to_proto(&platform),
                            username,
                            password,
                            twofa_method: twofa_method.unwrap_or_default(),
                        }))
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
                CredentialAction::Get { platform } => {
                    let resp = client
                        .get(tonic::Request::new(GetCredentialRequest {
                            platform: platform_to_proto(&platform),
                        }))
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
                CredentialAction::Delete { platform } => {
                    let resp = client
                        .delete(tonic::Request::new(DeleteCredentialRequest {
                            platform: platform_to_proto(&platform),
                        }))
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
