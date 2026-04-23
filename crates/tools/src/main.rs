use anyhow::Result;
use std::env;

mod environment_config;
use environment_config::{EnvironmentConfig, check_testnet_connection};

fn main() -> Result<()> {
    dotenv::dotenv().ok();

    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("StellarAid CLI - Soroban Contract Management Tool");
        println!("Usage: stellaraid-cli <command>");
        println!();
        println!("Commands:");
        println!("  config     - Manage configuration");
        println!("  network    - Show network configuration");
        println!("  deploy     - Deploy contract");
        println!("  invoke     - Invoke contract method");
        println!("  account    - Manage Stellar accounts");
        return Ok(());
    }

    match args[1].as_str() {
        "config" => handle_config(),
        "network" => handle_network(),
        "deploy" => handle_deploy(),
        "invoke" => handle_invoke(&args[2..]),
        "account" => handle_account(),
        _ => {
            println!("Unknown command: {}", args[1]);
            Ok(())
        }
    }
}

fn handle_config() -> Result<()> {
    let config = EnvironmentConfig::from_env()?;
    
    println!("📋 Configuration Check");
    println!("━━━━━━━━━━━━━━━━━━━━━");
    println!("Active Network: {}", config.network);

    match config.network.as_str() {
        "testnet" => {
            println!("RPC URL: {}", config.testnet.rpc_url);
            println!("Horizon URL: {}", config.testnet.horizon_url);
            println!("Passphrase: {}", config.testnet.network_passphrase);
        }
        "mainnet" => {
            println!("RPC URL: {}", config.mainnet.rpc_url);
            println!("Horizon URL: {}", config.mainnet.horizon_url);
            println!("Passphrase: {}", config.mainnet.network_passphrase);
        }
        _ => println!("Unknown network: {}", config.network),
    }

    if let Some(admin_key) = config.admin_public_key {
        println!("Admin Public Key: {}", admin_key);
    } else {
        println!("⚠️  Admin public key not set");
    }

    // Validate configuration
    if let Err(e) = config.validate() {
        println!("❌ Configuration validation failed: {}", e);
    } else {
        println!("✅ Configuration is valid");
    }

    Ok(())
}

fn handle_network() -> Result<()> {
    let config = EnvironmentConfig::from_env()?;
    
    println!("🌐 Network Configuration");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Active Network: {}", config.network);

    match config.network.as_str() {
        "testnet" => {
            println!("RPC URL: {}", config.testnet.rpc_url);
            println!("Horizon URL: {}", config.testnet.horizon_url);
            println!("Passphrase: {}", config.testnet.network_passphrase);
        }
        "mainnet" => {
            println!("RPC URL: {}", config.mainnet.rpc_url);
            println!("Horizon URL: {}", config.mainnet.horizon_url);
            println!("Passphrase: {}", config.mainnet.network_passphrase);
        }
        _ => println!("Unknown network configuration"),
    }

    Ok(())
}

fn handle_deploy() -> Result<()> {
    println!("🚀 Deploy contract functionality coming soon...");
    Ok(())
}

fn handle_invoke(args: &[String]) -> Result<()> {
    if args.is_empty() {
        println!("Usage: stellaraid-cli invoke <method>");
        return Ok(());
    }

    println!("🔄 Invoke method '{}' functionality coming soon...", args[0]);
    Ok(())
}

fn handle_account() -> Result<()> {
    println!("👤 Account management functionality coming soon...");
    Ok(())
}
