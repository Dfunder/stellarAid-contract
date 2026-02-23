use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

mod config;
use config::{Config, Network};

const CONTRACT_ID_FILE: &str = ".stellaraid_contract_id";

#[derive(Parser)]
#[command(name = "stellaraid-cli")]
#[command(about = "StellarAid CLI tools for contract deployment and management")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Deploy the core.wasm contract to the specified network
    Deploy {
        /// Network to deploy to (testnet, mainnet, sandbox)
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Path to the WASM file (defaults to built contract)
        #[arg(short, long)]
        wasm: Option<String>,
        /// Skip initialization (for contracts that don't require init)
        #[arg(long, default_value = "false")]
        skip_init: bool,
    },
    /// Invoke a method on a deployed contract
    Invoke {
        /// Method to invoke
        #[arg(default_value = "ping")]
        method: String,
        /// Arguments to pass to the method (as JSON)
        #[arg(short, long)]
        args: Option<String>,
        /// Network to use (defaults to stored contract network)
        #[arg(short, long)]
        network: Option<String>,
    },
    /// Get the deployed contract ID
    ContractId {
        /// Show the contract ID for a specific network
        #[arg(short, long)]
        network: Option<String>,
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
            wasm,
            skip_init,
        } => {
            deploy_contract(&network, wasm.as_deref(), skip_init)?;
        }
        Commands::Invoke {
            method,
            args,
            network,
        } => {
            invoke_contract(&method, args.as_deref(), network.as_deref())?;
        }
        Commands::ContractId { network } => {
            show_contract_id(network.as_deref())?;
        }
        Commands::Config { action } => match action {
            ConfigAction::Check => {
                println!("Checking configuration...");
                match Config::load(None) {
                    Ok(cfg) => {
                        println!("✅ Configuration valid!");
                        println!("  Network: {}", cfg.network);
                        println!("  RPC URL: {}", cfg.rpc_url);
                        println!("  Admin Key: {}", cfg.admin_key.map_or("Not set".to_string(), |_| "Configured".to_string()));
                    }
                    Err(e) => {
                        eprintln!("❌ Configuration error: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            ConfigAction::Init => {
                println!("Initializing configuration...");
                initialize_config()?;
            }
        },
        Commands::Network => match Config::load(None) {
            Ok(cfg) => {
                println!("Active network: {}", cfg.network);
                println!("RPC URL: {}", cfg.rpc_url);
                println!("Passphrase: {}", cfg.network_passphrase);
                if let Some(key) = cfg.admin_key {
                    println!("Admin Key: {}", key);
                }
            }
            Err(e) => {
                eprintln!("Failed to load config: {}", e);
                std::process::exit(2);
            }
        },
    }

    Ok(())
}

/// Get the path to the WASM file
fn get_wasm_path(custom_path: Option<&str>) -> Result<PathBuf> {
    if let Some(path) = custom_path {
        let p = PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
        anyhow::bail!("WASM file not found: {}", path);
    }

    // Try default paths
    let default_paths = vec![
        PathBuf::from("target/wasm32-unknown-unknown/debug/stellaraid_core.wasm"),
        PathBuf::from("target/wasm32-unknown-unknown/release/stellaraid_core.wasm"),
        PathBuf::from("contracts/core/target/wasm32-unknown-unknown/debug/stellaraid_core.wasm"),
        PathBuf::from("crates/contracts/core/target/wasm32-unknown-unknown/debug/stellaraid_core.wasm"),
    ];

    for p in &default_paths {
        if p.exists() {
            return Ok(p.clone());
        }
    }

    // Check if we're in the workspace root
    let cwd = env::current_dir()?;
    let wasm_path = cwd.join("target/wasm32-unknown-unknown/debug/stellaraid_core.wasm");
    if wasm_path.exists() {
        return Ok(wasm_path);
    }

    anyhow::bail!(
        "WASM file not found. Build with 'make wasm' or specify with --wasm flag"
    )
}

/// Store the contract ID in a local file
fn store_contract_id(contract_id: &str, network: &str) -> Result<()> {
    let cwd = env::current_dir()?;
    let file_path = cwd.join(CONTRACT_ID_FILE);
    
    let content = if file_path.exists() {
        let existing: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file_path)?)
            .unwrap_or(serde_json::json!({}));
        let mut map = serde_json::Map::new();
        if let Some(obj) = existing.as_object() {
            for (k, v) in obj {
                map.insert(k.clone(), v.clone());
            }
        }
        map.insert(network.to_string(), serde_json::json!(contract_id));
        serde_json::Value::Object(map)
    } else {
        serde_json::json!({ network: contract_id })
    };

    fs::write(&file_path, serde_json::to_string_pretty(&content)?)?;
    println!("✅ Contract ID stored in {}", CONTRACT_ID_FILE);
    Ok(())
}

