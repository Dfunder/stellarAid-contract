use anyhow::Result;
use clap::{Parser, Subcommand};

mod config;
use config::Config;

#[derive(Parser)]
#[command(name = "stellaraid-cli")]
#[command(about = "StellarAid CLI tools for contract deployment and management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy a contract (placeholder)
    Deploy {
        #[arg(short, long)]
        network: String,
        #[arg(short, long)]
        contract_id: Option<String>,
    },
    /// Configuration utilities
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Print resolved network configuration
    Network,
}

#[derive(Subcommand)]
enum ConfigAction {
    Check,
    Init,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Deploy {
            network,
            contract_id,
        } => {
            println!("Deploying to network: {}", network);
            if let Some(id) = contract_id {
                println!("Using contract ID: {}", id);
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Check => {
                println!("Checking configuration...");
            }
            ConfigAction::Init => {
                println!("Initializing configuration...");
            }
        },
        Commands::Network => {
            match Config::load(None) {
                Ok(cfg) => {
                    println!("Active network: {}", cfg.network);
                    println!("RPC URL: {}", cfg.rpc_url);
                    println!("Passphrase: {}", cfg.network_passphrase);
                }
                Err(e) => {
                    eprintln!("Failed to load config: {}", e);
                    std::process::exit(2);
                }
            }
        }
    }

    Ok(())
}
