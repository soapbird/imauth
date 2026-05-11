mod cli;
mod generated;
mod grpc;

use crate::cli::{Cli, Commands};
use crate::grpc::{AuthGrpcService, CredentialGrpcService, SessionGrpcService};
use clap::Parser;
use generated::v1::{
    auth_service_server::AuthServiceServer,
    credential_service_server::CredentialServiceServer,
    session_service_server::SessionServiceServer,
};
use imauth_core::ImauthCore;
use std::sync::Arc;
use tonic::transport::Server;
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { config, port, nats } => {
            let mut cfg = if let Some(path) = config {
                imauth_core::Config::from_file(&path)?
            } else {
                imauth_core::Config::load()?
            };
            cfg.grpc_addr = format!("0.0.0.0:{port}");
            cfg.nats_url = nats;

            let core = Arc::new(ImauthCore::new(cfg.clone()).await?);

            let addr = cfg.grpc_addr.parse()?;
            tracing::info!("Starting imauth gRPC server on {}", addr);

            let auth_service = AuthGrpcService::new(core.clone());
            let session_service = SessionGrpcService::new(core.clone());
            let credential_service = CredentialGrpcService::new(core.clone());

            Server::builder()
                .add_service(AuthServiceServer::new(auth_service))
                .add_service(SessionServiceServer::new(session_service))
                .add_service(CredentialServiceServer::new(credential_service))
                .serve(addr)
                .await?;
        }
    }

    Ok(())
}
