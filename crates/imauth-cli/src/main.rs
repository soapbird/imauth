mod cli_support;
#[cfg(test)]
mod cli_support_tests;
mod grpc_commands;
mod provider_record;
#[cfg(test)]
mod provider_record_tests;

use crate::cli_support::{install_crypto_provider, Cli, Commands};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    install_crypto_provider()?;
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    match cli.command {
        Commands::Provider { action } => provider_record::run(action),
        command => {
            grpc_commands::run(command, cli.server, cli.api_key, cli.tls_ca, cli.tls_domain).await
        }
    }
}
