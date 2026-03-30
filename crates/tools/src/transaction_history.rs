use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::horizon_client::HorizonClient;
use crate::config::Config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRecord {
    pub hash: String,
    pub ledger: u32,
    pub created_at: DateTime<Utc>,
    pub source_account: String,
    pub fee_paid: u32,
    pub operation_count: u32,
    pub memo: Option<String>,
    pub successful: bool,
    pub tx_type: TransactionType,
    pub amount: Option<f64>,
    pub asset: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransactionType {
    Payment,
    ContractInvocation,
    ContractDeploy,
    Donation,
    Other,
}

#[derive(Debug, Clone)]
pub struct TransactionHistoryRequest {
    pub account_id: String,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
    pub order: Option<Order>,
    pub tx_type: Option<TransactionType>,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub enum Order {
    Asc,
    Desc,
}

pub struct TransactionHistoryService;

impl TransactionHistoryService {
    pub async fn get_transaction_history(
        client: &HorizonClient,
        request: TransactionHistoryRequest,
    ) -> Result<Vec<TransactionRecord>> {
        let mut url = format!("/accounts/{}/transactions", request.account_id);
        
        let mut params = Vec::new();
        
        if let Some(limit) = request.limit {
            params.push(format!("limit={}", limit));
        }
        
        if let Some(cursor) = request.cursor {
            params.push(format!("cursor={}", cursor));
        }
        
        if let Some(order) = request.order {
            params.push(format!("order={:?}", order).to_lowercase());
        }
        
        if !params.is_empty() {
            url.push('?');
            url.push_str(&params.join("&"));
        }
        
        let response = client.get(&url).await?;
        let transactions: Vec<stellar_baselib::horizon::Transaction> = 
            serde_json::from_str(&response)?;
        
        let mut records = Vec::new();
        
        for tx in transactions {
            let record = self::parse_transaction(tx)?;
            if let Some(filter_type) = &request.tx_type {
                if record.tx_type != *filter_type {
                    continue;
                }
            }
            
            if let Some(start) = request.start_time {
                if record.created_at < start {
                    continue;
                }
            }
            
            if let Some(end) = request.end_time {
                if record.created_at > end {
                    continue;
                }
            }
            
            records.push(record);
        }
        
        Ok(records)
    }
    
    pub async fn get_transaction_details(
        client: &HorizonClient,
        tx_hash: &str,
    ) -> Result<TransactionRecord> {
        let url = format!("/transactions/{}", tx_hash);
        let response = client.get(&url).await?;
        let transaction: stellar_baselib::horizon::Transaction = 
            serde_json::from_str(&response)?;
        
        parse_transaction(transaction)
    }
    
    pub fn export_to_csv(records: &[TransactionRecord]) -> Result<String> {
        let mut csv = String::new();
        csv.push_str("Hash,Ledger,Created At,Source Account,Fee Paid,Operations,Memo,Successful,Type,Amount,Asset\n");
        
        for record in records {
            csv.push_str(&format!(
                "{},{},{},{},{},{},{},{},{:?},{},{}\n",
                record.hash,
                record.ledger,
                record.created_at.format("%Y-%m-%d %H:%M:%S UTC"),
                record.source_account,
                record.fee_paid,
                record.operation_count,
                record.memo.as_deref().unwrap_or(""),
                record.successful,
                record.tx_type,
                record.amount.unwrap_or(0.0),
                record.asset.as_deref().unwrap_or("")
            ));
        }
        
        Ok(csv)
    }
    
    pub fn generate_summary(records: &[TransactionRecord]) -> TransactionSummary {
        let mut summary = TransactionSummary::default();
        
        for record in records {
            summary.total_transactions += 1;
            summary.total_fees += record.fee_paid as u64;
            summary.total_operations += record.operation_count as u64;
            
            if record.successful {
                summary.successful_transactions += 1;
            } else {
                summary.failed_transactions += 1;
            }
            
            match record.tx_type {
                TransactionType::Payment => {
                    summary.payment_transactions += 1;
                    if let Some(amount) = record.amount {
                        summary.total_payment_amount += amount;
                    }
                },
                TransactionType::Donation => {
                    summary.donation_transactions += 1;
                    if let Some(amount) = record.amount {
                        summary.total_donation_amount += amount;
                    }
                },
                TransactionType::ContractInvocation => {
                    summary.contract_invocations += 1;
                },
                TransactionType::ContractDeploy => {
                    summary.contract_deploys += 1;
                },
                TransactionType::Other => {
                    summary.other_transactions += 1;
                },
            }
        }
        
        summary
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TransactionSummary {
    pub total_transactions: u32,
    pub successful_transactions: u32,
    pub failed_transactions: u32,
    pub total_fees: u64,
    pub total_operations: u64,
    pub payment_transactions: u32,
    pub donation_transactions: u32,
    pub contract_invocations: u32,
    pub contract_deploys: u32,
    pub other_transactions: u32,
    pub total_payment_amount: f64,
    pub total_donation_amount: f64,
}

fn parse_transaction(tx: stellar_baselib::horizon::Transaction) -> Result<TransactionRecord> {
    let tx_type = determine_transaction_type(&tx)?;
    let (amount, asset) = extract_amount_and_asset(&tx)?;
    
    Ok(TransactionRecord {
        hash: tx.hash,
        ledger: tx.ledger,
        created_at: tx.created_at.parse()?,
        source_account: tx.source_account,
        fee_paid: tx.fee_paid,
        operation_count: tx.operation_count,
        memo: tx.memo.as_ref().and_then(|m| match m {
            stellar_baselib::horizon::Memo::Text(text) => Some(text.clone()),
            stellar_baselib::horizon::Memo::Id(id) => Some(id.to_string()),
            stellar_baselib::horizon::Memo::Hash(hash) => Some(hex::encode(hash)),
            stellar_baselib::horizon::Memo::Return(return_hash) => Some(hex::encode(return_hash)),
            stellar_baselib::horizon::Memo::None => None,
        }),
        successful: tx.successful,
        tx_type,
        amount,
        asset,
    })
}

fn determine_transaction_type(tx: &stellar_baselib::horizon::Transaction) -> Result<TransactionType> {
    if let Some(operation) = tx.operations.first() {
        match operation.type_field.as_str() {
            "payment" => {
                if let Some(memo) = &tx.memo {
                    if let stellar_baselib::horizon::Memo::Text(text) = memo {
                        if text.starts_with("project_") {
                            return Ok(TransactionType::Donation);
                        }
                    }
                }
                Ok(TransactionType::Payment)
            },
            "invoke_contract_function" => Ok(TransactionType::ContractInvocation),
            "create_contract" => Ok(TransactionType::ContractDeploy),
            _ => Ok(TransactionType::Other),
        }
    } else {
        Ok(TransactionType::Other)
    }
}

fn extract_amount_and_asset(tx: &stellar_baselib::horizon::Transaction) -> Result<(Option<f64>, Option<String>)> {
    if let Some(operation) = tx.operations.first() {
        if operation.type_field == "payment" {
            let amount = operation.amount.parse::<f64>().ok();
            let asset = if operation.asset_type == "native" {
                Some("XLM".to_string())
            } else {
                Some(format!("{}:{}", operation.asset_code, operation.asset_issuer))
            };
            Ok((amount, asset))
        } else {
            Ok((None, None))
        }
    } else {
        Ok((None, None))
    }
}
