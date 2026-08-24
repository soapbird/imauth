#![allow(clippy::result_large_err)]

use clap::Parser;
use imauth_core::AppContainer;
use imauth_proto::generated::v1::{
    auth_service_server::AuthServiceServer, credential_service_server::CredentialServiceServer,
    session_service_server::SessionServiceServer,
};
use imauth_server::auth::{auth_interceptor, normalize_api_key};
use imauth_server::cli::{Cli, Commands};
use imauth_server::grpc::{AuthGrpcService, CredentialGrpcService, SessionGrpcService};
use std::net::{AddrParseError, SocketAddr};
use std::path::Path;
use std::sync::Arc;
use tonic::transport::{Identity, Server, ServerTlsConfig};

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

fn resolve_bind_addr(configured: &str, port: Option<u16>) -> Result<SocketAddr, AddrParseError> {
    let mut addr = configured.parse::<SocketAddr>()?;
    if let Some(port) = port {
        addr.set_port(port);
    }
    Ok(addr)
}

fn read_tls_file(path: &Path, kind: &str) -> std::io::Result<Vec<u8>> {
    std::fs::read(path).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("Failed to read TLS {kind} {}: {error}", path.display()),
        )
    })
}

fn load_server_identity(
    cfg: &imauth_core::Config,
) -> Result<Option<Identity>, Box<dyn std::error::Error>> {
    let Some((cert_path, key_path)) = cfg.tls_identity_paths()? else {
        return Ok(None);
    };
    let cert = read_tls_file(cert_path, "certificate")?;
    let key = read_tls_file(key_path, "private key")?;
    Ok(Some(Identity::from_pem(cert, key)))
}

#[cfg(unix)]
async fn shutdown_signal() -> std::io::Result<()> {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select! {
        result = tokio::signal::ctrl_c() => result,
        _ = terminate.recv() => Ok(()),
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() -> std::io::Result<()> {
    tokio::signal::ctrl_c().await
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_crypto_provider()?;
    // Load .env before tracing init so IMAUTH_API_KEY / IMAUTH_ENCRYPTION_KEY
    // set there reach Cli::parse and the encryption-key check. Matches the CLI
    // (imauth-cli/src/main.rs) so operators see identical .env behavior on both
    // binaries. Quiet failure when no .env is present is intentional.
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve {
            config,
            port,
            api_key,
        } => {
            let cfg = if let Some(path) = config {
                imauth_core::Config::from_file(&path)?
            } else {
                imauth_core::Config::from_env()
            };

            let addr = resolve_bind_addr(cfg.grpc_addr(), port)?;
            let identity = load_server_identity(&cfg)?;

            let container = Arc::new(AppContainer::from_config(cfg.clone()).await?);

            tracing::info!("Starting imauth gRPC server on {}", addr);

            let key = normalize_api_key(api_key).map(Arc::new);
            if key.is_none() {
                tracing::warn!(bind_addr = %addr, "API key authentication disabled");
            }

            // Apply auth interceptor to business services only; health check
            // must remain unauthenticated so Kubernetes probes work.
            let auth_service = AuthServiceServer::with_interceptor(
                AuthGrpcService::new(container.clone()),
                auth_interceptor(key.clone()),
            );
            let session_service = SessionServiceServer::with_interceptor(
                SessionGrpcService::new(container.clone()),
                auth_interceptor(key.clone()),
            );
            let credential_service = CredentialServiceServer::with_interceptor(
                CredentialGrpcService::new(container.clone()),
                auth_interceptor(key.clone()),
            );

            // Health check
            let (mut health_reporter, health_service) = tonic_health::server::health_reporter();
            health_reporter
                .set_serving::<AuthServiceServer<AuthGrpcService>>()
                .await;
            health_reporter
                .set_serving::<SessionServiceServer<SessionGrpcService>>()
                .await;
            health_reporter
                .set_serving::<CredentialServiceServer<CredentialGrpcService>>()
                .await;

            // Wait for SIGTERM/SIGINT and finish in-flight streams instead of
            // dropping the runtime under them. Prevents leaked browser-pool
            // permits when the orchestrator restarts the container.
            let shutdown = async {
                match shutdown_signal().await {
                    Ok(()) => tracing::info!("shutdown signal received, draining streams"),
                    Err(error) => tracing::error!(%error, "shutdown signal handler failed"),
                }
            };

            let mut server = Server::builder();
            if let Some(identity) = identity {
                server = server.tls_config(ServerTlsConfig::new().identity(identity))?;
                tracing::info!("gRPC TLS enabled");
            }

            server
                .add_service(auth_service)
                .add_service(session_service)
                .add_service(credential_service)
                .add_service(health_service)
                .serve_with_shutdown(addr, shutdown)
                .await?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{install_crypto_provider, resolve_bind_addr};

    #[test]
    fn install_crypto_provider_sets_process_default() {
        // Given: the server binary is built with transitive rustls providers.
        // When: server startup selects its provider.
        install_crypto_provider().unwrap();

        // Then: TLS construction has an unambiguous process default.
        assert!(rustls::crypto::CryptoProvider::get_default().is_some());
    }

    #[test]
    fn resolve_bind_addr_preserves_configured_address_without_override() {
        // Given: a configured loopback bind address.
        let configured = "127.0.0.1:6100";

        // When: no CLI port override is supplied.
        let resolved = resolve_bind_addr(configured, None).unwrap();

        // Then: both host and port are preserved.
        assert_eq!(resolved.to_string(), configured);
    }

    #[test]
    fn resolve_bind_addr_overrides_only_the_port() {
        // Given: a configured loopback bind address.
        let configured = "127.0.0.1:6100";

        // When: a CLI port override is supplied.
        let resolved = resolve_bind_addr(configured, Some(7443)).unwrap();

        // Then: the configured host is retained and only the port changes.
        assert_eq!(resolved.to_string(), "127.0.0.1:7443");
    }
}