/// Load the contract ID from local file
fn load_contract_id(network: &str) -> Result<String> {
    let cwd = env::current_dir()?;
    let file_path = cwd.join(CONTRACT_ID_FILE);
    
    if !file_path.exists() {
        anyhow::bail!(
            "No contract ID found. Deploy a contract first with 'deploy' command"
        );
    }

    let content: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file_path)?)?;
    
    if let Some(id) = content.get(network).and_then(|v| v.as_str()) {
        Ok(id.to_string())
    } else {
        anyhow::bail!(
            "No contract ID found for network '{}'. Available: {}",
            network,
            content.keys().collect::<Vec<_>>().join(", ")
        );
    }
}

/// Deploy the contract to the specified network
fn deploy_contract(network: &str, wasm_path: Option<&str>, skip_init: bool) -> Result<()> {
    println!("🚀 Deploying to network: {}", network);
    
    // Load configuration
    env::set_var("SOROBAN_NETWORK", network);
    let config = Config::load(None).context("Failed to load configuration")?;
    
    // Get WASM path
    let wasm = get_wasm_path(wasm_path)?;
    println!("📦 Using WASM: {}", wasm.display());
    
    // Build soroban deploy command
    let output = Command::new("soroban")
        .args([
            "contract",
            "deploy",
            "--wasm", wasm.to_str().unwrap(),
            "--network", network,
            "--rpc-url", &config.rpc_url,
            "--network-passphrase", &config.network_passphrase,
        ])
        .output()
        .context("Failed to execute soroban CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("❌ Deployment failed: {}", stderr);
        std::process::exit(1);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let contract_id = stdout.trim();
    
    println!("✅ Contract deployed successfully!");
    println!("📝 Contract ID: {}", contract_id);
    
    // Store contract ID
    store_contract_id(contract_id, network)?;
    
    // Initialize the contract if needed
    if !skip_init {
        if let Some(admin_key) = &config.admin_key {
            println!("🔧 Initializing contract with admin: {}", admin_key);
            let init_output = Command::new("soroban")
                .args([
                    "contract",
                    "invoke",
                    "--network", network,
                    "--rpc-url", &config.rpc_url,
                    "--network-passphrase", &config.network_passphrase,
                    contract_id,
                    "--",
                    "init",
                    "--admin", admin_key,
                ])
                .output()
                .context("Failed to initialize contract")?;

            if init_output.status.success() {
                println!("✅ Contract initialized!");
            } else {
                let stderr = String::from_utf8_lossy(&init_output.stderr);
                eprintln!("⚠️  Initialization warning: {}", stderr);
            }
        } else {
            println!("ℹ️  No admin key configured. Skipping initialization.");
            println!("   Set SOROBAN_ADMIN_KEY environment variable to initialize the contract.");
        }
    }
    
    Ok(())
}

/// Invoke a method on a deployed contract
fn invoke_contract(method: &str, args: Option<&str>, network_override: Option<&str>) -> Result<()> {
    // Determine which network to use
    let network = if let Some(n) = network_override {
        n.to_string()
    } else {
        // Try to load from stored contract ID
        if let Ok(cfg) = Config::load(None) {
            match cfg.network {
                Network::Testnet => "testnet".to_string(),
                Network::Mainnet => "mainnet".to_string(),
                Network::Sandbox => "sandbox".to_string(),
                Network::Custom(n) => n,
            }
        } else {
            "testnet".to_string()
        }
    };
    
    println!("🔄 Invoking method '{}' on network: {}", method, network);
    
    // Load configuration
    env::set_var("SOROBAN_NETWORK", &network);
    let config = Config::load(None).context("Failed to load configuration")?;
    
    // Load contract ID
    let contract_id = load_contract_id(&network)?;
    println!("📝 Using contract ID: {}", contract_id);
    
    // Build invoke command
    let mut cmd_args = vec![
        "contract",
        "invoke",
        "--network", &network,
        "--rpc-url", &config.rpc_url,
        "--network-passphrase", &config.network_passphrase,
        &contract_id,
        "--",
        method,
    ];
    
    // Add arguments if provided
    if let Some(arguments) = args {
        // Parse JSON arguments and add them
        let parsed: serde_json::Value = serde_json::from_str(arguments)
            .context("Failed to parse arguments as JSON")?;
        
        if let Some(arr) = parsed.as_array() {
            for val in arr {
                cmd_args.push(&val.to_string());
            }
        }
    }
    
    let output = Command::new("soroban")
        .args(&cmd_args)
        .output()
        .context("Failed to execute soroban CLI")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("❌ Invocation failed: {}", stderr);
        std::process::exit(1);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    println!("✅ Invocation successful!");
    println!("📤 Result: {}", stdout.trim());
    
    Ok(())
}

/// Show the contract ID for a network
fn show_contract_id(network_override: Option<&str>) -> Result<()> {
    if let Some(network) = network_override {
        let contract_id = load_contract_id(network)?;
        println!("Contract ID for {}: {}", network, contract_id);
    } else {
        // Show all stored contract IDs
        let cwd = env::current_dir()?;
        let file_path = cwd.join(CONTRACT_ID_FILE);
        
        if !file_path.exists() {
            println!("No contract IDs stored. Deploy a contract first.");
            return Ok(());
        }
        
        let content: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file_path)?)?;
        
        println!("Stored contract IDs:");
        if let Some(obj) = content.as_object() {
            for (network, id) in obj {
                println!("  {}: {}", network, id);
            }
        }
    }
    Ok(())
}

