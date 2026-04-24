# Signing Request Flow - Step-by-Step Implementation Guide

## Overview

This guide shows how to implement the complete signing request and response handling workflow in a real StellarAid application.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│  User Interface (Web/Mobile)                            │
│  - Display campaign details                             │
│  - Show donation form                                   │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Backend (Rust CLI / API)                               │
│  - stellaraid-cli signing build-donation                │
│  - SigningRequestBuilder                                │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Format for Transmission                                │
│  - JSON serialization                                   │
│  - Wallet-compatible format                             │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Wallet (Freighter / Albedo / etc.)                     │
│  - User reviews transaction                             │
│  - User signs with private key                          │
│  - Return signed XDR                                    │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Response Handler                                       │
│  - ResponseHandler::process_response()                  │
│  - Validate signed transaction                          │
│  - Check signer and integrity                           │
└──────────────────┬──────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────────────────────┐
│  Storage & Submission                                   │
│  - Save signed transaction to file                      │
│  - Submit to Stellar network                            │
│  - Store confirmation                                   │
└─────────────────────────────────────────────────────────┘
```

## Step 1: Build Signing Request

### In Backend (Rust)
```rust
use stellaraid_tools::signing_request::TransactionBuilder;

let request = TransactionBuilder::build_donation_request(
    "GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH".to_string(),
    1,  // campaign ID
    5000000,  // amount in stroops
    "XLM".to_string(),
    Some("Supporting education".to_string()),
)?;

// Validate request
request.validate()?;

// Export as JSON
let json = request.to_json()?;
println!("{}", json);
```

### CLI Command
```bash
stellaraid-cli signing build-donation \
  GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH \
  1 \
  5000000 \
  XLM \
  "Supporting education"
```

### Output
```json
{
  "id": "req_1713969841234",
  "network": "testnet",
  "transaction_xdr": "AAAAAgAAAADDRVZm3Wgf40kMCwbWI6txY5T7PX0J8p5hJF3J+VBDAAAAAAAAA==",
  "description": "Donate 5000000 XLM to campaign #1 [memo: Supporting education]",
  "created_at": 1713969841
}
```

## Step 2: Send to Wallet

### Browser Integration (Freighter)
```javascript
// Get signing request from backend
const signingRequest = await fetch('/api/signing-request/build-donation', {
    method: 'POST',
    body: JSON.stringify({
        campaign_id: 1,
        amount: 5000000,
        asset: 'XLM'
    })
}).then(r => r.json());

// Convert to wallet format
const walletRequest = JSON.parse(signingRequest.to_wallet_format());

// Send to Freighter
try {
    const signedXdr = await window.freighter.signTransaction(
        walletRequest.xdr,
        walletRequest.network
    );
    
    const response = {
        requestId: walletRequest.id,
        xdr: signedXdr,
        signer: await window.freighter.getAddress(),
        signedAt: Date.now()
    };
    
    // Send back to backend
    await submitSignedTransaction(response);
} catch (error) {
    console.error('User denied signing:', error);
}
```

## Step 3: Process Response

### Backend (Rust)
```rust
use stellaraid_tools::response_handler::ResponseHandler;

let response_json = r#"{
  "requestId": "req_1713969841234",
  "xdr": "AAAAAgAAAADDRVZm3Wgf40kMCwbWI6txY5T7PX0J8p5hJF3J+VBDAAAAAAAAAAwAAABg==",
  "signer": "GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH",
  "signedAt": 1713969850
}"#;

// Process response
let processed = ResponseHandler::process_response(response_json)?;

// Validate
if !processed.is_valid() {
    eprintln!("Validation errors: {:?}", processed.validation_errors);
    return Err(anyhow!("Response validation failed"));
}

println!("✅ Transaction signed by: {}", processed.signed_transaction.signer);
```

### CLI Command
```bash
RESPONSE='{"requestId":"req_1713969841234","xdr":"AAAA...","signer":"GBJCHU...","signedAt":1713969850}'
stellaraid-cli response process "$RESPONSE"
```

### Output
```
✅ Transaction Signed Successfully
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Request ID:    req_1713969841234
Signer:        GBJCHUKZMTFSLOMNC2P4TS4VJJBTCYL3SDKW3KSMSGQUZ6EFLXVX77JVH
Status:        Signed
Signed At:     1713969850

Ready for submission
```

## Step 4: Save for Later or Immediate Submission

### Save to File
```rust
use stellaraid_tools::response_handler::ResponseHandler;

// Save signed transaction
ResponseHandler::save_to_file(
    &processed.signed_transaction,
    "donations/req_1713969841234.json"
)?;

println!("Transaction saved for later submission");
```

### CLI Command
```bash
stellaraid-cli response save "$RESPONSE" "donations/donation_signed.json"
```

## Step 5: Submit to Network

### Backend (Rust) - Future Implementation
```rust
use stellaraid_tools::response_handler::ResponseHandler;

