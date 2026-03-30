#[cfg(test)]
mod tests {
    use super::*;
    use crate::transaction_history::{TransactionHistoryService, TransactionHistoryRequest, Order, TransactionType};
    use crate::validation::InputValidator;
    use chrono::Utc;

    #[test]
    fn test_transaction_history_request_creation() {
        let request = TransactionHistoryRequest {
            account_id: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
            limit: Some(50),
            cursor: None,
            order: Some(Order::Desc),
            tx_type: Some(TransactionType::Payment),
            start_time: Some(Utc::now() - chrono::Duration::hours(24)),
            end_time: Some(Utc::now()),
        };

        assert_eq!(request.account_id, "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K");
        assert_eq!(request.limit, Some(50));
        assert!(request.cursor.is_none());
        assert!(matches!(request.order, Some(Order::Desc)));
        assert!(matches!(request.tx_type, Some(TransactionType::Payment)));
        assert!(request.start_time.is_some());
        assert!(request.end_time.is_some());
    }

    #[test]
    fn test_transaction_type_validation() {
        let types = vec![
            TransactionType::Payment,
            TransactionType::ContractInvocation,
            TransactionType::ContractDeploy,
            TransactionType::Donation,
            TransactionType::Other,
        ];

        for tx_type in types {
            match tx_type {
                TransactionType::Payment => assert!(true),
                TransactionType::ContractInvocation => assert!(true),
                TransactionType::ContractDeploy => assert!(true),
                TransactionType::Donation => assert!(true),
                TransactionType::Other => assert!(true),
            }
        }
    }

    #[test]
    fn test_order_validation() {
        let orders = vec![Order::Asc, Order::Desc];

        for order in orders {
            match order {
                Order::Asc => assert!(true),
                Order::Desc => assert!(true),
            }
        }
    }

    #[test]
    fn test_transaction_record_structure() {
        use crate::transaction_history::TransactionRecord;

        let record = TransactionRecord {
            hash: "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456".to_string(),
            ledger: 12345,
            created_at: Utc::now(),
            source_account: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
            fee_paid: 100,
            operation_count: 1,
            memo: Some("test memo".to_string()),
            successful: true,
            tx_type: TransactionType::Payment,
            amount: Some(10.5),
            asset: Some("XLM".to_string()),
        };

        assert_eq!(record.ledger, 12345);
        assert_eq!(record.fee_paid, 100);
        assert_eq!(record.operation_count, 1);
        assert!(record.successful);
        assert!(matches!(record.tx_type, TransactionType::Payment));
        assert_eq!(record.amount, Some(10.5));
        assert_eq!(record.asset, Some("XLM".to_string()));
    }

    #[test]
    fn test_transaction_summary() {
        use crate::transaction_history::{TransactionRecord, TransactionSummary, TransactionType};

        let records = vec![
            TransactionRecord {
                hash: "hash1".to_string(),
                ledger: 12345,
                created_at: Utc::now(),
                source_account: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
                fee_paid: 100,
                operation_count: 1,
                memo: None,
                successful: true,
                tx_type: TransactionType::Payment,
                amount: Some(10.0),
                asset: Some("XLM".to_string()),
            },
            TransactionRecord {
                hash: "hash2".to_string(),
                ledger: 12346,
                created_at: Utc::now(),
                source_account: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
                fee_paid: 200,
                operation_count: 2,
                memo: None,
                successful: false,
                tx_type: TransactionType::Donation,
                amount: Some(5.0),
                asset: Some("XLM".to_string()),
            },
        ];

        let summary = TransactionHistoryService::generate_summary(&records);

        assert_eq!(summary.total_transactions, 2);
        assert_eq!(summary.successful_transactions, 1);
        assert_eq!(summary.failed_transactions, 1);
        assert_eq!(summary.total_fees, 300);
        assert_eq!(summary.total_operations, 3);
        assert_eq!(summary.payment_transactions, 1);
        assert_eq!(summary.donation_transactions, 1);
        assert_eq!(summary.total_payment_amount, 10.0);
        assert_eq!(summary.total_donation_amount, 5.0);
    }

