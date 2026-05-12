

use imauth_server::cli::{Cli, Commands};
use imauth_server::grpc::{AuthGrpcService, CredentialGrpcService, SessionGrpcService};
use clap::Parser;
use imauth_proto::generated::v1::{
    auth_service_server::AuthServiceServer, credential_service_server::CredentialServiceServer,
    session_service_server::SessionServiceServer,
};
use imauth_core::AppContainer;
use std::sync::Arc;
use tonic::transport::Server;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Serve { config, port } => {
            let mut cfg = if let Some(path) = config {
                imauth_core::Config::from_file(&path)?
            } else {
                imauth_core::Config::load()?
            };
            cfg.server.grpc_addr = format!("0.0.0.0:{port}");

            let container = Arc::new(AppContainer::from_config(cfg.clone()).await?);

            let addr = cfg.server.grpc_addr.parse()?;
            tracing::info!("Starting imauth gRPC server on {}", addr);

            let auth_service = AuthGrpcService::new(container.clone());
            let session_service = SessionGrpcService::new(container.clone());
            let credential_service = CredentialGrpcService::new(container.clone());

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
