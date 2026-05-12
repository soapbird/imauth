

use clap::{Parser, Subcommand};
use imauth_proto::generated::v1::{
    auth_service_client::AuthServiceClient, credential_service_client::CredentialServiceClient,
    session_service_client::SessionServiceClient, AuthStatus, DeleteCredentialRequest,
    ExportRequest, GetCookiesRequest, GetCredentialRequest, LoginRequest, SaveCredentialRequest,
    StatusRequest, Submit2FaRequest,
};
use std::io::{self, Write};
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
        #[arg(short = 'w', long)]
        password: String,
        #[arg(
            long = "2fa",
            visible_alias = "twofa",
            value_name = "CODE",
            num_args = 0..=1,
            default_missing_value = ""
        )]
        twofa: Option<Option<String>>,
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
        other => Err(format!(
            "Unknown platform '{other}'. Expected one of: instagram, threads"
        )),
    }
}

fn read_2fa_code() -> io::Result<String> {
    print!("Enter 2FA code: ");
    io::stdout().flush()?;

    let mut code = String::new();
    io::stdin().read_line(&mut code)?;
    Ok(normalize_2fa_code(&code))
}

fn normalize_2fa_code(raw: &str) -> String {
    let trimmed = raw.trim();
    let digits: String = trimmed.chars().filter(|c| c.is_ascii_digit()).collect();

    match digits.len() {
        6 => digits,
        len if len > 6 => digits[len - 6..].to_string(),
        _ => trimmed.to_string(),
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
                platform: platform_to_proto(&platform)?,
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
                            let code = match &twofa {
                                Some(Some(code)) if !code.is_empty() => {
                                    Some(normalize_2fa_code(code))
                                }
                                Some(_) => Some(read_2fa_code()?),
                                None => None,
                            };

                            if let Some(code) = code {
                                let mut client = AuthServiceClient::new(channel.clone());
                                let resp = client
                                    .submit2_fa(tonic::Request::new(Submit2FaRequest {
                                        session_id: event.session_id.clone(),
                                        code,
                                    }))
                                    .await?;
                                let resp = resp.into_inner();
                                println!(
                                    "2FA response: success={} session_id={} message={} cookies={}",
                                    resp.success,
                                    resp.session_id,
                                    resp.message,
                                    resp.cookies.len()
                                );
                                break;
                            } else {
                                println!("2FA required but no code provided. Use --2fa <CODE> or --2fa to enter it interactively.");
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
                        platform: platform_to_proto(&platform)?,
                    }))
                    .await?;
                println!("{}", resp.into_inner().content);
            } else {
                let resp = client
                    .get_cookies(tonic::Request::new(GetCookiesRequest {
                        platform: platform_to_proto(&platform)?,
                        domains: vec![],
                    }))
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
                        .save(tonic::Request::new(SaveCredentialRequest {
                            platform: platform_to_proto(&platform)?,
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
                            platform: platform_to_proto(&platform)?,
                        }))
                        .await?;
                    println!("{:#?}", resp.into_inner());
                }
                CredentialAction::Delete { platform } => {
                    let resp = client
                        .delete(tonic::Request::new(DeleteCredentialRequest {
                            platform: platform_to_proto(&platform)?,
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

#[cfg(test)]
mod tests {
    use super::normalize_2fa_code;

    #[test]
    fn normalize_2fa_code_keeps_six_digits_including_leading_zero() {
        assert_eq!(normalize_2fa_code(" 007594\n"), "007594");
    }

    #[test]
    fn normalize_2fa_code_uses_last_six_digits_when_terminal_input_was_prefixed() {
        assert_eq!(normalize_2fa_code("746736362"), "736362");
    }

    #[test]
    fn normalize_2fa_code_strips_separators() {
        assert_eq!(normalize_2fa_code("123 456"), "123456");
    }
}
