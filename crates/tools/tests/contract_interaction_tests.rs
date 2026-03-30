#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract_interaction::{ContractInteractionService, ContractQueryRequest, ExportFormat};
    use crate::validation::InputValidator;

    #[test]
    fn test_validate_contract_id() {
        // Valid contract IDs
        let valid_contracts = vec![
            "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3",
            "CB2EHQKPEWQWKLFRYIRLQYUVJGHZPXFL5FXYE7Y3EFAKQFCENKZQAAAA",
        ];

        for contract in valid_contracts {
            assert!(InputValidator::validate_contract_id(contract).is_ok());
        }

        // Invalid contract IDs
        let invalid_contracts = vec![
            "",
            "GA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3", // Wrong prefix
            "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA", // Too short
            "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3K", // Too long
        ];

        for contract in invalid_contracts {
            assert!(InputValidator::validate_contract_id(contract).is_err());
        }
    }

    #[test]
    fn test_contract_query_request_creation() {
        let request = ContractQueryRequest {
            contract_id: "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3".to_string(),
            method: "get_balance".to_string(),
            args: Some(vec![serde_json::json!("account_id")]),
            auth_required: false,
            simulate_only: true,
        };

        assert_eq!(request.contract_id, "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3");
        assert_eq!(request.method, "get_balance");
        assert!(request.args.is_some());
        assert!(!request.auth_required);
        assert!(request.simulate_only);
    }

    #[test]
    fn test_export_format_validation() {
        // Test that export formats are properly handled
        let json_format = ExportFormat::Json;
        let markdown_format = ExportFormat::Markdown;

        assert!(matches!(json_format, ExportFormat::Json));
        assert!(matches!(markdown_format, ExportFormat::Markdown));
    }

    #[test]
    fn test_contract_method_info() {
        use crate::contract_interaction::{ContractMethodInfo, ContractMethodParam, ContractAccess};

        let method = ContractMethodInfo {
            name: "get_balance".to_string(),
            inputs: vec![
                ContractMethodParam {
                    name: "account_id".to_string(),
                    type_field: "Address".to_string(),
                    optional: false,
                    description: Some("Account address to query".to_string()),
                },
            ],
            outputs: vec![
                ContractMethodParam {
                    name: "balance".to_string(),
                    type_field: "i128".to_string(),
                    optional: false,
                    description: Some("Account balance".to_string()),
                },
            ],
            access: ContractAccess::Read,
            description: Some("Get account balance".to_string()),
        };

        assert_eq!(method.name, "get_balance");
        assert_eq!(method.inputs.len(), 1);
        assert_eq!(method.outputs.len(), 1);
        assert!(matches!(method.access, ContractAccess::Read));
        assert!(method.description.is_some());

        let input_param = &method.inputs[0];
        assert_eq!(input_param.name, "account_id");
        assert_eq!(input_param.type_field, "Address");
        assert!(!input_param.optional);
        assert!(input_param.description.is_some());

        let output_param = &method.outputs[0];
        assert_eq!(output_param.name, "balance");
        assert_eq!(output_param.type_field, "i128");
        assert!(!output_param.optional);
        assert!(output_param.description.is_some());
    }

    #[test]
    fn test_contract_access_types() {
        use crate::contract_interaction::ContractAccess;

        let access_types = vec![
            ContractAccess::Read,
            ContractAccess::Write,
            ContractAccess::Admin,
        ];

        for access in access_types {
            match access {
                ContractAccess::Read => assert!(true),
                ContractAccess::Write => assert!(true),
                ContractAccess::Admin => assert!(true),
            }
        }
    }

    #[test]
    fn test_contract_state_structure() {
        use crate::contract_interaction::{ContractState, LedgerFootprint};
        use std::collections::HashMap;

        let state = ContractState {
            contract_id: "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3".to_string(),
            wasm_hash: "abc123def456".to_string(),
            instance_data: HashMap::new(),
            persistent_storage: HashMap::new(),
            temporary_storage: HashMap::new(),
            ledger_footprint: LedgerFootprint {
                read_only: vec!["key1".to_string()],
                read_write: vec!["key2".to_string()],
            },
        };

        assert_eq!(state.contract_id, "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3");
        assert_eq!(state.wasm_hash, "abc123def456");
        assert_eq!(state.instance_data.len(), 0);
        assert_eq!(state.persistent_storage.len(), 0);
        assert_eq!(state.temporary_storage.len(), 0);
        assert_eq!(state.ledger_footprint.read_only.len(), 1);
        assert_eq!(state.ledger_footprint.read_write.len(), 1);
    }

    #[test]
    fn test_contract_event_structure() {
        use crate::contract_interaction::ContractEvent;

        let event = ContractEvent {
            contract_id: "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3".to_string(),
            type_field: "transfer".to_string(),
            data: serde_json::json!({"from": "account1", "to": "account2", "amount": "100"}),
            topics: vec!["topic1".to_string(), "topic2".to_string()],
        };

        assert_eq!(event.contract_id, "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3");
        assert_eq!(event.type_field, "transfer");
        assert!(event.data.is_object());
        assert_eq!(event.topics.len(), 2);
    }

    #[test]
    fn test_contract_query_response() {
        use crate::contract_interaction::ContractQueryResponse;

        let response = ContractQueryResponse {
            result: serde_json::json!({"balance": 1000}),
            success: true,
            error: None,
            gas_used: Some(50000),
            auth_required: false,
            events: vec![],
        };

        assert!(response.success);
        assert!(response.error.is_none());
        assert_eq!(response.gas_used, Some(50000));
        assert!(!response.auth_required);
        assert_eq!(response.events.len(), 0);
        assert!(response.result.is_object());
    }

    #[test]
    fn test_contract_query_response_with_error() {
        use crate::contract_interaction::ContractQueryResponse;

        let response = ContractQueryResponse {
            result: serde_json::Value::Null,
            success: false,
            error: Some("Insufficient funds".to_string()),
            gas_used: None,
            auth_required: true,
            events: vec![],
        };

        assert!(!response.success);
        assert!(response.error.is_some());
        assert_eq!(response.error, Some("Insufficient funds".to_string()));
        assert_eq!(response.gas_used, None);
        assert!(response.auth_required);
        assert_eq!(response.events.len(), 0);
    }

    #[test]
    fn test_method_validation() {
        use crate::contract_interaction::{ContractMethodInfo, ContractMethodParam, ContractAccess};

        let method = ContractMethodInfo {
            name: "transfer".to_string(),
            inputs: vec![
                ContractMethodParam {
                    name: "to".to_string(),
                    type_field: "Address".to_string(),
                    optional: false,
                    description: None,
                },
                ContractMethodParam {
                    name: "amount".to_string(),
                    type_field: "i128".to_string(),
                    optional: false,
                    description: None,
                },
            ],
            outputs: vec![],
            access: ContractAccess::Write,
            description: None,
        };

        // Test argument validation
        let valid_args = vec![
            serde_json::json!("GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R"),
            serde_json::json!(1000),
        ];

        let invalid_args = vec![
            serde_json::json!("invalid_address"),
            serde_json::json!(1000),
        ];

        // Valid arguments should pass validation
        let validation_result = ContractInteractionService::validate_method_args(&method, &valid_args);
        assert!(validation_result.is_ok());

        // Invalid arguments should fail validation
        let validation_result = ContractInteractionService::validate_method_args(&method, &invalid_args);
        assert!(validation_result.is_err());

        // Wrong number of arguments should fail
        let wrong_count_args = vec![serde_json::json!("GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R")];
        let validation_result = ContractInteractionService::validate_method_args(&method, &wrong_count_args);
        assert!(validation_result.is_err());
    }

    #[test]
    fn test_method_template_generation() {
        use crate::contract_interaction::{ContractMethodInfo, ContractMethodParam, ContractAccess};

        let method = ContractMethodInfo {
            name: "get_balance".to_string(),
            inputs: vec![
                ContractMethodParam {
                    name: "account_id".to_string(),
                    type_field: "Address".to_string(),
                    optional: false,
                    description: Some("Account address to query".to_string()),
                },
            ],
            outputs: vec![
                ContractMethodParam {
                    name: "balance".to_string(),
                    type_field: "i128".to_string(),
                    optional: false,
                    description: Some("Account balance".to_string()),
                },
            ],
            access: ContractAccess::Read,
            description: Some("Get account balance".to_string()),
        };

        let contract_id = "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3";
        
        let template = ContractInteractionService::generate_method_call_template(&method, contract_id).unwrap();
        
        assert!(template.contains("get_balance"));
        assert!(template.contains(contract_id));
        assert!(template.contains("Account address to query"));
        assert!(template.contains("Account balance"));
        assert!(template.contains("stellaraid-cli invoke"));
    }
}
