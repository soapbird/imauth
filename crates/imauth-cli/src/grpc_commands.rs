use crate::cli_support::{
    connect_channel, platform_to_proto, resolve_password, with_api_key, Commands, CredentialAction,
};
use imauth_proto::generated::v1::{
    auth_service_client::AuthServiceClient, credential_service_client::CredentialServiceClient,
    session_service_client::SessionServiceClient, AuthStatus, CancelRequest,
    DeleteCredentialRequest, Empty, ExportRequest, GetCookiesRequest, GetCredentialRequest,
    LoginRequest, SaveCredentialRequest, StatusRequest, ValidateRequest,
};
use std::io::{self, Write};
use std::path::PathBuf;

pub(crate) async fn run(
    command: Commands,
    server: String,
    api_key: Option<String>,
    tls_ca: Option<PathBuf>,
    tls_domain: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let channel = connect_channel(&server, tls_ca.as_deref(), tls_domain.as_deref()).await?;
    match command {
        Commands::Login { platform } => {
            let mut client = AuthServiceClient::new(channel.clone());
            let request = with_api_key(
                tonic::Request::new(LoginRequest {
                    platform: platform_to_proto(&platform)?,
                }),
                &api_key,
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
            let response = client
                .get_status(with_api_key(
                    tonic::Request::new(StatusRequest { session_id }),
                    &api_key,
                )?)
                .await?;
            println!("{:#?}", response.into_inner());
        }
        Commands::Cancel { session_id } => {
            let mut client = AuthServiceClient::new(channel.clone());
            let response = client
                .cancel(with_api_key(
                    tonic::Request::new(CancelRequest { session_id }),
                    &api_key,
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
                    &api_key,
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
                .get_connection_status(with_api_key(tonic::Request::new(Empty {}), &api_key)?)
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
                let response = client
                    .export_netscape(with_api_key(
                        tonic::Request::new(ExportRequest {
                            platform: platform_to_proto(&platform)?,
                        }),
                        &api_key,
                    )?)
                    .await?;
                println!("{}", response.into_inner().content);
            } else {
                let response = client
                    .get_cookies(with_api_key(
                        tonic::Request::new(GetCookiesRequest {
                            platform: platform_to_proto(&platform)?,
                            domains: vec![],
                        }),
                        &api_key,
                    )?)
                    .await?;
                for cookie in response.into_inner().cookies {
                    println!(
                        "{}={} (domain: {}, path: {})",
                        cookie.name, cookie.value, cookie.domain, cookie.path
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
                    let response = client
                        .save(with_api_key(
                            tonic::Request::new(SaveCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                                username,
                                password,
                                twofa_method: twofa_method.unwrap_or_default(),
                            }),
                            &api_key,
                        )?)
                        .await?;
                    println!("{:#?}", response.into_inner());
                }
                CredentialAction::Get { platform } => {
                    let response = client
                        .get(with_api_key(
                            tonic::Request::new(GetCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                            }),
                            &api_key,
                        )?)
                        .await?;
                    println!("{:#?}", response.into_inner());
                }
                CredentialAction::Delete { platform } => {
                    let response = client
                        .delete(with_api_key(
                            tonic::Request::new(DeleteCredentialRequest {
                                platform: platform_to_proto(&platform)?,
                            }),
                            &api_key,
                        )?)
                        .await?;
                    println!("{:#?}", response.into_inner());
                }
            }
        }
        Commands::Provider { .. } => return Err("Provider commands must run locally".into()),
    }
    Ok(())
}
