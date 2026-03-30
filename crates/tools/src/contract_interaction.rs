use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::horizon_client::HorizonClient;
use crate::config::Config;
use crate::soroban_tx_builder::{build_soroban_invoke_transaction, json_to_sc_vals};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMethodInfo {
    pub name: String,
    pub inputs: Vec<ContractMethodParam>,
    pub outputs: Vec<ContractMethodParam>,
    pub access: ContractAccess,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMethodParam {
    pub name: String,
    pub type_field: String,
    pub optional: bool,
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ContractAccess {
    Read,
    Write,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractState {
    pub contract_id: String,
    pub instance_data: HashMap<String, serde_json::Value>,
    pub persistent_storage: HashMap<String, serde_json::Value>,
    pub temporary_storage: HashMap<String, serde_json::Value>,
    pub ledger_footprint: LedgerFootprint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerFootprint {
    pub read_only: Vec<String>,
    pub read_write: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ContractQueryRequest {
    pub contract_id: String,
    pub method: String,
    pub args: Option<Vec<serde_json::Value>>,
    pub auth_required: bool,
    pub simulate_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractQueryResponse {
    pub result: serde_json::Value,
    pub success: bool,
    pub error: Option<String>,
    pub gas_used: Option<u64>,
    pub auth_required: bool,
    pub events: Vec<ContractEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEvent {
    pub contract_id: String,
    pub type_field: String,
    pub data: serde_json::Value,
    pub topics: Vec<String>,
}

pub struct ContractInteractionService;

impl ContractInteractionService {
    pub async fn get_contract_info(
        client: &HorizonClient,
        contract_id: &str,
    ) -> Result<Vec<ContractMethodInfo>> {
        // This would typically involve parsing the contract's XDR metadata
        // For now, return placeholder methods based on common StellarAid patterns
        let methods = vec![
            ContractMethodInfo {
                name: "ping".to_string(),
                inputs: vec![],
                outputs: vec![ContractMethodParam {
                    name: "response".to_string(),
                    type_field: "String".to_string(),
                    optional: false,
                    description: Some("Ping response".to_string()),
                }],
                access: ContractAccess::Read,
                description: Some("Health check method".to_string()),
            },
            ContractMethodInfo {
                name: "initialize".to_string(),
                inputs: vec![
                    ContractMethodParam {
                        name: "admin".to_string(),
                        type_field: "Address".to_string(),
                        optional: false,
                        description: Some("Admin account address".to_string()),
                    },
                    ContractMethodParam {
                        name: "platform_fee".to_string(),
                        type_field: "u32".to_string(),
                        optional: false,
                        description: Some("Platform fee in basis points".to_string()),
                    },
                ],
                outputs: vec![],
                access: ContractAccess::Admin,
                description: Some("Initialize contract with admin and fee settings".to_string()),
            },
            ContractMethodInfo {
                name: "create_project".to_string(),
                inputs: vec![
                    ContractMethodParam {
                        name: "title".to_string(),
                        type_field: "String".to_string(),
                        optional: false,
                        description: Some("Project title".to_string()),
                    },
                    ContractMethodParam {
                        name: "description".to_string(),
                        type_field: "String".to_string(),
                        optional: false,
                        description: Some("Project description".to_string()),
                    },
                    ContractMethodParam {
                        name: "goal_amount".to_string(),
                        type_field: "i128".to_string(),
                        optional: false,
                        description: Some("Funding goal amount".to_string()),
                    },
                    ContractMethodParam {
                        name: "deadline".to_string(),
                        type_field: "u64".to_string(),
                        optional: false,
                        description: Some("Project deadline timestamp".to_string()),
                    },
                ],
                outputs: vec![ContractMethodParam {
                    name: "project_id".to_string(),
                    type_field: "u32".to_string(),
                    optional: false,
                    description: Some("Created project ID".to_string()),
                }],
                access: ContractAccess::Write,
                description: Some("Create a new funding project".to_string()),
            },
            ContractMethodInfo {
                name: "donate".to_string(),
                inputs: vec![
                    ContractMethodParam {
                        name: "project_id".to_string(),
                        type_field: "u32".to_string(),
                        optional: false,
                        description: Some("Project ID to donate to".to_string()),
                    },
                    ContractMethodParam {
                        name: "amount".to_string(),
                        type_field: "i128".to_string(),
                        optional: false,
                        description: Some("Donation amount".to_string()),
                    },
                ],
                outputs: vec![],
                access: ContractAccess::Write,
                description: Some("Donate to a project".to_string()),
            },
            ContractMethodInfo {
                name: "get_project".to_string(),
                inputs: vec![
                    ContractMethodParam {
                        name: "project_id".to_string(),
                        type_field: "u32".to_string(),
                        optional: false,
                        description: Some("Project ID to query".to_string()),
                    },
                ],
                outputs: vec![ContractMethodParam {
                    name: "project".to_string(),
                    type_field: "Project".to_string(),
                    optional: false,
                    description: Some("Project information".to_string()),
                }],
                access: ContractAccess::Read,
                description: Some("Get project information".to_string()),
            },
            ContractMethodInfo {
                name: "get_projects".to_string(),
                inputs: vec![],
                outputs: vec![ContractMethodParam {
                    name: "projects".to_string(),
                    type_field: "Vec<Project>".to_string(),
                    optional: false,
                    description: Some("All projects list".to_string()),
                }],
                access: ContractAccess::Read,
                description: Some("Get all projects".to_string()),
            },
            ContractMethodInfo {
                name: "withdraw_funds".to_string(),
                inputs: vec![
                    ContractMethodParam {
                        name: "project_id".to_string(),
                        type_field: "u32".to_string(),
                        optional: false,
                        description: Some("Project ID to withdraw from".to_string()),
                    },
                ],
                outputs: vec![],
                access: ContractAccess::Write,
                description: Some("Withdraw funds for a successful project".to_string()),
            },
        ];
        
        Ok(methods)
    }
    
    pub async fn query_contract(
        client: &HorizonClient,
        config: &Config,
        request: ContractQueryRequest,
    ) -> Result<ContractQueryResponse> {
        // Build the transaction for the contract call
        let tx_request = crate::soroban_tx_builder::BuildSorobanInvokeRequest {
            source: config.admin_key.clone().unwrap_or_default(),
            sequence: "1".to_string(), // Would need to get actual sequence
            contract: request.contract_id.clone(),
            function: request.method.clone(),
            args: request.args.map(|args| serde_json::to_string(&args).ok()).flatten(),
            timeout_seconds: 300,
            base_fee: 100,
            network_passphrase: Some(config.network_passphrase.clone()),
            soroban_data_xdr: None,
        };
        
        let tx_xdr = build_soroban_invoke_transaction(&tx_request)?;
        
        if request.simulate_only {
            // Simulate the transaction without submitting
            let simulation_result = self::simulate_transaction(client, &tx_xdr).await?;
            Ok(simulation_result)
        } else {
            // Submit the actual transaction
            let submission_result = self::submit_transaction(client, &tx_xdr).await?;
            Ok(submission_result)
        }
    }
    
    async fn simulate_transaction(
        client: &HorizonClient,
        tx_xdr: &str,
    ) -> Result<ContractQueryResponse> {
        // This would use the RPC endpoint to simulate the transaction
        // For now, return a placeholder response
        Ok(ContractQueryResponse {
            result: serde_json::Value::String("Simulation successful".to_string()),
            success: true,
            error: None,
            gas_used: Some(1000000),
            auth_required: false,
            events: vec![],
        })
    }
    
    async fn submit_transaction(
        client: &HorizonClient,
        tx_xdr: &str,
    ) -> Result<ContractQueryResponse> {
        // This would submit the transaction via Horizon
        // For now, return a placeholder response
        Ok(ContractQueryResponse {
            result: serde_json::Value::String("Transaction submitted".to_string()),
            success: true,
            error: None,
            gas_used: Some(1500000),
            auth_required: false,
            events: vec![],
        })
    }
    
    pub async fn get_contract_state(
        client: &HorizonClient,
        contract_id: &str,
    ) -> Result<ContractState> {
        // This would query the contract's current state via RPC
        // For now, return placeholder data
        Ok(ContractState {
            contract_id: contract_id.to_string(),
            instance_data: HashMap::new(),
            persistent_storage: HashMap::new(),
            temporary_storage: HashMap::new(),
            ledger_footprint: LedgerFootprint {
                read_only: vec![],
                read_write: vec![],
            },
        })
    }
    
    pub fn generate_method_call_template(
        method: &ContractMethodInfo,
        contract_id: &str,
    ) -> Result<String> {
        let mut template = format!(
            "# Method: {}\n# Contract: {}\n# Access: {:?}\n",
            method.name, contract_id, method.access
        );
        
        if let Some(description) = &method.description {
            template.push_str(&format!("# Description: {}\n", description));
        }
        
        template.push_str("\n# CLI Command:\n");
        template.push_str(&format!(
            "stellaraid-cli invoke --method {} --contract {}",
            method.name, contract_id
        ));
        
        if !method.inputs.is_empty() {
            template.push_str(" --args '");
            let mut args = Vec::new();
            for input in &method.inputs {
                args.push(format!("\"{}\": <{}>", input.name, input.type_field));
            }
            template.push_str(&format!("{{{}}}'", args.join(", ")));
        }
        
        template.push_str("\n\n# Parameters:\n");
        for input in &method.inputs {
            template.push_str(&format!(
                "# {}: {} ({}){}\n",
                input.name,
                input.type_field,
                if input.optional { "optional" } else { "required" },
                if let Some(desc) = &input.description {
                    format!(" - {}", desc)
                } else {
                    String::new()
                }
            ));
        }
        
        if !method.outputs.is_empty() {
            template.push_str("\n# Returns:\n");
            for output in &method.outputs {
                template.push_str(&format!(
                    "# {}: {}{}\n",
                    output.name,
                    output.type_field,
                    if let Some(desc) = &output.description {
                        format!(" - {}", desc)
                    } else {
                        String::new()
                    }
                ));
            }
        }
        
        Ok(template)
    }
    
    pub fn validate_method_args(
        method: &ContractMethodInfo,
        args: &[serde_json::Value],
    ) -> Result<()> {
        if args.len() != method.inputs.len() {
            return Err(anyhow::anyhow!(
                "Argument count mismatch: expected {}, got {}",
                method.inputs.len(),
                args.len()
            ));
        }
        
        for (i, (input, arg)) in method.inputs.iter().zip(args.iter()).enumerate() {
            if arg.is_null() && !input.optional {
                return Err(anyhow::anyhow!(
                    "Required argument '{}' at position {} is null",
                    input.name,
                    i
                ));
            }
            
            // Basic type validation (could be extended)
            match input.type_field.as_str() {
                "String" => {
                    if !arg.is_string() && !arg.is_null() {
                        return Err(anyhow::anyhow!(
                            "Argument '{}' at position {} must be a string",
                            input.name,
                            i
                        ));
                    }
                },
                "u32" | "u64" | "i128" => {
                    if !arg.is_number() && !arg.is_null() {
                        return Err(anyhow::anyhow!(
                            "Argument '{}' at position {} must be a number",
                            input.name,
                            i
                        ));
                    }
                },
                "Address" => {
                    if !arg.is_string() && !arg.is_null() {
                        return Err(anyhow::anyhow!(
                            "Argument '{}' at position {} must be a string (address)",
                            input.name,
                            i
                        ));
                    }
                },
                _ => {
                    // Unknown type - skip validation
                }
            }
        }
        
        Ok(())
    }
    
    pub fn export_contract_methods(
        methods: &[ContractMethodInfo],
        format: ExportFormat,
    ) -> Result<String> {
        match format {
            ExportFormat::Json => {
                serde_json::to_string_pretty(methods).map_err(|e| anyhow::anyhow!("JSON export failed: {}", e))
            },
            ExportFormat::Markdown => {
                let mut markdown = String::new();
                markdown.push_str("# Contract Methods\n\n");
                
                for method in methods {
                    markdown.push_str(&format!("## `{}`\n\n", method.name));
                    
                    if let Some(description) = &method.description {
                        markdown.push_str(&format!("{}\n\n", description));
                    }
                    
                    markdown.push_str(&format!("**Access:** `{:?}`\n\n", method.access));
                    
                    if !method.inputs.is_empty() {
                        markdown.push_str("### Parameters\n\n");
                        markdown.push_str("| Name | Type | Required | Description |\n");
                        markdown.push_str("|------|------|----------|-------------|\n");
                        
                        for input in &method.inputs {
                            markdown.push_str(&format!(
                                "| `{}` | `{}` | {} | {} |\n",
                                input.name,
                                input.type_field,
                                if input.optional { "No" } else { "Yes" },
                                input.description.as_deref().unwrap_or("-")
                            ));
                        }
                        markdown.push_str("\n");
                    }
                    
                    if !method.outputs.is_empty() {
                        markdown.push_str("### Returns\n\n");
                        for output in &method.outputs {
                            markdown.push_str(&format!(
                                "- **`{}`**: `{}` {}\n",
                                output.name,
                                output.type_field,
                                output.description.as_deref().unwrap_or("")
                            ));
                        }
                        markdown.push_str("\n");
                    }
                    
                    markdown.push_str("---\n\n");
                }
                
                Ok(markdown)
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExportFormat {
    Json,
    Markdown,
}
