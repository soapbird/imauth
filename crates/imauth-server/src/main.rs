#![allow(clippy::result_large_err)]

use clap::Parser;
use imauth_core::AppContainer;
use imauth_proto::generated::v1::{
    auth_service_server::AuthServiceServer, credential_service_server::CredentialServiceServer,
    session_service_server::SessionServiceServer,
};
use imauth_server::cli::{Cli, Commands};
use imauth_server::grpc::{AuthGrpcService, CredentialGrpcService, SessionGrpcService};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tonic::transport::Server;

/// Returns the configured API key, or `None` when it is missing, empty, or
/// whitespace-only. An empty `IMAUTH_API_KEY` must NOT enable auth — otherwise
/// callers could authenticate with `Authorization: Bearer ` (empty token).
fn normalize_api_key(raw: Option<String>) -> Option<String> {
    raw.map(|k| k.trim().to_string()).filter(|k| !k.is_empty())
}

fn auth_interceptor(
    api_key: Option<std::sync::Arc<String>>,
) -> impl Fn(tonic::Request<()>) -> Result<tonic::Request<()>, tonic::Status> + Clone {
    move |req: tonic::Request<()>| {
        if let Some(ref key) = api_key {
            let provided = req
                .metadata()
                .get("authorization")
                .and_then(|v| v.to_str().ok())
                .and_then(|s| s.strip_prefix("Bearer "))
                .or_else(|| {
                    req.metadata()
                        .get("x-api-key")
                        .and_then(|v| v.to_str().ok())
                });

            match provided {
                Some(k) if bool::from(k.as_bytes().ct_eq(key.as_bytes())) => Ok(req),
                _ => Err(tonic::Status::unauthenticated("Invalid or missing API key")),
            }
        } else {
            Ok(req)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            config,
            port,
            api_key,
        } => {
            let mut cfg = if let Some(path) = config {
                imauth_core::Config::from_file(&path)?
            } else {
                imauth_core::Config::load()?
            };
            cfg.server.grpc_addr = format!("0.0.0.0:{port}");

            let container = Arc::new(AppContainer::from_config(cfg.clone()).await?);

            let addr = cfg.server.grpc_addr.parse()?;
            tracing::info!("Starting imauth gRPC server on {}", addr);

            let auth_service = AuthServiceServer::new(AuthGrpcService::new(container.clone()));
            let session_service =
                SessionServiceServer::new(SessionGrpcService::new(container.clone()));
            let credential_service =
                CredentialServiceServer::new(CredentialGrpcService::new(container.clone()));

            let key = normalize_api_key(api_key).map(std::sync::Arc::new);
            Server::builder()
                .layer(tonic::service::interceptor(auth_interceptor(key)))
                .add_service(auth_service)
                .add_service(session_service)
                .add_service(credential_service)
                .serve(addr)
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::normalize_api_key;

    #[test]
    fn empty_key_is_treated_as_none() {
        assert!(normalize_api_key(Some(String::new())).is_none());
    }

    #[test]
    fn whitespace_only_key_is_treated_as_none() {
        assert!(normalize_api_key(Some("   \t\n".into())).is_none());
    }

    #[test]
    fn unset_key_is_none() {
        assert!(normalize_api_key(None).is_none());
    }

    #[test]
    fn real_key_is_preserved_after_trim() {
        assert_eq!(
            normalize_api_key(Some("  secret-key  ".into())).as_deref(),
            Some("secret-key")
        );
    }
}
