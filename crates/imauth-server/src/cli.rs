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
    },
}
