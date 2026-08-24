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
        #[arg(short, long)]
        port: Option<u16>,
        /// API key for incoming requests (also reads IMAUTH_API_KEY env var)
        #[arg(short = 'k', long, env = "IMAUTH_API_KEY")]
        api_key: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands};
    use clap::Parser;

    #[test]
    fn serve_port_is_none_when_flag_is_omitted() {
        // Given: a serve invocation without a port override.
        let cli = Cli::try_parse_from(["imauth-server", "serve"]).unwrap();

        // When: the serve arguments are inspected.
        let Commands::Serve { port, .. } = cli.command;

        // Then: config retains ownership of the bind port.
        assert_eq!(port, None);
    }

    #[test]
    fn serve_port_contains_explicit_override() {
        // Given: a serve invocation with a port override.
        let cli = Cli::try_parse_from(["imauth-server", "serve", "--port", "7443"]).unwrap();

        // When: the serve arguments are inspected.
        let Commands::Serve { port, .. } = cli.command;

        // Then: the explicit override is retained.
        assert_eq!(port, Some(7443));
    }
}
