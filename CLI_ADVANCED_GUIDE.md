# StellarAid CLI Advanced Features Guide

This guide covers the advanced CLI features added to StellarAid for transaction history tracking, batch operations, debugging, contract interaction, and account management.

## Table of Contents

- [Transaction History](#transaction-history)
- [Batch Operations](#batch-operations)
- [Debugging Utilities](#debugging-utilities)
- [Contract Interaction](#contract-interaction)
- [Account Management](#account-management)
- [Error Handling and Validation](#error-handling-and-validation)

## Transaction History

### Get Transaction History for an Account

```bash
# Basic transaction history
stellaraid-cli tx-history --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K

# With filtering and export
stellaraid-cli tx-history \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --limit 100 \
  --tx-type payment \
  --order desc \
  --export-csv transactions.csv \
  --summary \
  --network testnet
```

#### Options

- `--account`: Account ID to query (required)
- `--limit`: Number of transactions to fetch (max 200, default 50)
- `--tx-type`: Filter by transaction type (`payment`, `donation`, `contract`, `deploy`)
- `--order`: Sort order (`asc` or `desc`, default `desc`)
- `--export-csv`: Export results to CSV file
- `--summary`: Show transaction summary statistics
- `--network`: Network to query (`testnet`, `mainnet`, default `testnet`)

#### Example Output

```
🔍 Fetching transaction history for account: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
📊 Found 25 transactions

📈 Transaction Summary:
   Total: 25
   Successful: 23
   Failed: 2
   Total Fees: 2500 stroops
   Payments: 15 (150.0000000 XLM)
   Donations: 8 (45.5000000 XLM)
   Contract Invocations: 2
   Contract Deploys: 0

📋 Recent Transactions:

1. a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456
   Type: Payment
   Status: ✅ Success
   Ledger: 123456
   Fee: 100 stroops
   Operations: 1
   Date: 2024-03-30 12:00:00 UTC
   Memo: Monthly donation
   Amount: 10.0000000 XLM
```

## Batch Operations

### Create Batch Template

```bash
# Create payment batch template
stellaraid-cli batch create-template --output payments.csv --operation-type payment

# Create donation batch template
stellaraid-cli batch create-template --output donations.csv --operation-type donation

# Create contract invocation template
stellaraid-cli batch create-template --output invokes.csv --operation-type invoke
```

#### Template Examples

**Payment Template (payments.csv):**
```csv
payment,destination,amount,asset,issuer
payment,GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R,10.5,XLM,
payment,GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K,5.0,USDC,GA5ZSEJYB37JRC5VMCIYLL2D7ZXLX7N5JMN6H6YWEY4KVYFQHE4T6LM7
```

**Donation Template (donations.csv):**
```csv
donation,donor,project_id,amount,asset
donation,GD5JD3BU6Y7WHOWBTTPKDUL5RBXM3DF6K5MV5RH2LJQEBL74HPUTYW3R,project_123,10.0,XLM
donation,GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K,project_456,5.0,XLM
```

### Execute Batch Operations

```bash
# Execute batch sequentially
stellaraid-cli batch execute \
  --file payments.csv \
  --continue-on-error \
  --export-results results.csv

# Execute batch in parallel
stellaraid-cli batch execute \
  --file payments.csv \
  --parallel \
  --max-concurrent 5 \
  --continue-on-error \
  --export-results results.csv
```

#### Options

- `--file`: CSV file with batch operations (required)
- `--parallel`: Execute operations in parallel
- `--continue-on-error`: Continue processing even if some operations fail
- `--max-concurrent`: Maximum concurrent operations (only with `--parallel`)
- `--export-results`: Export results to CSV file

#### Example Output

```
🔧 Executing batch operations from: payments.csv
📊 Found 10 operations
✅ Batch execution completed
   Batch ID: 550e8400-e29b-41d4-a716-446655440000
   Total Operations: 10
   Successful: 8
   Failed: 2
   Execution Time: 15420ms
📁 Exported results to: results.csv
```

## Debugging Utilities

### Collect Debug Information

```bash
# Basic debug info collection
stellaraid-cli debug collect \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --network testnet

# With contract information and export
stellaraid-cli debug collect \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --export debug_report.json \
  --network testnet
```

#### Example Output

```
🔍 Collecting debug information...
✅ Debug information collected
   Network: testnet
   Latest Ledger: 456789
   Horizon Status: Healthy
   Response Time: 245ms

💰 Account Information:
   Account: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
   Sequence: 123456789
   Balance: 1000.5 XLM
   Signers: 1

📈 Performance Metrics:
   RPC Response Time: 245ms
   Horizon Response Time: 198ms
   Memory Usage: 128.0MB
```

### Analyze Transaction Failure

```bash
stellaraid-cli debug analyze-failure \
  --tx-hash a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456 \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --network testnet
```

### Check Network Status

```bash
# Basic network status
stellaraid-cli debug network-status --network testnet

# Detailed network metrics
stellaraid-cli debug network-status --network testnet --detailed
```

#### Example Output

```
🌐 Checking network status: testnet
✅ Network Status: Healthy
   Response Time: 198ms

📊 Detailed Metrics:
   Base Fee: 100 stroops
   Base Reserve: 5000000 stroops
   Recommended Fee: 100 stroops

💰 Fee Distribution (percentiles):
   p10: 100 stroops
   p25: 100 stroops
   p50: 100 stroops
   p75: 100 stroops
   p90: 100 stroops
   p95: 100 stroops
   p99: 100 stroops
```

## Contract Interaction

### Get Contract Information

```bash
# Get contract methods in JSON format
stellaraid-cli contract info \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --format json

# Export contract documentation to Markdown
stellaraid-cli contract info \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --format markdown \
  --output contract_docs.md
```

### Query Contract Methods

```bash
# Simulate contract method call
stellaraid-cli contract query \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --method get_balance \
  --args '["GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K"]' \
  --simulate \
  --network testnet

# Submit actual contract call
stellaraid-cli contract query \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --method donate \
  --args '["project_123", "10000000"]' \
  --network testnet
```

### Get Contract State

```bash
stellaraid-cli contract state \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --export contract_state.json \
  --network testnet
```

### Generate Method Call Template

```bash
stellaraid-cli contract template \
  --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 \
  --method donate \
  --output donate_template.md
```

#### Example Template Output

```markdown
# Method: donate
# Contract: CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3
# Access: Write

# CLI Command:
stellaraid-cli invoke --method donate --contract CA3D5KRYM6CB7OWQ6TWYJ3HZQG2X5MFOWFGY6J5GQYQQRX2JR2V7CA3 --args '{"project_id": <u32>, "amount": <i128>}'

# Parameters:
# project_id: u32 (required) - Project ID to donate to
# amount: i128 (required) - Donation amount

# Returns:
# result: () - Donate to a project
```

## Account Management

### Create New Account

```bash
# Create account with mnemonic
stellaraid-cli account create --generate-mnemonic

# Create account and save to secure vault
stellaraid-cli account create --save --password your_secure_password --generate-mnemonic
```

#### Example Output

```
🔐 Creating new account...
✅ Account created successfully
   Account ID: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
   Public Key: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
   Mnemonic: abandon ability able about above absent absorb abstract absurd abuse access accident account accuse achieve acid acoustic acquire across act
   ⚠️  Save this mnemonic phrase securely!
   Secret Key: SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
   ⚠️  Save this secret key securely!
```

### Import Existing Account

```bash
# Import from private key
stellaraid-cli account import --private-key "SABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K"

# Import from mnemonic and save to vault
stellaraid-cli account import \
  --mnemonic "abandon ability able about above absent absorb abstract absurd abuse access accident account accuse" \
  --save \
  --password your_secure_password
```

### Export Account

```bash
stellaraid-cli account export \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --password your_secure_password \
  --format json
```

### List Accounts

```bash
# Basic account list
stellaraid-cli account list

# Detailed account list
stellaraid-cli account list --detailed
```

### Get Account Balance

```bash
stellaraid-cli account balance \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --network testnet
```

#### Example Output

```
💰 Getting account balance: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
✅ Account balance retrieved
   Account: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
   Sequence: 123456789
   Total XLM Balance: 1000.5000000

💰 Asset Balances:
   XLM: 1000.5000000
   USDC: 500.000000
     Issuer: GA5ZSEJYB37JRC5VMCIYLL2D7ZXLX7N5JMN6H6YWEY4KVYFQHE4T6LM7
```

### Get Account Signers

```bash
stellaraid-cli account signers \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --network testnet
```

### Fund Testnet Account

```bash
stellaraid-cli account fund \
  --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K \
  --network testnet
```

### Connect Wallet

```bash
# Connect to Freighter
stellaraid-cli account connect-wallet --wallet-type freighter

# Connect to Ledger
stellaraid-cli account connect-wallet --wallet-type ledger
```

### Validate Address

```bash
stellaraid-cli account validate-address --address GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
```

#### Example Output

```
✅ Address is valid: GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K
```

## Error Handling and Validation

The CLI provides comprehensive input validation and helpful error messages:

### Validation Examples

```bash
# Invalid address
stellaraid-cli account validate-address --address INVALID_ADDRESS
# Output: ❌ Invalid address: INVALID_ADDRESS
#         💡 Stellar addresses start with 'G' and are 56 characters long

# Invalid amount
stellaraid-cli tx-history --account GABJ2Z7Q4F64EYDQ3JX2PTNZWRZQZKBY3NHOVPJQDE4ZXW2Q6L7LYY6K --limit 0
# Output: ❌ Invalid amount: 0 (must be between 1 and 200)

# Invalid contract ID
stellaraid-cli contract info --contract INVALID_CONTRACT
# Output: ❌ Invalid contract ID: INVALID_CONTRACT
```

### Common Error Types

- **Invalid Address**: Stellar addresses must start with 'G' and be 56 characters
- **Invalid Amount**: Amounts must be positive numbers with up to 7 decimal places
- **Invalid Contract ID**: Contract IDs must start with 'C' and be 56 characters
- **Invalid Transaction Hash**: Hashes must be 64-character hexadecimal strings
- **Invalid Network**: Supported networks are testnet, mainnet, sandbox, public, future
- **Missing Required Fields**: All required parameters must be provided
- **Network Errors**: Connection issues, timeouts, or service unavailability

### Best Practices

1. **Always validate addresses** before using them in transactions
2. **Use testnet for development** before moving to mainnet
3. **Export transaction history** regularly for record keeping
4. **Use batch operations** for multiple similar transactions
5. **Secure private keys and mnemonics** properly
6. **Check network status** before performing critical operations
7. **Use simulation mode** for contract calls before actual submission

## Integration with Existing Workflow

The new CLI features are fully backward compatible with existing commands:

```bash
# Existing commands still work
stellaraid-cli deploy --network testnet
stellaraid-cli invoke --method ping
stellaraid-cli build-donation-tx --donor GABJ2... --amount 10.5

# New commands can be combined with existing ones
stellaraid-cli account create --generate-mnemonic
stellaraid-cli account fund --account GABJ2... --network testnet
stellaraid-cli tx-history --account GABJ2... --summary
```

## Troubleshooting

### Common Issues

1. **Connection Errors**: Check network connectivity and Horizon status
2. **Validation Errors**: Verify input formats and required parameters
3. **Permission Errors**: Ensure proper file permissions for export operations
4. **Memory Issues**: Use pagination for large transaction histories

### Getting Help

```bash
# Get general help
stellaraid-cli --help

# Get help for specific command
stellaraid-cli tx-history --help
stellaraid-cli batch --help
stellaraid-cli debug --help
stellaraid-cli contract --help
stellaraid-cli account --help
```

For more detailed troubleshooting, use the debug utilities:

```bash
stellaraid-cli debug collect --account YOUR_ACCOUNT --network testnet
stellaraid-cli debug network-status --network testnet --detailed
```
