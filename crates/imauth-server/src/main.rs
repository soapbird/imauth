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
                imauth_core::Config::from_env()
            };
            cfg.server.grpc_addr = format!("0.0.0.0:{port}");

            let container = Arc::new(AppContainer::from_config(cfg.clone()).await?);

            let addr = cfg.server.grpc_addr.parse()?;
            tracing::info!("Starting imauth gRPC server on {}", addr);

            let key = normalize_api_key(api_key).map(Arc::new);

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

            // Metrics
            let metrics_port = cfg.metrics_port();
            let recorder = metrics_exporter_prometheus::PrometheusBuilder::new()
                .build_recorder();
            let metrics_handle = recorder.handle();
            metrics::set_global_recorder(recorder)
                .map_err(|e| tracing::warn!("Failed to set global metrics recorder: {e}"))
                .ok();
            tokio::spawn(async move {
                let listener = match tokio::net::TcpListener::bind(format!("0.0.0.0:{}", metrics_port)).await {
                    Ok(l) => l,
                    Err(e) => {
                        tracing::error!("Failed to bind metrics server: {e}");
                        return;
                    }
                };
                tracing::info!("Metrics server listening on 0.0.0.0:{}", metrics_port);
                loop {
                    let (mut socket, _) = match listener.accept().await {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::warn!("Metrics accept error: {e}");
                            continue;
                        }
                    };
                    let body = metrics_handle.render();
                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    if let Err(e) = tokio::io::AsyncWriteExt::write_all(&mut socket, response.as_bytes()).await {
                        tracing::warn!("Failed to write metrics response: {e}");
                    }
                }
            });

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
