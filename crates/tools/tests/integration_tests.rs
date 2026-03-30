use stellaraid_tools::*;
use std::process::Command;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_cli_help() {
        let output = Command::new("cargo")
            .args(&["run", "--bin", "stellaraid-cli", "--", "--help"])
            .output()
            .expect("Failed to execute CLI help command");

        assert!(output.status.success());
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("StellarAid CLI tools"));
        assert!(stdout.contains("transaction history"));
        assert!(stdout.contains("batch operations"));
        assert!(stdout.contains("debug utilities"));
        assert!(stdout.contains("contract interaction"));
        assert!(stdout.contains("account management"));
    }

    #[test]
    fn test_transaction_history_command() {
        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "tx-history",
                "--account", "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K",
                "--limit", "10",
                "--network", "testnet"
            ])
            .output()
            .expect("Failed to execute tx-history command");

        // The command should succeed or fail gracefully with validation error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            // Should fail gracefully with validation error, not panic
            assert!(stderr.contains("Invalid") || stderr.contains("network") || stderr.contains("connection"));
        }
    }

    #[test]
    fn test_batch_template_creation() {
        let temp_dir = TempDir::new().unwrap();
        let template_path = temp_dir.path().join("batch_template.csv");

        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "batch", "create-template",
                "--output", template_path.to_str().unwrap(),
                "--operation-type", "payment"
            ])
            .output()
            .expect("Failed to execute batch create-template command");

        assert!(output.status.success());
        
        // Check that template file was created
        assert!(template_path.exists());
        
        let template_content = std::fs::read_to_string(&template_path).unwrap();
        assert!(template_content.contains("payment,destination,amount,asset,issuer"));
    }

    #[test]
    fn test_account_validation() {
        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "account", "validate-address",
                "--address", "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K"
            ])
            .output()
            .expect("Failed to execute account validate-address command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("✅ Address is valid"));
        assert!(output.status.success());
    }

    #[test]
    fn test_account_validation_invalid() {
        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "account", "validate-address",
                "--address", "INVALID_ADDRESS"
            ])
            .output()
            .expect("Failed to execute account validate-address command");

        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("❌ Invalid address"));
        assert!(!output.status.success());
    }

    #[test]
    fn test_contract_info_command() {
        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "contract", "info",
                "--contract", "CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3",
                "--format", "json"
            ])
            .output()
            .expect("Failed to execute contract info command");

        // Should fail gracefully with connection error or validation error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            assert!(stderr.contains("connection") || stderr.contains("network") || stderr.contains("Invalid"));
        }
    }

    #[test]
    fn test_debug_network_status() {
        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "debug", "network-status",
                "--network", "testnet"
            ])
            .output()
            .expect("Failed to execute debug network-status command");

        // Should fail gracefully with connection error or validation error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            assert!(stderr.contains("connection") || stderr.contains("network") || stderr.contains("Invalid"));
        }
    }

    #[test]
    fn test_batch_operations_with_csv() {
        let temp_dir = TempDir::new().unwrap();
        let csv_path = temp_dir.path().join("test_batch.csv");
        
        // Create test CSV file
        let csv_content = r#"payment,GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R,10.5,XLM,
donation,GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K,project_123,5.0,XLM"#;
        
        std::fs::write(&csv_path, csv_content).unwrap();

        let output = Command::new("cargo")
            .args(&[
                "run", "--bin", "stellaraid-cli", "--",
                "batch", "execute",
                "--file", csv_path.to_str().unwrap(),
                "--continue-on-error"
            ])
            .output()
            .expect("Failed to execute batch execute command");

        // Should fail gracefully with connection error or validation error
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() {
            assert!(stderr.contains("connection") || stderr.contains("network") || stderr.contains("Invalid"));
        }
    }

    #[test]
    fn test_all_new_commands_exist() {
        let commands = vec![
            vec!["tx-history", "--help"],
            vec!["batch", "--help"],
            vec!["debug", "--help"],
            vec!["contract", "--help"],
            vec!["account", "--help"],
        ];

        for cmd in commands {
            let mut args = vec!["run", "--bin", "stellaraid-cli", "--"];
            args.extend(&cmd);

            let output = Command::new("cargo")
                .args(&args)
                .output()
                .expect(&format!("Failed to execute command: {:?}", cmd));

            assert!(output.status.success(), "Command {:?} should succeed", cmd);
            
            let stdout = String::from_utf8_lossy(&output.stdout);
            assert!(!stdout.is_empty(), "Command {:?} should produce output", cmd);
        }
    }

    #[test]
    fn test_backward_compatibility() {
        // Test that existing commands still work
        let existing_commands = vec![
            vec!["deploy", "--help"],
            vec!["invoke", "--help"],
            vec!["contract-id", "--help"],
            vec!["config", "--help"],
            vec!["network"],
            vec!["build-donation-tx", "--help"],
            vec!["build-invoke-tx", "--help"],
            vec!["prepare-wallet-signing", "--help"],
            vec!["complete-wallet-signing", "--help"],
            vec!["submit-tx", "--help"],
            vec!["submission-status", "--help"],
            vec!["verify-tx", "--help"],
        ];

        for cmd in existing_commands {
            let mut args = vec!["run", "--bin", "stellaraid-cli", "--"];
            args.extend(&cmd);

            let output = Command::new("cargo")
                .args(&args)
                .output()
                .expect(&format!("Failed to execute existing command: {:?}", cmd));

            assert!(output.status.success(), "Existing command {:?} should still work", cmd);
        }
    }

    #[test]
    fn test_error_handling_and_validation() {
        // Test various invalid inputs to ensure proper error handling
        let test_cases = vec![
            // Invalid addresses
            vec!["account", "validate-address", "--address", "INVALID"],
            vec!["tx-history", "--account", "INVALID"],
            vec!["account", "balance", "--account", "INVALID"],
            
            // Invalid contract IDs
            vec!["contract", "info", "--contract", "INVALID"],
            vec!["contract", "query", "--contract", "INVALID"],
            
            // Invalid networks
            vec!["tx-history", "--account", "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K", "--network", "invalid"],
            vec!["account", "balance", "--account", "GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K", "--network", "invalid"],
        ];

        for cmd in test_cases {
            let mut args = vec!["run", "--bin", "stellaraid-cli", "--"];
            args.extend(&cmd);

            let output = Command::new("cargo")
                .args(&args)
                .output()
                .expect(&format!("Failed to execute validation test command: {:?}", cmd));

            let stderr = String::from_utf8_lossy(&output.stderr);
            
            // Should fail with proper validation error, not panic
            assert!(!output.status.success(), "Command {:?} should fail with invalid input", cmd);
            assert!(stderr.contains("Invalid") || stderr.contains("required") || stderr.contains("error"), 
                   "Command {:?} should show validation error. Stderr: {}", cmd, stderr);
        }
    }

    #[test]
    fn test_help_completeness() {
        let new_commands = vec![
            ("tx-history", vec!["account", "limit", "tx-type", "order", "export-csv", "summary", "network"]),
            ("batch", vec!["execute", "create-template"]),
            ("debug", vec!["collect", "analyze-failure", "network-status"]),
            ("contract", vec!["info", "query", "state", "template"]),
            ("account", vec!["create", "import", "export", "list", "balance", "signers", "fund", "connect-wallet", "validate-address"]),
        ];

        for (command, subcommands) in new_commands {
            let mut args = vec!["run", "--bin", "stellaraid-cli", "--", command, "--help"];
            let output = Command::new("cargo")
                .args(&args)
                .output()
                .expect(&format!("Failed to get help for command: {}", command));

            assert!(output.status.success(), "Help for command {} should succeed", command);
            
            let stdout = String::from_utf8_lossy(&output.stdout);
            
            // Check that subcommands are mentioned in help
            for subcommand in subcommands {
                assert!(stdout.contains(subcommand), 
                       "Help for {} should mention subcommand {}", command, subcommand);
            }
        }
    }
}
