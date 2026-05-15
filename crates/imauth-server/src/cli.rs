use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "imauth-server")]
#[command(about = "imauth gRPC server")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Start the gRPC server
    Serve {
        /// Path to config file
        #[arg(short, long)]
        config: Option<std::path::PathBuf>,
        /// gRPC port
        #[arg(short, long, default_value = "50051")]
        port: u16,
        /// API key for incoming requests (also reads IMAUTH_API_KEY env var)
        #[arg(short = 'k', long, env = "IMAUTH_API_KEY")]
        api_key: Option<String>,
    },
}
