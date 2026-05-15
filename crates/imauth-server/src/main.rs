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
use std::sync::Arc;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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

            let key = normalize_api_key(api_key).map(Arc::new);

            // Wait for SIGTERM/SIGINT and finish in-flight streams instead of
            // dropping the runtime under them. Prevents leaked browser-pool
            // permits when the orchestrator restarts the container.
            let shutdown = async {
                tokio::signal::ctrl_c()
                    .await
                    .expect("install ctrl_c handler");
                tracing::info!("shutdown signal received, draining streams");
            };

            Server::builder()
                .layer(tonic::service::interceptor(auth_interceptor(key)))
                .add_service(auth_service)
                .add_service(session_service)
                .add_service(credential_service)
                .serve_with_shutdown(addr, shutdown)
                .await?;
        }
    }

    Ok(())
}