// Load signed transaction
let signed_tx = ResponseHandler::load_from_file("donations/donation_signed.json")?;

// Submit to Stellar network
let submission_result = submit_to_network(&signed_tx.transaction_xdr)?;

println!("✅ Transaction submitted!");
println!("Hash: {}", submission_result.transaction_hash);
println!("Ledger: {}", submission_result.ledger);

// Update database with submission
database::update_donation_status(
    signed_tx.request_id,
    submission_result.transaction_hash
)?;
```

### CLI Command
```bash
stellaraid-cli response submit "donations/donation_signed.json"
```

## Error Handling

### Example: Invalid Signer

```rust
let response_json = r#"{
  "requestId": "req_1713969841234",
  "xdr": "AAAA...",
  "signer": "INVALID_ADDRESS",
  "signedAt": 1713969850
}"#;

match ResponseHandler::process_response(response_json) {
    Ok(processed) => {
        if processed.is_valid() {
            // Submit transaction
        }
    }
    Err(e) => {
        eprintln!("❌ Response processing failed: {}", e);
        // Invalid signer, reject transaction
    }
}
```

### Example: Mismatched Network

```bash
# Built for testnet
stellaraid-cli signing build-donation GBJCHU... 1 5000000 XLM

# But signing response came from mainnet
# Error: Network mismatch - request expects 'testnet', response is 'mainnet'
```

## Sequence Diagram

```
User                Wallet              Backend            Stellar Network
 │                   │                    │                     │
 │─ Click "Donate" ─→ │                    │                     │
 │                    │← Build Request ────│                     │
 │                    │                    │                     │
 │ Review & Sign ────→ │                    │                     │
 │                    │─ Submit Response ─→ │                     │
 │                    │                    │                     │
 │                    │                    │─ Validate ─────────→ │
 │                    │                    │←─ Fee Check ────────│
 │                    │                    │                     │
 │                    │ Display Confirm ←──│─ Success  ─────────│
 │                    │                    │←─ Confirmation ───│
 │
 │─────────────────── Done ─────────────────────────────────────→
```

## Testing the Workflow

### Unit Test
```rust
#[test]
fn test_complete_signing_workflow() {
    // Build request
    let request = TransactionBuilder::build_donation_request(
        "GBJCHU...".to_string(),
        1,
        5000000,
        "XLM".to_string(),
        None,
    ).unwrap();
    
    // Simulate wallet response
    let response = format!(r#"{{
        "requestId": "{}",
        "xdr": "AAAA...",
        "signer": "GBJCHU...",
        "signedAt": {}
    }}"#, request.id, chrono::Local::now().timestamp());
    
    // Process response
    let processed = ResponseHandler::process_response(&response).unwrap();
    assert!(processed.is_valid());
    assert_eq!(processed.signed_transaction.request_id, request.id);
}
```

### Integration Test Example (from tests/integration_test.rs)
```bash
cargo test --test integration_test -- --nocapture
```

## Security Best Practices

1. **Validate Network**
   ```rust
   if tx.network != expected_network {
       return Err(anyhow!("Network mismatch"));
   }
   ```

2. **Verify Signer**
   ```rust
   if tx.signer != expected_signer {
       return Err(anyhow!("Unauthorized signer"));
   }
   ```

3. **Check Timestamps**
   ```rust
   let age_seconds = now - tx.signed_at;
   if age_seconds > 3600 {  // 1 hour expiry
       return Err(anyhow!("Signature expired"));
   }
   ```

4. **Use HTTPS**
   - Always transmit signing requests and responses over HTTPS
   - Use secure WebSocket (WSS) for real-time updates

5. **Rate Limiting**
   - Implement rate limiting on signing request endpoints
   - Prevent replay attacks with request IDs

## Monitoring & Logging

```rust
// Log signing request creation
info!("Built signing request: {:?} for campaign {}", 
      request.id, campaign_id);

// Log response processing
info!("Processing response {} signed by {}", 
      response.requestId, response.signer);

// Log submission
info!("Submitted transaction {} to network", 
      submission_result.transaction_hash);
```

## Troubleshooting

| Issue | Cause | Solution |
|-------|-------|----------|
| "Invalid XDR" | Malformed transaction data | Verify transaction structure with Stellar SDK |
| "Signer not found" | Response from wrong wallet | Verify wallet integration and signer address |
| "Network mismatch" | Request/response mismatch | Check SOROBAN_NETWORK env var |
| "Signature expired" | Response too old | Rebuild request and re-sign |
| "Submission failed" | Network or account issue | Check account balance and sequence number |

## Summary

The complete signing workflow provides:
- ✅ **Safe transaction building** with validation
- ✅ **Wallet integration** with Freighter and others
- ✅ **Response validation** before submission
- ✅ **Error handling** for all failure modes
- ✅ **Audit trail** for security and compliance
- ✅ **Batch processing** capability for efficiency

For complete API documentation, see [SIGNING_REQUEST_GUIDE.md](SIGNING_REQUEST_GUIDE.md).
