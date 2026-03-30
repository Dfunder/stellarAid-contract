use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

mod account_management;
mod batch_operations;
mod config;
mod contract_interaction;
mod debug_utils;
mod donation_tx_builder;
mod environment_config;
mod fee;
mod horizon_client;
mod horizon_error;
mod horizon_rate_limit;
mod horizon_retry;
mod secure_vault;
mod soroban_tx_builder;
mod transaction_history;
mod transaction_submission;
mod transaction_verification;
mod validation;
mod wallet_signing;

use account_management::{AccountManagementRequest, AccountAction, AccountManagementService};
use batch_operations::{BatchOperationService, BatchRequest, BatchOperation, BatchOperationType};
use config::{Config, Network};
use contract_interaction::{ContractInteractionService, ContractQueryRequest, ExportFormat};
use debug_utils::DebugService;
use donation_tx_builder::{build_donation_transaction, BuildDonationTxRequest};
use soroban_tx_builder::{
    build_soroban_invoke_transaction, json_to_sc_vals, BuildSorobanInvokeRequest,
};
use transaction_history::{TransactionHistoryService, TransactionHistoryRequest, Order, TransactionType};
use validation::{InputValidator, ErrorHandler};
use horizon_client::health::{HealthStatus, HorizonHealthChecker};
use horizon_client::{HorizonClient, HorizonClientConfig};
use transaction_submission::{
    SubmissionConfig, SubmissionLogger, SubmissionRequest, SubmissionResponse,
    TransactionSubmissionService,
};
use transaction_verification::{TransactionVerificationService, VerificationRequest};
use wallet_signing::{
    CompleteSigningRequest, PrepareSigningRequest, SigningStatus, WalletSigningService, WalletType,
};

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
    /// Build a donation payment transaction XDR for client-side signing
    BuildDonationTx {
        /// Donor public key (source account)
        #[arg(long)]
        donor: String,
        /// Current donor account sequence number
        #[arg(long)]
        donor_sequence: String,
        /// Donation amount (up to 7 decimals, e.g. 10.5)
        #[arg(long)]
        amount: String,
        /// Asset code (XLM for native, or token code like USDC)
        #[arg(long, default_value = "XLM")]
        asset: String,
        /// Asset issuer public key (required for non-XLM assets)
        #[arg(long)]
        issuer: Option<String>,
        /// Project ID used in memo as project_<id>
        #[arg(long)]
        project_id: String,
        /// Destination platform public key (overrides env var)
        #[arg(long)]
        destination: Option<String>,
        /// Transaction timeout in seconds
        #[arg(long, default_value_t = 300)]
        timeout_seconds: i64,
        /// Base fee in stroops per operation
        #[arg(long, default_value_t = 100)]
        base_fee: u32,
        /// Explicit network passphrase (defaults to config value)
        #[arg(long)]
        network_passphrase: Option<String>,
    },
    /// Build an unsigned Soroban contract-invoke transaction (base64 XDR) for signing
    BuildInvokeTx {
        /// Source account public key (G…)
        #[arg(long)]
        source: String,
        /// Current account sequence (sequence number to use for this tx)
        #[arg(long)]
        sequence: String,
        /// Contract id (C…)
        #[arg(long)]
        contract: String,
        /// Contract function / method name (Soroban symbol)
        #[arg(long)]
        function: String,
        /// JSON array of arguments (see soroban_tx_builder::json_to_sc_vals)
        #[arg(long)]
        args: Option<String>,
        /// Transaction validity window (seconds from now); 0 = no time bound
        #[arg(long, default_value_t = 300)]
        timeout_seconds: i64,
        /// Base fee in stroops per operation
        #[arg(long, default_value_t = 100)]
        base_fee: u32,
        /// Network passphrase override (defaults to config)
        #[arg(long)]
        network_passphrase: Option<String>,
        /// SorobanTransactionData XDR (base64) from RPC simulation — recommended for submission
        #[arg(long)]
        soroban_data_xdr: Option<String>,
    },
    /// Prepare a wallet-specific transaction signing request
    PrepareWalletSigning {
        /// Wallet name: freighter, albedo, lobstr
        #[arg(long)]
        wallet: String,
        /// Unsigned transaction envelope XDR (base64)
        #[arg(long)]
        xdr: String,
        /// Network passphrase override
        #[arg(long)]
        network_passphrase: Option<String>,
        /// Optional signer public key/address
        #[arg(long)]
        public_key: Option<String>,
        /// Callback URL for popup/deep-link wallets
        #[arg(long)]
        callback_url: Option<String>,
        /// Signing timeout in seconds
        #[arg(long, default_value_t = 180)]
        timeout_seconds: u64,
        /// Log file path for signing attempts/results
        #[arg(long, default_value = ".wallet_signing_attempts.jsonl")]
        log_file: String,
    },
    /// Complete a wallet signing attempt with callback/response payload
    CompleteWalletSigning {
        /// Wallet name: freighter, albedo, lobstr
        #[arg(long)]
        wallet: String,
        /// Attempt ID returned from prepare-wallet-signing
        #[arg(long)]
        attempt_id: String,
        /// Raw wallet response payload (JSON, callback URL, query string, or signed XDR)
        #[arg(long)]
        response: String,
        /// Attempt start UNIX timestamp in seconds
        #[arg(long)]
        started_at_unix: u64,
        /// Signing timeout in seconds
        #[arg(long, default_value_t = 180)]
        timeout_seconds: u64,
        /// Log file path for signing attempts/results
        #[arg(long, default_value = ".wallet_signing_attempts.jsonl")]
        log_file: String,
    },
    /// Submit a signed transaction to the Stellar network
    SubmitTx {
        /// Signed transaction envelope XDR (base64)
        #[arg(long)]
        xdr: String,
        /// Network to submit to (testnet, mainnet)
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Maximum submission timeout in seconds
        #[arg(long, default_value_t = 60)]
        timeout_seconds: u64,
        /// Maximum retry attempts for transient failures
        #[arg(long, default_value_t = 3)]
        max_retries: u32,
        /// Disable retry logic
        #[arg(long, default_value_t = false)]
        no_retry: bool,
        /// Log file path for submission attempts
        #[arg(long, default_value = ".transaction_submissions.jsonl")]
        log_file: String,
    },
    /// Check transaction submission status and statistics
    SubmissionStatus {
        /// Show detailed recent submissions
        #[arg(long, default_value_t = false)]
        detailed: bool,
        /// Filter by transaction hash
        #[arg(long)]
        tx_hash: Option<String>,
        /// Log file path
        #[arg(long, default_value = ".transaction_submissions.jsonl")]
        log_file: String,
    },
    /// Verify a transaction on-chain via Horizon
    VerifyTx {
        /// 64-character hex transaction hash to verify
        #[arg(long)]
        hash: String,
        /// Network to query (testnet, mainnet)
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Verification timeout in seconds
        #[arg(long, default_value_t = 30)]
        timeout_seconds: u64,
    },
    /// Get transaction history for an account
    TxHistory {
        /// Account ID to query
        #[arg(long)]
        account: String,
        /// Limit number of transactions (max 200)
        #[arg(long, default_value_t = 50)]
        limit: u32,
        /// Transaction type filter
        #[arg(long)]
        tx_type: Option<String>,
        /// Order: asc or desc
        #[arg(long, default_value = "desc")]
        order: String,
        /// Export to CSV file
        #[arg(long)]
        export_csv: Option<String>,
        /// Show summary statistics
        #[arg(long)]
        summary: bool,
        /// Network to query
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Execute batch operations
    Batch {
        #[command(subcommand)]
        action: BatchAction,
    },
    /// Advanced debugging utilities
    Debug {
        #[command(subcommand)]
        action: DebugAction,
    },
    /// Contract interaction utilities
    Contract {
        #[command(subcommand)]
        action: ContractAction,
    },
    /// Account management utilities
    Account {
        #[command(subcommand)]
        action: AccountAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    Check,
    Init,
}

#[derive(Subcommand)]
enum BatchAction {
    /// Execute batch operations from CSV file
    Execute {
        /// CSV file path
        #[arg(long)]
        file: String,
        /// Execute in parallel
        #[arg(long)]
        parallel: bool,
        /// Continue on error
        #[arg(long)]
        continue_on_error: bool,
        /// Max concurrent operations
        #[arg(long)]
        max_concurrent: Option<usize>,
        /// Export results to file
        #[arg(long)]
        export_results: Option<String>,
    },
    /// Create batch template
    CreateTemplate {
        /// Output file path
        #[arg(long)]
        output: String,
        /// Batch operation type
        #[arg(long)]
        operation_type: String,
    },
}

#[derive(Subcommand)]
enum DebugAction {
    /// Collect comprehensive debug information
    Collect {
        /// Account ID to debug
        #[arg(long)]
        account: String,
        /// Contract ID to debug
        #[arg(long)]
        contract: Option<String>,
        /// Export debug report to file
        #[arg(long)]
        export: Option<String>,
        /// Network to debug
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Analyze transaction failure
    AnalyzeFailure {
        /// Transaction hash to analyze
        #[arg(long)]
        tx_hash: String,
        /// Account ID for context
        #[arg(long)]
        account: String,
        /// Network to query
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Check network status and performance
    NetworkStatus {
        /// Network to check
        #[arg(short, long, default_value = "testnet")]
        network: String,
        /// Detailed performance metrics
        #[arg(long)]
        detailed: bool,
    },
}

#[derive(Subcommand)]
enum ContractAction {
    /// Get contract methods and information
    Info {
        /// Contract ID
        #[arg(long)]
        contract: String,
        /// Export format (json, markdown)
        #[arg(long, default_value = "json")]
        format: String,
        /// Output file
        #[arg(long)]
        output: Option<String>,
    },
    /// Query contract method
    Query {
        /// Contract ID
        #[arg(long)]
        contract: String,
        /// Method name
        #[arg(long)]
        method: String,
        /// Method arguments (JSON array)
        #[arg(long)]
        args: Option<String>,
        /// Simulate only (don't submit)
        #[arg(long)]
        simulate: bool,
        /// Network to query
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Get contract state
    State {
        /// Contract ID
        #[arg(long)]
        contract: String,
        /// Export state to file
        #[arg(long)]
        export: Option<String>,
        /// Network to query
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Generate method call template
    Template {
        /// Contract ID
        #[arg(long)]
        contract: String,
        /// Method name
        #[arg(long)]
        method: String,
        /// Output file
        #[arg(long)]
        output: Option<String>,
    },
}

#[derive(Subcommand)]
enum AccountAction {
    /// Create new account
    Create {
        /// Save to secure vault
        #[arg(long)]
        save: bool,
        /// Password for secure vault
        #[arg(long)]
        password: Option<String>,
        /// Generate mnemonic
        #[arg(long)]
        generate_mnemonic: bool,
    },
    /// Import existing account
    Import {
        /// Import from private key
        #[arg(long)]
        private_key: Option<String>,
        /// Import from mnemonic
        #[arg(long)]
        mnemonic: Option<String>,
        /// Save to secure vault
        #[arg(long)]
        save: bool,
        /// Password for secure vault
        #[arg(long)]
        password: Option<String>,
    },
    /// Export account information
    Export {
        /// Account ID
        #[arg(long)]
        account: String,
        /// Password for secure vault
        #[arg(long)]
        password: String,
        /// Export format (json)
        #[arg(long, default_value = "json")]
        format: String,
    },
    /// List all accounts
    List {
        /// Show detailed information
        #[arg(long)]
        detailed: bool,
    },
    /// Get account balance
    Balance {
        /// Account ID
        #[arg(long)]
        account: String,
        /// Network to query
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Get account signers
    Signers {
        /// Account ID
        #[arg(long)]
        account: String,
        /// Network to query
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Fund testnet account
    Fund {
        /// Account ID to fund
        #[arg(long)]
        account: String,
        /// Network (only testnet supported)
        #[arg(short, long, default_value = "testnet")]
        network: String,
    },
    /// Connect wallet
    ConnectWallet {
        /// Wallet type (freighter, albedo, lobstr, ledger, trezor)
        #[arg(long)]
        wallet_type: String,
    },
    /// Validate address
    ValidateAddress {
        /// Address to validate
        #[arg(long)]
        address: String,
    },
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
        },
        Commands::Invoke {
            method,
            args,
            network,
        } => {
            invoke_contract(&method, args.as_deref(), network.as_deref())?;
        },
        Commands::ContractId { network } => {
            show_contract_id(network.as_deref())?;
        },
        Commands::Config { action } => match action {
            ConfigAction::Check => {
                println!("Checking configuration...");
                match Config::load(None) {
                    Ok(cfg) => {
                        println!("✅ Configuration valid!");
                        println!("  Network: {}", cfg.network);
                        println!("  RPC URL: {}", cfg.rpc_url);
                        println!("  Horizon URL: {}", cfg.horizon_url);
                        println!(
                            "  Admin Key: {}",
                            cfg.admin_key
                                .map_or("Not set".to_string(), |_| "Configured".to_string())
                        );
                        println!("Testing Horizon connectivity...");
                        let hcfg = HorizonClientConfig {
                            server_url: cfg.horizon_url.clone(),
                            ..HorizonClientConfig::default()
                        };
                        let client = HorizonClient::with_config(hcfg)
                            .map_err(|e| anyhow::anyhow!("Horizon client: {}", e))?;
                        let rt = tokio::runtime::Runtime::new()?;
                        let checker = HorizonHealthChecker::default_config();
                        let result = rt.block_on(checker.check(&client));
                        match result {
                            Ok(r) if r.status == HealthStatus::Healthy || r.status == HealthStatus::Degraded => {
                                println!(
                                    "✅ Horizon reachable ({}, {} ms)",
                                    r.status, r.response_time_ms
                                );
                            },
                            Ok(r) => {
                                eprintln!("❌ Horizon status: {}", r.status);
                                if let Some(err) = r.error {
                                    eprintln!("   {}", err);
                                }
                                std::process::exit(1);
                            },
                            Err(e) => {
                                eprintln!("❌ Horizon check failed: {}", e);
                                std::process::exit(1);
                            },
                        }
                    },
                    Err(e) => {
                        eprintln!("❌ Configuration error: {}", e);
                        std::process::exit(1);
                    },
                }
            },
            ConfigAction::Init => {
                println!("Initializing configuration...");
                initialize_config()?;
            },
        },
        Commands::Network => match Config::load(None) {
            Ok(cfg) => {
                println!("Active network: {}", cfg.network);
                println!("RPC URL: {}", cfg.rpc_url);
                println!("Horizon URL: {}", cfg.horizon_url);
                println!("Passphrase: {}", cfg.network_passphrase);
                if let Some(key) = cfg.admin_key {
                    println!("Admin Key: {}", key);
                }
            },
            Err(e) => {
                eprintln!("Failed to load config: {}", e);
                std::process::exit(2);
            },
        },
        Commands::BuildDonationTx {
            donor,
            donor_sequence,
            amount,
            asset,
            issuer,
            project_id,
            destination,
            timeout_seconds,
            base_fee,
            network_passphrase,
        } => {
            build_donation_tx(
                &donor,
                &donor_sequence,
                &amount,
                &asset,
                issuer.as_deref(),
                &project_id,
                destination.as_deref(),
                timeout_seconds,
                base_fee,
                network_passphrase.as_deref(),
            )?;
        },
        Commands::BuildInvokeTx {
            source,
            sequence,
            contract,
            function,
            args,
            timeout_seconds,
            base_fee,
            network_passphrase,
            soroban_data_xdr,
        } => {
            build_invoke_tx(
                &source,
                &sequence,
                &contract,
                &function,
                args.as_deref(),
                timeout_seconds,
                base_fee,
                network_passphrase.as_deref(),
                soroban_data_xdr.as_deref(),
            )?;
        },
        Commands::PrepareWalletSigning {
            wallet,
            xdr,
            network_passphrase,
            public_key,
            callback_url,
            timeout_seconds,
            log_file,
        } => {
            prepare_wallet_signing(
                &wallet,
                &xdr,
                network_passphrase.as_deref(),
                public_key.as_deref(),
                callback_url.as_deref(),
                timeout_seconds,
                &log_file,
            )?;
        },
        Commands::CompleteWalletSigning {
            wallet,
            attempt_id,
            response,
            started_at_unix,
            timeout_seconds,
            log_file,
        } => {
            complete_wallet_signing(
                &wallet,
                &attempt_id,
                &response,
                started_at_unix,
                timeout_seconds,
                &log_file,
            )?;
        },
        Commands::SubmitTx {
            xdr,
            network,
            timeout_seconds,
            max_retries,
            no_retry,
            log_file,
        } => {
            submit_transaction(
                &xdr,
                &network,
                timeout_seconds,
                max_retries,
                no_retry,
                &log_file,
            )?;
        },
        Commands::SubmissionStatus {
            detailed,
            tx_hash,
            log_file,
        } => {
            show_submission_status(detailed, tx_hash.as_deref(), &log_file)?;
        },
        Commands::VerifyTx {
            hash,
            network,
            timeout_seconds,
        } => {
            verify_transaction(&hash, &network, timeout_seconds)?;
        },
        Commands::TxHistory {
            account,
            limit,
            tx_type,
            order,
            export_csv,
            summary,
            network,
        } => {
            get_transaction_history(&account, limit, tx_type.as_deref(), &order, export_csv.as_deref(), summary, &network)?;
        },
        Commands::Batch { action } => match action {
            BatchAction::Execute {
                file,
                parallel,
                continue_on_error,
                max_concurrent,
                export_results,
            } => {
                execute_batch_operations(&file, parallel, continue_on_error, max_concurrent, export_results.as_deref())?;
            },
            BatchAction::CreateTemplate {
                output,
                operation_type,
            } => {
                create_batch_template(&output, &operation_type)?;
            },
        },
        Commands::Debug { action } => match action {
            DebugAction::Collect {
                account,
                contract,
                export,
                network,
            } => {
                collect_debug_info(&account, contract.as_deref(), export.as_deref(), &network)?;
            },
            DebugAction::AnalyzeFailure {
                tx_hash,
                account,
                network,
            } => {
                analyze_transaction_failure(&tx_hash, &account, &network)?;
            },
            DebugAction::NetworkStatus {
                network,
                detailed,
            } => {
                check_network_status(&network, detailed)?;
            },
        },
        Commands::Contract { action } => match action {
            ContractAction::Info {
                contract,
                format,
                output,
            } => {
                get_contract_info(&contract, &format, output.as_deref())?;
            },
            ContractAction::Query {
                contract,
                method,
                args,
                simulate,
                network,
            } => {
                query_contract(&contract, &method, args.as_deref(), simulate, &network)?;
            },
            ContractAction::State {
                contract,
                export,
                network,
            } => {
                get_contract_state(&contract, export.as_deref(), &network)?;
            },
            ContractAction::Template {
                contract,
                method,
                output,
            } => {
                generate_contract_template(&contract, &method, output.as_deref())?;
            },
        },
        Commands::Account { action } => match action {
            AccountAction::Create {
                save,
                password,
                generate_mnemonic,
            } => {
                create_account(save, password.as_deref(), generate_mnemonic)?;
            },
            AccountAction::Import {
                private_key,
                mnemonic,
                save,
                password,
            } => {
                import_account(private_key.as_deref(), mnemonic.as_deref(), save, password.as_deref())?;
            },
            AccountAction::Export {
                account,
                password,
                format,
            } => {
                export_account(&account, &password, &format)?;
            },
            AccountAction::List { detailed } => {
                list_accounts(detailed)?;
            },
            AccountAction::Balance {
                account,
                network,
            } => {
                get_account_balance(&account, &network)?;
            },
            AccountAction::Signers {
                account,
                network,
            } => {
                get_account_signers(&account, &network)?;
            },
            AccountAction::Fund {
                account,
                network,
            } => {
                fund_account(&account, &network)?;
            },
            AccountAction::ConnectWallet {
                wallet_type,
            } => {
                connect_wallet(&wallet_type)?;
            },
            AccountAction::ValidateAddress {
                address,
            } => {
                validate_address(&address)?;
            },
        },
    }

    Ok(())
}

fn status_indicator(status: &SigningStatus) -> &'static str {
    match status {
        SigningStatus::AwaitingUser => "🟡",
        SigningStatus::Signed => "✅",
        SigningStatus::Rejected => "🛑",
        SigningStatus::TimedOut => "⏱️",
        SigningStatus::Invalid => "❌",
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_wallet_signing(
    wallet: &str,
    xdr: &str,
    network_passphrase_override: Option<&str>,
    public_key: Option<&str>,
    callback_url: Option<&str>,
    timeout_seconds: u64,
    log_file: &str,
) -> Result<()> {
    let wallet = wallet.parse::<WalletType>()?;
    let network_passphrase = if let Some(passphrase) = network_passphrase_override {
        passphrase.to_string()
    } else {
        Config::load(None)
            .map(|cfg| cfg.network_passphrase)
            .context(
                "Failed to resolve network passphrase from config. Pass --network-passphrase or configure soroban.toml",
            )?
    };

    let service = WalletSigningService::new(PathBuf::from(log_file));
    let prepared = service.prepare_signing(PrepareSigningRequest {
        wallet,
        unsigned_xdr: xdr.to_string(),
        network_passphrase,
        public_key: public_key.map(ToString::to_string),
        callback_url: callback_url.map(ToString::to_string),
        timeout_seconds,
    })?;

    println!(
        "{} Wallet signing request prepared",
        status_indicator(&prepared.status)
    );
    println!("  Wallet: {}", prepared.wallet.as_str());
    println!("  Attempt ID: {}", prepared.attempt_id);
    println!("  Status: {:?}", prepared.status);
    println!("  Message: {}", prepared.message);
    println!("  Started At: {}", prepared.created_at_unix);
    println!("  Expires At: {}", prepared.expires_at_unix);
    if let Some(launch_url) = &prepared.launch_url {
        println!("  Launch URL: {}", launch_url);
    }
    println!("  Request Payload: {}", prepared.request_payload);
    println!("  Log File: {}", log_file);

    Ok(())
}

fn complete_wallet_signing(
    wallet: &str,
    attempt_id: &str,
    response: &str,
    started_at_unix: u64,
    timeout_seconds: u64,
    log_file: &str,
) -> Result<()> {
    let wallet = wallet.parse::<WalletType>()?;
    let service = WalletSigningService::new(PathBuf::from(log_file));

    let completion = service.complete_signing(CompleteSigningRequest {
        attempt_id: attempt_id.to_string(),
        wallet,
        wallet_response: response.to_string(),
        started_at_unix,
        timeout_seconds,
    })?;

    println!(
        "{} Wallet signing completion",
        status_indicator(&completion.status)
    );
    println!("  Wallet: {}", completion.wallet.as_str());
    println!("  Attempt ID: {}", completion.attempt_id);
    println!("  Status: {:?}", completion.status);
    println!("  Message: {}", completion.message);

    if let Some(signed_xdr) = completion.signed_xdr {
        println!("  Signed XDR: {}", signed_xdr);
    }
    if let Some(envelope_xdr) = completion.envelope_xdr {
        println!("  Envelope XDR: {}", envelope_xdr);
    }
    println!("  Log File: {}", log_file);

    Ok(())
}

fn resolve_platform_public_key(destination_override: Option<&str>) -> Result<String> {
    if let Some(destination) = destination_override {
        return Ok(destination.to_string());
    }

    env::var("STELLARAID_PLATFORM_PUBLIC_KEY")
        .or_else(|_| env::var("PLATFORM_PUBLIC_KEY"))
        .context(
            "Missing destination account. Pass --destination or set STELLARAID_PLATFORM_PUBLIC_KEY",
        )
}

#[allow(clippy::too_many_arguments)]
fn build_donation_tx(
    donor: &str,
    donor_sequence: &str,
    amount: &str,
    asset: &str,
    issuer: Option<&str>,
    project_id: &str,
    destination_override: Option<&str>,
    timeout_seconds: i64,
    base_fee: u32,
    network_passphrase_override: Option<&str>,
) -> Result<()> {
    let destination = resolve_platform_public_key(destination_override)?;

    let network_passphrase = if let Some(passphrase) = network_passphrase_override {
        passphrase.to_string()
    } else {
        Config::load(None)
            .map(|cfg| cfg.network_passphrase)
            .context(
                "Failed to resolve network passphrase from config. Pass --network-passphrase or configure soroban.toml",
            )?
    };

    let request = BuildDonationTxRequest {
        donor_address: donor.to_string(),
        donor_sequence: donor_sequence.to_string(),
        platform_address: destination,
        donation_amount: amount.to_string(),
        asset_code: asset.to_string(),
        asset_issuer: issuer.map(ToString::to_string),
        project_id: project_id.to_string(),
        network_passphrase,
        timeout_seconds,
        base_fee_stroops: base_fee,
    };

    match build_donation_transaction(request) {
        Ok(result) => {
            println!("✅ Donation transaction built successfully");
            println!("  Destination: {}", result.destination);
            println!("  Asset: {}", result.asset);
            println!("  Amount (stroops): {}", result.amount_stroops);
            println!("  Memo: {}", result.memo);
            println!("  Fee (stroops): {}", result.fee);
            println!("  XDR (ready for signing): {}", result.xdr);
            Ok(())
        },
        Err(err) => {
            eprintln!("❌ Failed to build donation transaction: {}", err);
            std::process::exit(1);
        },
    }
}

fn build_invoke_tx(
    source: &str,
    sequence: &str,
    contract: &str,
    function: &str,
    args_json: Option<&str>,
    timeout_seconds: i64,
    base_fee: u32,
    network_passphrase_override: Option<&str>,
    soroban_data_xdr: Option<&str>,
) -> Result<()> {
    let network_passphrase = if let Some(passphrase) = network_passphrase_override {
        passphrase.to_string()
    } else {
        Config::load(None)
            .map(|cfg| cfg.network_passphrase)
            .context(
                "Failed to resolve network passphrase. Pass --network-passphrase or configure soroban.toml",
            )?
    };

    let arg_vals: Vec<serde_json::Value> = match args_json {
        None | Some("") => Vec::new(),
        Some(raw) => serde_json::from_str(raw).context("Failed to parse --args as JSON array")?,
    };
    let sc_args = json_to_sc_vals(&arg_vals).map_err(|e| anyhow::anyhow!(e))?;

    let request = BuildSorobanInvokeRequest {
        source_account: source.to_string(),
        sequence: sequence.to_string(),
        contract_id: contract.to_string(),
        function_name: function.to_string(),
        args: sc_args,
        network_passphrase,
        timeout_seconds,
        base_fee_stroops: base_fee,
        soroban_data_xdr: soroban_data_xdr.map(String::from),
    };

    match build_soroban_invoke_transaction(request) {
        Ok(result) => {
            println!("✅ Soroban invoke transaction built (unsigned)");
            println!("  Contract: {}", contract);
            println!("  Function: {}", function);
            println!("  Total fee (stroops): {}", result.fee_stroops);
            println!("  Operations: {}", result.operation_count);
            println!("  XDR (sign this envelope): {}", result.xdr);
            Ok(())
        },
        Err(err) => {
            eprintln!("❌ Failed to build Soroban invoke transaction: {}", err);
            std::process::exit(1);
        },
    }
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
        PathBuf::from(
            "crates/contracts/core/target/wasm32-unknown-unknown/debug/stellaraid_core.wasm",
        ),
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

    anyhow::bail!("WASM file not found. Build with 'make wasm' or specify with --wasm flag")
}

/// Store the contract ID in a local file
fn store_contract_id(contract_id: &str, network: &str) -> Result<()> {
    let cwd = env::current_dir()?;
    let file_path = cwd.join(CONTRACT_ID_FILE);

    let content = if file_path.exists() {
        let existing: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&file_path)?).unwrap_or(serde_json::json!({}));
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
        anyhow::bail!("No contract ID found. Deploy a contract first with 'deploy' command");
    }

    let content: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file_path)?)?;

    if let Some(id) = content.get(network).and_then(|v| v.as_str()) {
        Ok(id.to_string())
    } else {
        let available = content
            .as_object()
            .map(|obj| obj.keys().cloned().collect::<Vec<_>>().join(", "))
            .unwrap_or_else(|| "none".to_string());
        anyhow::bail!(
            "No contract ID found for network '{}'. Available: {}",
            network,
            available
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
            "--wasm",
            wasm.to_str().unwrap(),
            "--network",
            network,
            "--rpc-url",
            &config.rpc_url,
            "--network-passphrase",
            &config.network_passphrase,
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
                    "--network",
                    network,
                    "--rpc-url",
                    &config.rpc_url,
                    "--network-passphrase",
                    &config.network_passphrase,
                    contract_id,
                    "--",
                    "init",
                    "--admin",
                    admin_key,
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
        "contract".to_string(),
        "invoke".to_string(),
        "--network".to_string(),
        network.clone(),
        "--rpc-url".to_string(),
        config.rpc_url.clone(),
        "--network-passphrase".to_string(),
        config.network_passphrase.clone(),
        contract_id.clone(),
        "--".to_string(),
        method.to_string(),
    ];

    // Add arguments if provided
    if let Some(arguments) = args {
        // Parse JSON arguments and add them
        let parsed: serde_json::Value =
            serde_json::from_str(arguments).context("Failed to parse arguments as JSON")?;

        if let Some(arr) = parsed.as_array() {
            for val in arr {
                cmd_args.push(val.to_string());
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
# Network: testnet, mainnet, or sandbox (selects a profile in soroban.toml)
SOROBAN_NETWORK=testnet

# RPC URL (optional - will use soroban.toml if not set)
# SOROBAN_RPC_URL=https://soroban-testnet.stellar.org

# Horizon REST URL for transaction submit / verify (optional - uses soroban.toml or network defaults)
# SOROBAN_HORIZON_URL=https://horizon-testnet.stellar.org

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

/// Submit a signed transaction to the Stellar network
fn submit_transaction(
    xdr: &str,
    network: &str,
    timeout_seconds: u64,
    max_retries: u32,
    no_retry: bool,
    log_file: &str,
) -> Result<()> {
    use std::time::Duration;

    println!("🚀 Submitting transaction to {}...", network);

    let app_cfg =
        Config::load_for_network(network).context("Failed to load configuration for network")?;

    // Build configuration
    let config = SubmissionConfig {
        horizon_url: app_cfg.horizon_url.clone(),
        timeout: Duration::from_secs(timeout_seconds),
        max_retries: if no_retry { 0 } else { max_retries },
        log_path: Some(PathBuf::from(log_file)),
        ..Default::default()
    };

    // Create submission service
    let service = TransactionSubmissionService::with_config(config)
        .map_err(|e| anyhow::anyhow!("Failed to create submission service: {}", e))?;

    // Create submission request
    let request = SubmissionRequest::new(xdr)
        .with_timeout(Duration::from_secs(timeout_seconds))
        .with_retries(if no_retry { 0 } else { max_retries });

    // Run the submission
    let runtime = tokio::runtime::Runtime::new()?;
    let response = runtime.block_on(service.submit(request));

    // Display results
    match response.status {
        transaction_submission::SubmissionStatus::Success => {
            println!("✅ Transaction submitted successfully!");
            println!("   Transaction Hash: {}", response.transaction_hash.as_ref().unwrap());
            if let Some(ledger) = response.ledger_sequence {
                println!("   Ledger Sequence: {}", ledger);
            }
            println!("   Attempts: {}", response.attempts);
        }
        transaction_submission::SubmissionStatus::Duplicate => {
            println!("⚠️  Transaction already submitted (duplicate)");
            println!("   Transaction Hash: {}", response.transaction_hash.as_ref().unwrap());
        }
        _ => {
            eprintln!("❌ Transaction submission failed");
            eprintln!("   Status: {:?}", response.status);
            if let Some(error) = &response.error_message {
                eprintln!("   Error: {}", error);
            }
            if let Some(code) = &response.error_code {
                eprintln!("   Error Code: {}", code);
            }
            eprintln!("   Attempts: {}", response.attempts);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Verify a transaction on-chain by querying Horizon
fn verify_transaction(hash: &str, network: &str, timeout_seconds: u64) -> Result<()> {
    use std::time::Duration;
    use transaction_verification::VerificationConfig;

    println!("Verifying transaction on {}...", network);
    println!("  Hash: {}", hash);

    let app_cfg =
        Config::load_for_network(network).context("Failed to load configuration for network")?;
    let config = VerificationConfig::default()
        .with_horizon_url(app_cfg.horizon_url.clone())
        .with_timeout(Duration::from_secs(timeout_seconds));

    let service = TransactionVerificationService::with_config(config)
        .map_err(|e| anyhow::anyhow!("Failed to create verification service: {}", e))?;

    let request = VerificationRequest::new(hash).with_timeout(Duration::from_secs(timeout_seconds));

    let runtime = tokio::runtime::Runtime::new()?;
    let response = runtime
        .block_on(service.verify(request))
        .map_err(|e| anyhow::anyhow!("Verification error: {}", e))?;

    // Display result
    match response.status {
        transaction_verification::VerificationStatus::Confirmed => {
            println!("Transaction confirmed on-chain!");
            if let Some(ledger) = response.ledger_sequence {
                println!("  Ledger: {}", ledger);
            }
            if let Some(time) = &response.ledger_close_time {
                println!("  Ledger Close Time: {}", time);
            }
            if let Some(fee) = &response.fee_charged {
                println!("  Fee Charged (stroops): {}", fee);
            }
            if let Some(contract) = &response.contract_result {
                println!("  Contract Execution: success={}", contract.success);
                if let Some(xdr) = &contract.return_value_xdr {
                    println!("  Return Value XDR: {}", xdr);
                }
                if !contract.events.is_empty() {
                    println!("  Contract Events: {}", contract.events.len());
                }
                if !contract.operation_results.is_empty() {
                    println!("  Operations: {}", contract.operation_results.len());
                }
            }
        }
        transaction_verification::VerificationStatus::Failed => {
            eprintln!("Transaction is on-chain but failed!");
            if let Some(code) = &response.result_code {
                eprintln!("  Result Code: {}", code);
            }
            if let Some(msg) = &response.error_message {
                eprintln!("  Reason: {}", msg);
            }
            std::process::exit(1);
        }
        transaction_verification::VerificationStatus::NotFound => {
            eprintln!("Transaction not found on {}.", network);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Show transaction submission status and statistics
fn show_submission_status(
    detailed: bool,
    tx_hash_filter: Option<&str>,
    log_file: &str,
) -> Result<()> {
    let logger = SubmissionLogger::new(log_file);

    // Load logs from file
    let logs = logger.load_from_file()?;

    if logs.is_empty() {
        println!("No submission logs found.");
        return Ok(());
    }

    // Filter by transaction hash if specified
    let filtered_logs: Vec<_> = if let Some(hash) = tx_hash_filter {
        logs.into_iter()
            .filter(|log| {
                log.transaction_hash
                    .as_ref()
                    .map(|h| h == hash)
                    .unwrap_or(false)
            })
            .collect()
    } else {
        logs
    };

    if filtered_logs.is_empty() {
        println!("No submissions found matching the criteria.");
        return Ok(());
    }

    // Show statistics
    let stats = logger.get_stats();
    println!("📊 Submission Statistics");
    println!("   Total: {}", stats.total);
    println!("   Successful: {}", stats.successful);
    println!("   Failed: {}", stats.failed);
    println!("   Pending: {}", stats.pending);
    println!("   Duplicates: {}", stats.duplicates);
    println!("   Avg Duration: {}ms", stats.avg_duration_ms);

    // Show detailed logs if requested
    if detailed {
        println!("\n📋 Recent Submissions:");
        for log in filtered_logs.iter().rev().take(10) {
            println!("\n   Request ID: {}", log.request_id);
            println!("   Status: {}", log.status);
            if let Some(hash) = &log.transaction_hash {
                println!("   Transaction Hash: {}", hash);
            }
            if let Some(ledger) = log.ledger_sequence {
                println!("   Ledger: {}", ledger);
            }
            println!("   Timestamp: {}", log.timestamp);
            println!("   Duration: {}ms", log.duration_ms);
            println!("   Attempts: {}", log.attempts);
            if let Some(error) = &log.error_message {
                println!("   Error: {}", error);
            }
            println!("   ---");
        }
    }

    Ok(())
}

// New handler functions for advanced CLI commands

fn get_transaction_history(
    account: &str,
    limit: u32,
    tx_type: Option<&str>,
    order: &str,
    export_csv: Option<&str>,
    summary: bool,
    network: &str,
) -> Result<()> {
    // Validate inputs
    InputValidator::validate_stellar_address(account)?;
    InputValidator::validate_range(&limit.to_string(), 1.0, 200.0)?;
    InputValidator::validate_network(network)?;
    
    let tx_type_filter = match tx_type {
        Some("payment") => Some(TransactionType::Payment),
        Some("donation") => Some(TransactionType::Donation),
        Some("contract") => Some(TransactionType::ContractInvocation),
        Some("deploy") => Some(TransactionType::ContractDeploy),
        Some(_) => None,
        None => None,
    };
    
    let order_filter = match order {
        "asc" => Order::Asc,
        "desc" => Order::Desc,
        _ => return Err(anyhow::anyhow!("Invalid order: {}. Use 'asc' or 'desc'", order)),
    };
    
    println!("🔍 Fetching transaction history for account: {}", account);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    let request = TransactionHistoryRequest {
        account_id: account.to_string(),
        limit: Some(limit),
        cursor: None,
        order: Some(order_filter),
        tx_type: tx_type_filter,
        start_time: None,
        end_time: None,
    };
    
    let transactions = rt.block_on(
        TransactionHistoryService::get_transaction_history(&horizon_client, request)
    )?;
    
    if transactions.is_empty() {
        println!("No transactions found.");
        return Ok(());
    }
    
    println!("📊 Found {} transactions", transactions.len());
    
    if summary {
        let summary = TransactionHistoryService::generate_summary(&transactions);
        println!("\n📈 Transaction Summary:");
        println!("   Total: {}", summary.total_transactions);
        println!("   Successful: {}", summary.successful_transactions);
        println!("   Failed: {}", summary.failed_transactions);
        println!("   Total Fees: {} stroops", summary.total_fees);
        println!("   Payments: {} ({:.7} XLM)", summary.payment_transactions, summary.total_payment_amount);
        println!("   Donations: {} ({:.7} XLM)", summary.donation_transactions, summary.total_donation_amount);
        println!("   Contract Invocations: {}", summary.contract_invocations);
        println!("   Contract Deploys: {}", summary.contract_deploys);
    }
    
    // Display transactions
    println!("\n📋 Recent Transactions:");
    for (i, tx) in transactions.iter().take(10).enumerate() {
        println!("\n{}. {}", i + 1, tx.hash);
        println!("   Type: {:?}", tx.tx_type);
        println!("   Status: {}", if tx.successful { "✅ Success" } else { "❌ Failed" });
        println!("   Ledger: {}", tx.ledger);
        println!("   Fee: {} stroops", tx.fee_paid);
        println!("   Operations: {}", tx.operation_count);
        println!("   Date: {}", tx.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
        if let Some(memo) = &tx.memo {
            println!("   Memo: {}", memo);
        }
        if let Some(amount) = tx.amount {
            println!("   Amount: {:.7} {}", amount, tx.asset.as_deref().unwrap_or("XLM"));
        }
    }
    
    // Export to CSV if requested
    if let Some(csv_path) = export_csv {
        let csv_content = TransactionHistoryService::export_to_csv(&transactions)?;
        fs::write(csv_path, csv_content)?;
        println!("📁 Exported transactions to: {}", csv_path);
    }
    
    Ok(())
}

fn execute_batch_operations(
    file: &str,
    parallel: bool,
    continue_on_error: bool,
    max_concurrent: Option<usize>,
    export_results: Option<&str>,
) -> Result<()> {
    InputValidator::validate_file_path(file)?;
    
    if let Some(max) = max_concurrent {
        InputValidator::validate_range(&max.to_string(), 1.0, 100.0)?;
    }
    
    println!("🔧 Executing batch operations from: {}", file);
    
    let csv_content = fs::read_to_string(file)?;
    let batch_request = BatchOperationService::create_batch_from_csv(&csv_content)?;
    
    InputValidator::validate_batch_size(batch_request.operations.len())?;
    
    println!("📊 Found {} operations", batch_request.operations.len());
    
    let config = Config::load(None)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        BatchOperationService::execute_batch(&horizon_client, batch_request)
    )?;
    
    println!("✅ Batch execution completed");
    println!("   Batch ID: {}", result.batch_id);
    println!("   Total Operations: {}", result.total_operations);
    println!("   Successful: {}", result.successful_operations);
    println!("   Failed: {}", result.failed_operations);
    println!("   Execution Time: {}ms", result.execution_time_ms);
    
    // Export results if requested
    if let Some(export_path) = export_results {
        let csv_content = BatchOperationService::export_batch_results(&result)?;
        fs::write(export_path, csv_content)?;
        println!("📁 Exported results to: {}", export_path);
    }
    
    Ok(())
}

fn create_batch_template(output: &str, operation_type: &str) -> Result<()> {
    InputValidator::validate_file_path(output)?;
    
    let template = match operation_type {
        "payment" => "payment,destination,amount,asset,issuer\n",
        "donation" => "donation,donor,project_id,amount,asset\n",
        "invoke" => "invoke,contract,method,arg1,arg2\n",
        "deploy" => "deploy,source,wasm_path\n",
        _ => return Err(anyhow::anyhow!("Invalid operation type: {}", operation_type)),
    };
    
    fs::write(output, template)?;
    println!("📝 Created batch template: {}", output);
    
    Ok(())
}

fn collect_debug_info(
    account: &str,
    contract: Option<&str>,
    export: Option<&str>,
    network: &str,
) -> Result<()> {
    InputValidator::validate_stellar_address(account)?;
    InputValidator::validate_network(network)?;
    
    if let Some(contract_id) = contract {
        InputValidator::validate_contract_id(contract_id)?;
    }
    
    println!("🔍 Collecting debug information...");
    
    let config = Config::load_for_network(network)?;
    let rt = tokio::runtime::Runtime::new()?;
    
    let debug_info = rt.block_on(
        DebugService::collect_debug_info(&config, Some(account), contract)
    )?;
    
    println!("✅ Debug information collected");
    println!("   Network: {}", debug_info.network_info.network);
    println!("   Latest Ledger: {}", debug_info.network_info.latest_ledger);
    println!("   Horizon Status: {}", debug_info.network_info.horizon_status);
    println!("   Response Time: {}ms", debug_info.network_info.response_time_ms);
    
    println!("\n💰 Account Information:");
    println!("   Account: {}", debug_info.account_info.account_id);
    println!("   Sequence: {}", debug_info.account_info.sequence_number);
    println!("   Balance: {} XLM", 
        debug_info.account_info.balance.get("XLM").unwrap_or(&0.0));
    println!("   Signers: {}", debug_info.account_info.signers.len());
    
    if let Some(contract_info) = &debug_info.contract_info {
        println!("\n📄 Contract Information:");
        println!("   Contract ID: {}", contract_info.contract_id);
        println!("   WASM Hash: {}", contract_info.wasm_hash);
    }
    
    println!("\n📈 Performance Metrics:");
    println!("   RPC Response Time: {}ms", debug_info.performance_metrics.rpc_response_time_ms);
    println!("   Horizon Response Time: {}ms", debug_info.performance_metrics.horizon_response_time_ms);
    println!("   Memory Usage: {:.1}MB", debug_info.performance_metrics.memory_usage_mb);
    
    // Export if requested
    if let Some(export_path) = export {
        let report = DebugService::export_debug_report(&debug_info)?;
        fs::write(export_path, report)?;
        println!("📁 Exported debug report to: {}", export_path);
    }
    
    Ok(())
}

fn analyze_transaction_failure(tx_hash: &str, account: &str, network: &str) -> Result<()> {
    InputValidator::validate_transaction_hash(tx_hash)?;
    InputValidator::validate_stellar_address(account)?;
    InputValidator::validate_network(network)?;
    
    println!("🔍 Analyzing transaction failure: {}", tx_hash);
    
    let config = Config::load_for_network(network)?;
    let rt = tokio::runtime::Runtime::new()?;
    
    let debug_info = rt.block_on(
        DebugService::collect_debug_info(&config, Some(account), None)
    )?;
    
    let analysis = DebugService::analyze_transaction_failure(&debug_info, tx_hash)?;
    
    println!("📊 Failure Analysis:");
    println!("   {}", analysis);
    
    Ok(())
}

fn check_network_status(network: &str, detailed: bool) -> Result<()> {
    InputValidator::validate_network(network)?;
    
    println!("🌐 Checking network status: {}", network);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    
    // Basic health check
    let checker = HorizonHealthChecker::default_config();
    let health = rt.block_on(checker.check(&horizon_client))?;
    
    println!("✅ Network Status: {}", health.status);
    println!("   Response Time: {}ms", health.response_time_ms);
    
    if detailed {
        println!("\n📊 Detailed Metrics:");
        
        // Get fee stats
        let fee_stats = rt.block_on(
            DebugService::collect_fee_stats(&horizon_client)
        )?;
        
        println!("   Base Fee: {} stroops", fee_stats.base_fee);
        println!("   Base Reserve: {} stroops", fee_stats.base_reserve);
        println!("   Recommended Fee: {} stroops", fee_stats.recommended_fee);
        
        println!("\n💰 Fee Distribution (percentiles):");
        println!("   p10: {} stroops", fee_stats.fee_distribution.p10);
        println!("   p25: {} stroops", fee_stats.fee_distribution.p25);
        println!("   p50: {} stroops", fee_stats.fee_distribution.p50);
        println!("   p75: {} stroops", fee_stats.fee_distribution.p75);
        println!("   p90: {} stroops", fee_stats.fee_distribution.p90);
        println!("   p95: {} stroops", fee_stats.fee_distribution.p95);
        println!("   p99: {} stroops", fee_stats.fee_distribution.p99);
    }
    
    Ok(())
}

fn get_contract_info(contract: &str, format: &str, output: Option<&str>) -> Result<()> {
    InputValidator::validate_contract_id(contract)?;
    
    let export_format = match format {
        "json" => ExportFormat::Json,
        "markdown" => ExportFormat::Markdown,
        _ => return Err(anyhow::anyhow!("Invalid format: {}. Use 'json' or 'markdown'", format)),
    };
    
    println!("📄 Getting contract information: {}", contract);
    
    let config = Config::load(None)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    let methods = rt.block_on(
        ContractInteractionService::get_contract_info(&horizon_client, contract)
    )?;
    
    let export_content = ContractInteractionService::export_contract_methods(&methods, export_format)?;
    
    if let Some(output_path) = output {
        fs::write(output_path, export_content)?;
        println!("📁 Exported contract info to: {}", output_path);
    } else {
        println!("{}", export_content);
    }
    
    Ok(())
}

fn query_contract(
    contract: &str,
    method: &str,
    args: Option<&str>,
    simulate: bool,
    network: &str,
) -> Result<()> {
    InputValidator::validate_contract_id(contract)?;
    InputValidator::validate_network(network)?;
    
    let args_parsed = if let Some(args_str) = args {
        Some(serde_json::from_str::<Vec<serde_json::Value>>(args_str)?)
    } else {
        None
    };
    
    println!("🔍 Querying contract: {}", contract);
    println!("   Method: {}", method);
    println!("   Simulate: {}", simulate);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    let request = ContractQueryRequest {
        contract_id: contract.to_string(),
        method: method.to_string(),
        args: args_parsed,
        auth_required: false,
        simulate_only: simulate,
    };
    
    let response = rt.block_on(
        ContractInteractionService::query_contract(&horizon_client, &config, request)
    )?;
    
    println!("✅ Query completed");
    println!("   Success: {}", response.success);
    println!("   Result: {}", serde_json::to_string_pretty(&response.result)?);
    
    if let Some(gas_used) = response.gas_used {
        println!("   Gas Used: {}", gas_used);
    }
    
    if !response.events.is_empty() {
        println!("   Events: {}", response.events.len());
    }
    
    if let Some(error) = response.error {
        println!("   Error: {}", error);
    }
    
    Ok(())
}

fn get_contract_state(contract: &str, export: Option<&str>, network: &str) -> Result<()> {
    InputValidator::validate_contract_id(contract)?;
    InputValidator::validate_network(network)?;
    
    println!("🔍 Getting contract state: {}", contract);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    let state = rt.block_on(
        ContractInteractionService::get_contract_state(&horizon_client, contract)
    )?;
    
    println!("✅ Contract state retrieved");
    println!("   Contract ID: {}", state.contract_id);
    println!("   Instance Data: {} entries", state.instance_data.len());
    println!("   Persistent Storage: {} entries", state.persistent_storage.len());
    println!("   Temporary Storage: {} entries", state.temporary_storage.len());
    
    // Export if requested
    if let Some(export_path) = export {
        let state_json = serde_json::to_string_pretty(&state)?;
        fs::write(export_path, state_json)?;
        println!("📁 Exported contract state to: {}", export_path);
    }
    
    Ok(())
}

fn generate_contract_template(contract: &str, method: &str, output: Option<&str>) -> Result<()> {
    InputValidator::validate_contract_id(contract)?;
    
    println!("📝 Generating method template: {}::{}", contract, method);
    
    let config = Config::load(None)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let rt = tokio::runtime::Runtime::new()?;
    let methods = rt.block_on(
        ContractInteractionService::get_contract_info(&horizon_client, contract)
    )?;
    
    let method_info = methods.iter()
        .find(|m| m.name == method)
        .ok_or_else(|| anyhow::anyhow!("Method '{}' not found in contract", method))?;
    
    let template = ContractInteractionService::generate_method_call_template(method_info, contract)?;
    
    if let Some(output_path) = output {
        fs::write(output_path, template)?;
        println!("📁 Template saved to: {}", output_path);
    } else {
        println!("{}", template);
    }
    
    Ok(())
}

fn create_account(save: bool, password: Option<&str>, generate_mnemonic: bool) -> Result<()> {
    if save && password.is_none() {
        return Err(anyhow::anyhow!("Password required when saving account"));
    }
    
    println!("🔐 Creating new account...");
    
    let request = AccountManagementRequest {
        action: AccountAction::Create,
        account_id: None,
        wallet_type: None,
        private_key: None,
        mnemonic: None,
        password: password.map(|p| p.to_string()),
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&HorizonClient::with_config(HorizonClientConfig::default())?, request)
    )?;
    
    let account_info: serde_json::Value = result;
    
    println!("✅ Account created successfully");
    println!("   Account ID: {}", account_info["account_id"]);
    println!("   Public Key: {}", account_info["public_key"]);
    
    if generate_mnemonic {
        if let Some(mnemonic) = account_info.get("mnemonic") {
            println!("   Mnemonic: {}", mnemonic);
            println!("   ⚠️  Save this mnemonic phrase securely!");
        }
    }
    
    if save {
        println!("   🔐 Account saved to secure vault");
    } else {
        println!("   Secret Key: {}", account_info["secret_key"]);
        println!("   ⚠️  Save this secret key securely!");
    }
    
    Ok(())
}

fn import_account(
    private_key: Option<&str>,
    mnemonic: Option<&str>,
    save: bool,
    password: Option<&str>,
) -> Result<()> {
    if private_key.is_none() && mnemonic.is_none() {
        return Err(anyhow::anyhow!("Either private key or mnemonic required"));
    }
    
    if save && password.is_none() {
        return Err(anyhow::anyhow!("Password required when saving account"));
    }
    
    println!("🔐 Importing account...");
    
    if let Some(key) = private_key {
        InputValidator::validate_private_key(key)?;
    }
    
    if let Some(mnemonic_phrase) = mnemonic {
        InputValidator::validate_mnemonic(mnemonic_phrase)?;
    }
    
    let request = AccountManagementRequest {
        action: AccountAction::Import,
        account_id: None,
        wallet_type: None,
        private_key: private_key.map(|k| k.to_string()),
        mnemonic: mnemonic.map(|m| m.to_string()),
        password: password.map(|p| p.to_string()),
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&HorizonClient::with_config(HorizonClientConfig::default())?, request)
    )?;
    
    let account_info: serde_json::Value = result;
    
    println!("✅ Account imported successfully");
    println!("   Account ID: {}", account_info["account_id"]);
    println!("   Public Key: {}", account_info["public_key"]);
    
    if save {
        println!("   🔐 Account saved to secure vault");
    }
    
    Ok(())
}

fn export_account(account: &str, password: &str, format: &str) -> Result<()> {
    InputValidator::validate_stellar_address(account)?;
    
    if format != "json" {
        return Err(anyhow::anyhow!("Only JSON format is supported for export"));
    }
    
    println!("📤 Exporting account: {}", account);
    
    let request = AccountManagementRequest {
        action: AccountAction::Export,
        account_id: Some(account.to_string()),
        wallet_type: None,
        private_key: None,
        mnemonic: None,
        password: Some(password.to_string()),
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&HorizonClient::with_config(HorizonClientConfig::default())?, request)
    )?;
    
    println!("✅ Account exported");
    println!("{}", serde_json::to_string_pretty(&result)?);
    
    Ok(())
}

fn list_accounts(detailed: bool) -> Result<()> {
    println!("📋 Listing accounts...");
    
    let request = AccountManagementRequest {
        action: AccountAction::List,
        account_id: None,
        wallet_type: None,
        private_key: None,
        mnemonic: None,
        password: None,
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&HorizonClient::with_config(HorizonClientConfig::default())?, request)
    )?;
    
    let accounts: serde_json::Value = result;
    
    if let Some(accounts_array) = accounts["accounts"].as_array() {
        if accounts_array.is_empty() {
            println!("No accounts found.");
            return Ok(());
        }
        
        println!("📊 Found {} accounts", accounts_array.len());
        
        for (i, account) in accounts_array.iter().enumerate() {
            println!("\n{}. {}", i + 1, account["account_id"]);
            if detailed {
                println!("   Public Key: {}", account["public_key"]);
                println!("   Created: {}", account["created_at"]);
            }
        }
    }
    
    Ok(())
}

fn get_account_balance(account: &str, network: &str) -> Result<()> {
    InputValidator::validate_stellar_address(account)?;
    InputValidator::validate_network(network)?;
    
    println!("💰 Getting account balance: {}", account);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let request = AccountManagementRequest {
        action: AccountAction::Balance,
        account_id: Some(account.to_string()),
        wallet_type: None,
        private_key: None,
        mnemonic: None,
        password: None,
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&horizon_client, request)
    )?;
    
    let balance_info: serde_json::Value = result;
    
    println!("✅ Account balance retrieved");
    println!("   Account: {}", balance_info["account_id"]);
    println!("   Sequence: {}", balance_info["sequence"]);
    println!("   Total XLM Balance: {:.7}", balance_info["total_balance_xlm"]);
    
    if let Some(balances) = balance_info["balances"].as_array() {
        println!("\n💰 Asset Balances:");
        for balance in balances {
            let asset_code = balance["asset_code"].as_str().unwrap_or("Unknown");
            let balance_amount = balance["balance"].as_str().unwrap_or("0");
            println!("   {}: {}", asset_code, balance_amount);
            
            if let Some(issuer) = balance["asset_issuer"].as_str() {
                println!("     Issuer: {}", issuer);
            }
        }
    }
    
    Ok(())
}

fn get_account_signers(account: &str, network: &str) -> Result<()> {
    InputValidator::validate_stellar_address(account)?;
    InputValidator::validate_network(network)?;
    
    println!("🔑 Getting account signers: {}", account);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let request = AccountManagementRequest {
        action: AccountAction::Signers,
        account_id: Some(account.to_string()),
        wallet_type: None,
        private_key: None,
        mnemonic: None,
        password: None,
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&horizon_client, request)
    )?;
    
    let signers_info: serde_json::Value = result;
    
    println!("✅ Account signers retrieved");
    println!("   Account: {}", signers_info["account_id"]);
    
    if let Some(thresholds) = signers_info["thresholds"].as_object() {
        println!("\n🔒 Thresholds:");
        println!("   Low: {}", thresholds["low"]);
        println!("   Medium: {}", thresholds["medium"]);
        println!("   High: {}", thresholds["high"]);
    }
    
    if let Some(signers) = signers_info["signers"].as_array() {
        println!("\n🔑 Signers:");
        for (i, signer) in signers.iter().enumerate() {
            println!("   {}. {} (weight: {})", i + 1, signer["key"], signer["weight"]);
        }
    }
    
    Ok(())
}

fn fund_account(account: &str, network: &str) -> Result<()> {
    InputValidator::validate_stellar_address(account)?;
    InputValidator::validate_network(network)?;
    
    if network != "testnet" {
        return Err(anyhow::anyhow!("Account funding is only available on testnet"));
    }
    
    println!("💰 Funding testnet account: {}", account);
    
    let config = Config::load_for_network(network)?;
    let horizon_client = HorizonClient::with_config(HorizonClientConfig {
        server_url: config.horizon_url.clone(),
        ..Default::default()
    })?;
    
    let request = AccountManagementRequest {
        action: AccountAction::Fund,
        account_id: Some(account.to_string()),
        wallet_type: None,
        private_key: None,
        mnemonic: None,
        password: None,
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let result = rt.block_on(
        AccountManagementService::manage_accounts(&horizon_client, request)
    )?;
    
    let fund_result: serde_json::Value = result;
    
    println!("✅ Account funding completed");
    println!("   Account: {}", fund_result["account_id"]);
    println!("   Status: {}", fund_result["status"]);
    println!("   Source: {}", fund_result["source"]);
    
    Ok(())
}

fn connect_wallet(wallet_type: &str) -> Result<()> {
    println!("🔗 Connecting wallet: {}", wallet_type);
    
    let wallet_enum = match wallet_type {
        "freighter" => account_management::WalletType::Freighter,
        "albedo" => account_management::WalletType::Albedo,
        "lobstr" => account_management::WalletType::Lobstr,
        "ledger" => account_management::WalletType::HardwareLedger,
        "trezor" => account_management::WalletType::HardwareTrezor,
        _ => return Err(anyhow::anyhow!("Unsupported wallet type: {}", wallet_type)),
    };
    
    let rt = tokio::runtime::Runtime::new()?;
    let wallet_info = rt.block_on(
        AccountManagementService::connect_wallet(wallet_enum)
    )?;
    
    println!("✅ Wallet connection completed");
    println!("   Name: {}", wallet_info.name);
    println!("   Type: {:?}", wallet_info.wallet_type);
    println!("   Connected: {}", wallet_info.is_connected);
    println!("   Accounts: {}", wallet_info.accounts.len());
    
    Ok(())
}

fn validate_address(address: &str) -> Result<()> {
    match InputValidator::validate_stellar_address(address) {
        Ok(_) => {
            println!("✅ Address is valid: {}", address);
            Ok(())
        },
        Err(e) => {
            let formatted_error = ErrorHandler::format_validation_error(&e);
            let suggestion = ErrorHandler::suggest_fix(&e);
            println!("❌ {}", formatted_error);
            println!("💡 {}", suggestion);
            Err(e)
        }
    }
}
