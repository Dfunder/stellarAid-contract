use anyhow::Result;
use std::env;

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
    println!("📋 Configuration Check");
    println!("━━━━━━━━━━━━━━━━━━━━━");

    let network = env::var("SOROBAN_NETWORK").unwrap_or_else(|_| "testnet".to_string());
    println!("Network: {}", network);

    match network.as_str() {
        "testnet" => {
            let rpc_url = env::var("SOROBAN_TESTNET_RPC_URL")
                .unwrap_or_else(|_| "https://soroban-testnet.stellar.org:443".to_string());
            let horizon_url = env::var("SOROBAN_TESTNET_HORIZON_URL")
                .unwrap_or_else(|_| "https://horizon-testnet.stellar.org".to_string());
            println!("RPC URL: {}", rpc_url);
            println!("Horizon URL: {}", horizon_url);
        }
        "mainnet" => {
            let rpc_url = env::var("SOROBAN_MAINNET_RPC_URL")
                .unwrap_or_else(|_| "https://soroban-rpc.mainnet.stellar.gateway.fm".to_string());
            let horizon_url = env::var("SOROBAN_MAINNET_HORIZON_URL")
                .unwrap_or_else(|_| "https://horizon.stellar.org".to_string());
            println!("RPC URL: {}", rpc_url);
            println!("Horizon URL: {}", horizon_url);
        }
        _ => println!("Unknown network: {}", network),
    }

    Ok(())
}

fn handle_network() -> Result<()> {
    let network = env::var("SOROBAN_NETWORK").unwrap_or_else(|_| "testnet".to_string());
    
    println!("🌐 Network Configuration");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Active Network: {}", network);

    match network.as_str() {
        "testnet" => {
            println!("RPC URL: https://soroban-testnet.stellar.org:443");
            println!("Horizon URL: https://horizon-testnet.stellar.org");
            println!("Passphrase: Test SDF Network ; September 2015");
        }
        "mainnet" => {
            println!("RPC URL: https://soroban-rpc.mainnet.stellar.gateway.fm");
            println!("Horizon URL: https://horizon.stellar.org");
            println!("Passphrase: Public Global Stellar Network ; September 2015");
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
