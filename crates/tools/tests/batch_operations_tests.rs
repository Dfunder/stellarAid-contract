#[cfg(test)]
mod tests {
    use super::*;
    use crate::batch_operations::{BatchOperationService, BatchOperation, BatchOperationType, BatchOperationStatus};
    use std::collections::HashMap;

    #[test]
    fn test_create_batch_from_csv() {
        let csv_content = r#"payment,GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R,10.5,XLM,
donation,GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K,project_123,5.0,XLM
invoke,CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3,get_balance,[],"#;

        let batch_request = BatchOperationService::create_batch_from_csv(csv_content).unwrap();
        
        assert_eq!(batch_request.operations.len(), 3);
        assert_eq!(batch_request.parallel, false);
        assert_eq!(batch_request.continue_on_error, true);
        assert_eq!(batch_request.max_concurrent, None);

        // Check first operation (payment)
        let payment_op = &batch_request.operations[0];
        assert!(matches!(payment_op.operation_type, BatchOperationType::Payment));
        assert_eq!(payment_op.status, BatchOperationStatus::Pending);
        assert_eq!(payment_op.parameters.get("param_1"), Some(&"GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R".to_string()));

        // Check second operation (donation)
        let donation_op = &batch_request.operations[1];
        assert!(matches!(donation_op.operation_type, BatchOperationType::Donation));
        assert_eq!(donation_op.status, BatchOperationStatus::Pending);

        // Check third operation (invoke)
        let invoke_op = &batch_request.operations[2];
        assert!(matches!(invoke_op.operation_type, BatchOperationType::ContractInvocation));
        assert_eq!(invoke_op.status, BatchOperationStatus::Pending);
    }

    #[test]
    fn test_create_batch_from_csv_invalid_operation_type() {
        let csv_content = r#"invalid,GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R,10.5,XLM,"#;

        let result = BatchOperationService::create_batch_from_csv(csv_content);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid operation type"));
    }

    #[test]
    fn test_export_batch_results() {
        let batch_result = crate::batch_operations::BatchResult {
            batch_id: "test-batch-123".to_string(),
            total_operations: 2,
            successful_operations: 1,
            failed_operations: 1,
            operations: vec![
                BatchOperation {
                    id: "op_1".to_string(),
                    operation_type: BatchOperationType::Payment,
                    parameters: HashMap::new(),
                    status: BatchOperationStatus::Completed,
                    error: None,
                },
                BatchOperation {
                    id: "op_2".to_string(),
                    operation_type: BatchOperationType::Donation,
                    parameters: HashMap::new(),
                    status: BatchOperationStatus::Failed,
                    error: Some("Insufficient funds".to_string()),
                },
            ],
            execution_time_ms: 1500,
        };

        let csv_content = BatchOperationService::export_batch_results(&batch_result).unwrap();
        
        assert!(csv_content.contains("Batch ID,Operation ID,Type,Status,Error"));
        assert!(csv_content.contains("test-batch-123"));
        assert!(csv_content.contains("op_1"));
        assert!(csv_content.contains("op_2"));
        assert!(csv_content.contains("Completed"));
        assert!(csv_content.contains("Failed"));
        assert!(csv_content.contains("Insufficient funds"));
    }

    #[test]
    fn test_batch_operation_types() {
        // Test that all operation types can be created and compared
        let payment_op = BatchOperationType::Payment;
        let donation_op = BatchOperationType::Donation;
        let invoke_op = BatchOperationType::ContractInvocation;
        let deploy_op = BatchOperationType::ContractDeploy;

        assert!(matches!(payment_op, BatchOperationType::Payment));
        assert!(matches!(donation_op, BatchOperationType::Donation));
        assert!(matches!(invoke_op, BatchOperationType::ContractInvocation));
        assert!(matches!(deploy_op, BatchOperationType::ContractDeploy));
    }

    #[test]
    fn test_batch_operation_status() {
        let status = BatchOperationStatus::Pending;
        assert!(matches!(status, BatchOperationStatus::Pending));

        let completed = BatchOperationStatus::Completed;
        assert!(matches!(completed, BatchOperationStatus::Completed));

        let failed = BatchOperationStatus::Failed;
        assert!(matches!(failed, BatchOperationStatus::Failed));

        let in_progress = BatchOperationStatus::InProgress;
        assert!(matches!(in_progress, BatchOperationStatus::InProgress));
    }

    #[test]
    fn test_batch_operation_creation() {
        let mut parameters = HashMap::new();
        parameters.insert("destination".to_string(), "GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R".to_string());
        parameters.insert("amount".to_string(), "10.5".to_string());

        let operation = BatchOperation {
            id: "test_op".to_string(),
            operation_type: BatchOperationType::Payment,
            parameters: parameters.clone(),
            status: BatchOperationStatus::Pending,
            error: None,
        };

        assert_eq!(operation.id, "test_op");
        assert!(matches!(operation.operation_type, BatchOperationType::Payment));
        assert_eq!(operation.parameters.get("destination"), Some(&"GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R".to_string()));
        assert_eq!(operation.parameters.get("amount"), Some(&"10.5".to_string()));
        assert!(matches!(operation.status, BatchOperationStatus::Pending));
        assert!(operation.error.is_none());
    }
}
