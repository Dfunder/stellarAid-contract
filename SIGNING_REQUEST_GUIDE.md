# Signing Request & Response Handler Documentation

## Overview

The StellarAid signing request and response handler modules provide a complete workflow for building, signing, and processing blockchain transactions through wallet integration.

## Features

### Signing Request Module (`signing_request.rs`)

The signing request module handles the creation of transaction signing requests that can be sent to wallets (like Freighter) for user authorization.

#### Key Components:

- **`SigningRequest`** - Data structure representing a signing request
- **`SigningRequestBuilder`** - Builder pattern for constructing signing requests
- **`TransactionBuilder`** - Helper for building common transaction types

#### Creating Signing Requests

##### Donation Request
```bash
stellaraid-cli signing build-donation <donor_address> <campaign_id> <amount> [asset] [memo]
```

Example:
```bash
stellaraid-cli signing build-donation GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH 1 10000000 XLM "Supporting education"
```

##### Campaign Creation Request
```bash
stellaraid-cli signing build-campaign <creator_address> <title> <goal> <deadline_timestamp>
```

Example:
```bash
stellaraid-cli signing build-campaign GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH "Education Fund" 1000000000 1735689600
```

##### Custom Transaction Request
```bash
stellaraid-cli signing build-custom <xdr> [description]
```

#### Validating Requests
```bash
stellaraid-cli signing validate <json_file>
```

#### Exporting for Wallet
```bash
stellaraid-cli signing export <json_file>
```

### Response Handler Module (`response_handler.rs`)

The response handler module processes signed transactions returned from wallets and prepares them for submission to the Stellar network.

#### Key Components:

- **`SignedTransaction`** - Represents a transaction signed by wallet
- **`TransactionStatus`** - Enum tracking tx state (Signed, Submitted, Confirmed, Failed)
- **`ResponseHandler`** - Main handler for processing responses
- **`ProcessedResponse`** - Result of response processing with validation status

#### Processing Responses

##### Parse Wallet Response
```bash
stellaraid-cli response process <json_response>
```

Example:
```bash
stellaraid-cli response process '{"requestId":"req_123","xdr":"AAAAA...","signer":"GBJCHU...","signedAt":1234567890}'
```

##### Validate Signed Transaction
```bash
stellaraid-cli response validate <json_file>
```

##### Save Signed Transaction
```bash
stellaraid-cli response save <json_response> <output_file>
```

Example:
```bash
stellaraid-cli response save '{"requestId":"req_123",...}' signed_tx.json
```

##### Load Signed Transaction
```bash
stellaraid-cli response load <json_file>
```

##### Submit Signed Transaction
```bash
stellaraid-cli response submit <json_file>
```

## Complete Workflow Example

### 1. Build Signing Request
```bash
# Create a donation signing request
stellaraid-cli signing build-donation \
  GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH \
  1 \
  5000000 \
  XLM \
  "Supporting education initiative"
```

Output shows:
- Request ID
- Network (testnet/mainnet)
- Transaction XDR
- JSON format for wallet submission

### 2. Send to Wallet (Manual step)
- Copy the JSON output
- Send to Freighter or other Stellar wallet
- User reviews and signs transaction
- Wallet returns signed XDR

### 3. Process Response
```bash
# Receive signed transaction from wallet
SIGNED_RESPONSE='{"requestId":"req_123...","xdr":"AAAAA...","signer":"GBJCHU...","signedAt":1234567890}'

# Process and validate
stellaraid-cli response process "$SIGNED_RESPONSE"
```

### 4. Save for Submission
```bash
# Save to file for later submission
stellaraid-cli response save "$SIGNED_RESPONSE" my_signed_tx.json
```

### 5. Submit to Network
```bash
# Submit to Stellar network
stellaraid-cli response submit my_signed_tx.json
```

## Data Structures

### SigningRequest JSON Format
```json
{
  "id": "req_1234567890000",
  "network": "testnet",
  "transaction_xdr": "AAAAAgAAAADDRVZm3Wg...",
  "description": "Donate 5000000 XLM to campaign #1",
  "created_at": 1234567890
}
```

### SignedTransaction JSON Format
```json
{
  "request_id": "req_1234567890000",
  "transaction_xdr": "AAAAAgAAAADDRVZm3Wg...",
  "signed_at": 1234567890,
  "signer": "GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH",
  "status": "signed"
}
```

## API Usage (Rust)

### Building a Signing Request in Code

```rust
use stellaraid_tools::signing_request::{SigningRequestBuilder, TransactionBuilder};

// Create a donation request
let request = TransactionBuilder::build_donation_request(
    "GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH".to_string(),
    1,
    5000000,
    "XLM".to_string(),
    Some("Supporting education".to_string()),
)?;

// Validate
request.validate()?;

// Export for wallet
let wallet_format = request.to_wallet_format()?;
println!("{}", wallet_format);
```

### Processing a Response in Code

```rust
use stellaraid_tools::response_handler::ResponseHandler;

let response_json = r#"{
  "requestId": "req_123",
  "xdr": "AAAAA...",
  "signer": "GBJCHU...",
  "signedAt": 1234567890
}"#;

// Process response
let processed = ResponseHandler::process_response(response_json)?;

// Validate
if processed.is_valid() {
    println!("Transaction ready for submission");
}

// Save to file
ResponseHandler::save_to_file(&processed.signed_transaction, "signed_tx.json")?;
```

## Error Handling

Both modules provide comprehensive error handling:

- Invalid XDR format
- Missing required fields
- Invalid network specification
- Signer validation failures
- File I/O errors

## Security Considerations

1. **Never log private keys or secret signers**
2. **Validate signer addresses** before submission
3. **Store signed transactions securely** before submission
4. **Verify network** (testnet vs mainnet) before submission
5. **Check transaction fees** before user signs
6. **Validate response sources** when integrating with wallets

## Integration with Wallet Connect (Freighter)

For browser-based integration with Freighter wallet:

```javascript
// Build signing request (from Rust backend)
const signingRequest = {
    id: "req_123",
    type: "tx",
    xdr: "AAAAA...",
    network: "testnet",
    description: "Donate to campaign"
};

// Send to Freighter
const signedXdr = await window.freighter.signTransaction(
    signingRequest.xdr,
    signingRequest.network
);

// Return to backend for processing
const response = {
    requestId: signingRequest.id,
    xdr: signedXdr,
    signer: await window.freighter.getAddress(),
    signedAt: Date.now()
};
```

## Testing

Run integration tests:
```bash
cargo test --test integration_test
```

Run unit tests:
```bash
cargo test --lib signing_request
cargo test --lib response_handler
```

## Related Commands

- `stellaraid-cli keypair` - Manage keypairs
- `stellaraid-cli config` - Configure network settings
- `stellaraid-cli network` - Show network info