/// Initialize configuration files
fn initialize_config() -> Result<()> {
    let cwd = env::current_dir()?;
    
    // Check if .env already exists
    let env_path = cwd.join(".env");
    if env_path.exists() {
        println!("⚠️  .env file already exists");
        return Ok(());
    }
    
    // Create .env file with example values
    let env_content = r#"# StellarAid Configuration
# Network: testnet, mainnet, or sandbox
SOROBAN_NETWORK=testnet

# RPC URL (optional - will use soroban.toml if not set)
# SOROBAN_RPC_URL=https://soroban-testnet.stellar.org

# Network passphrase (optional - will use soroban.toml if not set)
# SOROBAN_NETWORK_PASSPHRASE=Test SDF Network ; September 2015

# Admin key for contract deployment (optional)
# Use 'soroban keys generate' to create a new key
# SOROBAN_ADMIN_KEY=
"#;
    
    fs::write(&env_path, env_content)?;
    println!("✅ Created .env file");
    println!("ℹ️  Edit .env to configure your network and admin key");
    
    // Check if contract ID file exists
    let contract_path = cwd.join(CONTRACT_ID_FILE);
    if !contract_path.exists() {
        let empty: serde_json::Value = serde_json::json!({});
        fs::write(&contract_path, serde_json::to_string_pretty(&empty)?)?;
        println!("✅ Created {} file", CONTRACT_ID_FILE);
    }
    
    Ok(())
}