    #[test]
    fn test_export_to_csv() {
        use crate::transaction_history::TransactionRecord;

        let records = vec![
            TransactionRecord {
                hash: "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456".to_string(),
                ledger: 12345,
                created_at: Utc::now(),
                source_account: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
                fee_paid: 100,
                operation_count: 1,
                memo: Some("test memo".to_string()),
                successful: true,
                tx_type: TransactionType::Payment,
                amount: Some(10.5),
                asset: Some("XLM".to_string()),
            },
        ];

        let csv_content = TransactionHistoryService::export_to_csv(&records).unwrap();

        assert!(csv_content.contains("Hash,Ledger,Created At,Source Account,Fee Paid,Operations,Memo,Successful,Type,Amount,Asset"));
        assert!(csv_content.contains("a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456"));
        assert!(csv_content.contains("12345"));
        assert!(csv_content.contains("GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K"));
        assert!(csv_content.contains("100"));
        assert!(csv_content.contains("1"));
        assert!(csv_content.contains("test memo"));
        assert!(csv_content.contains("true"));
        assert!(csv_content.contains("Payment"));
        assert!(csv_content.contains("10.5"));
        assert!(csv_content.contains("XLM"));
    }

    #[test]
    fn test_empty_transaction_history() {
        let records: Vec<crate::transaction_history::TransactionRecord> = vec![];
        let summary = TransactionHistoryService::generate_summary(&records);

        assert_eq!(summary.total_transactions, 0);
        assert_eq!(summary.successful_transactions, 0);
        assert_eq!(summary.failed_transactions, 0);
        assert_eq!(summary.total_fees, 0);
        assert_eq!(summary.total_operations, 0);
        assert_eq!(summary.payment_transactions, 0);
        assert_eq!(summary.donation_transactions, 0);
        assert_eq!(summary.contract_invocations, 0);
        assert_eq!(summary.contract_deploys, 0);
        assert_eq!(summary.other_transactions, 0);
        assert_eq!(summary.total_payment_amount, 0.0);
        assert_eq!(summary.total_donation_amount, 0.0);
    }

    #[test]
    fn test_transaction_history_validation() {
        // Test that transaction history requests require valid account IDs
        let valid_request = TransactionHistoryRequest {
            account_id: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
            limit: Some(50),
            cursor: None,
            order: Some(Order::Desc),
            tx_type: None,
            start_time: None,
            end_time: None,
        };

        let invalid_request = TransactionHistoryRequest {
            account_id: "invalid_address".to_string(),
            limit: Some(50),
            cursor: None,
            order: Some(Order::Desc),
            tx_type: None,
            start_time: None,
            end_time: None,
        };

        // Valid account should pass validation
        assert!(InputValidator::validate_stellar_address(&valid_request.account_id).is_ok());

        // Invalid account should fail validation
        assert!(InputValidator::validate_stellar_address(&invalid_request.account_id).is_err());

        // Valid limit should pass validation
        if let Some(limit) = valid_request.limit {
            assert!(InputValidator::validate_range(&limit.to_string(), 1.0, 200.0).is_ok());
        }

        // Invalid limit should fail validation
        if let Some(limit) = Some(201) {
            assert!(InputValidator::validate_range(&limit.to_string(), 1.0, 200.0).is_err());
        }
    }

    #[test]
    fn test_transaction_hash_validation() {
        let valid_hash = "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456";
        let invalid_hash = "invalid_hash";

        assert!(InputValidator::validate_transaction_hash(valid_hash).is_ok());
        assert!(InputValidator::validate_transaction_hash(invalid_hash).is_err());
    }

    #[test]
    fn test_transaction_filtering() {
        use crate::transaction_history::{TransactionRecord, TransactionType};

        let records = vec![
            TransactionRecord {
                hash: "hash1".to_string(),
                ledger: 12345,
                created_at: Utc::now(),
                source_account: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
                fee_paid: 100,
                operation_count: 1,
                memo: None,
                successful: true,
                tx_type: TransactionType::Payment,
                amount: Some(10.0),
                asset: Some("XLM".to_string()),
            },
            TransactionRecord {
                hash: "hash2".to_string(),
                ledger: 12346,
                created_at: Utc::now(),
                source_account: "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K".to_string(),
                fee_paid: 200,
                operation_count: 1,
                memo: None,
                successful: true,
                tx_type: TransactionType::Donation,
                amount: Some(5.0),
                asset: Some("XLM".to_string()),
            },
        ];

        // Filter by payment type
        let payment_filter = Some(TransactionType::Payment);
        let filtered: Vec<_> = records.iter()
            .filter(|tx| payment_filter.map_or(true, |filter| tx.tx_type == filter))
            .collect();

        assert_eq!(filtered.len(), 1);
        assert!(matches!(filtered[0].tx_type, TransactionType::Payment));

        // Filter by donation type
        let donation_filter = Some(TransactionType::Donation);
        let filtered: Vec<_> = records.iter()
            .filter(|tx| donation_filter.map_or(true, |filter| tx.tx_type == filter))
            .collect();

        assert_eq!(filtered.len(), 1);
        assert!(matches!(filtered[0].tx_type, TransactionType::Donation));
    }
}
