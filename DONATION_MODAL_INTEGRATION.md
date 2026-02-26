# Fee Integration Guide for Donation Modal

## Overview

This guide shows how to integrate the fee estimation service into your donation modal, confirmation screens, and wallet UI.

## Architecture Integration

```
┌─────────────────────────────────────────┐
│         Donation Modal (UI)             │
│  ┌─────────────────────────────────┐   │
│  │ Donation Amount Input           │   │
│  └─────────────────────────────────┘   │
│  ┌─────────────────────────────────┐   │
│  │ Network Fee Display   (FEE)     │◄──┤── FeeEstimationService
│  │ "0.00002 XLM ($0.000005)"       │   │
│  └─────────────────────────────────┘   │
│  ┌─────────────────────────────────┐   │
│  │ Total Cost                      │   │
│  │ = Donation + Fee                │   │
│  └─────────────────────────────────┘   │
│  ┌─────────────────────────────────┐   │
│  │ [Confirm] [Cancel]              │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
         │
         ▼
    ┌──────────────────┐
    │ FeeEstimation    │
    │ Service          │
    │ - Surge detect   │
    │ - Cache (5min)   │
    │ - Currency conv  │
    │ - History track  │
    └──────────────────┘
         │
         ▼
    [ Horizon API ]
```

## Step 1: Initialize Service

### Rust Backend

```rust
use fee::FeeEstimationService;

// In your service initialization
pub struct DonationService {
    fee_service: FeeEstimationService,
    // ... other fields
}

impl DonationService {
    pub fn new() -> Self {
        // Create service for public Horizon
        let fee_service = FeeEstimationService::public_horizon();
        
        Self {
            fee_service,
            // ... initialize other fields
        }
    }
}
```

## Step 2: Set Exchange Rates

### Update Exchange Rates Periodically

```rust
use fee::Currency;

// In your price feed module
pub async fn update_exchange_rates(
    donation_service: &DonationService,
) -> Result<()> {
    // Fetch from your price provider
    let rates = fetch_current_rates().await?;
    
    // Update service rates
    for (currency, rate) in rates {
        donation_service
            .fee_service
            .set_exchange_rate(Currency::XLM, currency, rate)
            .await?;
    }
    
    Ok(())
}

async fn fetch_current_rates() -> Result<Vec<(Currency, f64)>> {
    // Example: fetch from an oracle or API
    Ok(vec![
        (Currency::USD, 0.25),
        (Currency::EUR, 0.23),
        (Currency::GBP, 0.20),
    ])
}
```

## Step 3: Calculate Fees in Modal

### API Endpoint for Fee Estimates

```rust
use fee::Currency;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct FeeEstimateResponse {
    pub fee_xlm: f64,
    pub fee_converted: f64,
    pub currency: String,
    pub is_surging: bool,
    pub surge_level: Option<String>,
    pub total_cost_xlm: f64,
    pub cache_age_seconds: Option<i64>,
}

#[derive(Deserialize)]
pub struct FeeEstimateRequest {
    pub donation_amount: f64,
    pub operation_count: u32,
    pub target_currency: String,
}

pub async fn estimate_donation_fee(
    donation_service: &DonationService,
    req: FeeEstimateRequest,
) -> Result<FeeEstimateResponse> {
    // Parse target currency
    let currency = Currency::from_code(&req.target_currency)?;
    
    // Estimate fee (2 operations: payment + contract invoke)
    let (fee_info, fee_converted) = donation_service
        .fee_service
        .estimate_fee_in_currency(req.operation_count, currency)
        .await?;
    
    // Get cache metadata
    let cache_metadata = donation_service
        .fee_service
        .get_cache_metadata()
        .await;
    
    // Build response
    Ok(FeeEstimateResponse {
        fee_xlm: fee_info.total_fee_xlm,
        fee_converted,
        currency: currency.code().to_string(),
        is_surging: fee_info.is_surge_pricing,
        surge_level: if fee_info.is_surge_pricing {
            Some(format!("{}%", fee_info.surge_percent as i64))
        } else {
            None
        },
        total_cost_xlm: req.donation_amount + fee_info.total_fee_xlm,
        cache_age_seconds: cache_metadata.map(|m| m.age_seconds),
    })
}
```

## Step 4: Frontend Display

### React/Vue Component Example

```jsx
// DonationModal.jsx
import { useState, useEffect } from 'react';

export function DonationModal({ userBalance, preferredCurrency }) {
  const [amount, setAmount] = useState('');
  const [feeEstimate, setFeeEstimate] = useState(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);

  // Fetch fee estimate whenever amount changes
  useEffect(() => {
    if (!amount || parseFloat(amount) <= 0) {
      setFeeEstimate(null);
      return;
    }

    const fetchFee = async () => {
      setLoading(true);
      setError(null);
      try {
        const response = await fetch('/api/estimate-fee', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            donation_amount: parseFloat(amount),
            operation_count: 2, // Payment + contract invoke
            target_currency: preferredCurrency || 'USD',
          }),
        });

        if (!response.ok) {
          throw new Error('Fee estimation failed');
        }

        const data = await response.json();
        setFeeEstimate(data);
      } catch (err) {
        setError('Unable to fetch fee');
        console.error(err);
      } finally {
        setLoading(false);
      }
    };

    // Debounce API calls
    const timer = setTimeout(fetchFee, 300);
    return () => clearTimeout(timer);
  }, [amount, preferredCurrency]);

  const canAfford = feeEstimate && parseFloat(amount) + feeEstimate.total_cost_xlm <= userBalance;

  return (
    <div className="donation-modal">
      <h2>Donate to StellarAid</h2>

      {/* Input Section */}
      <div className="input-section">
        <label>Amount (XLM)</label>
        <input
          type="number"
          value={amount}
          onChange={(e) => setAmount(e.target.value)}
          placeholder="0.00"
        />
      </div>

      {/* Fee Display */}
      {feeEstimate && (
        <div className={`fee-section ${feeEstimate.is_surging ? 'warning' : ''}`}>
          <div className="fee-header">
            <span>Network Fee</span>
            {feeEstimate.is_surging && (
              <span className="surge-badge">⚠️ SURGE</span>
            )}
          </div>

          {/* Fee Breakdown */}
          <div className="fee-breakdown">
            <div className="fee-row">
              <span className="label">Fee (XLM)</span>
              <span className="value">{feeEstimate.fee_xlm.toFixed(8)}</span>
            </div>

            <div className="fee-row">
              <span className="label">Fee ({feeEstimate.currency})</span>
              <span className="value">
                {formatCurrency(feeEstimate.fee_converted, feeEstimate.currency)}
              </span>
            </div>

            {/* Surge Pricing Alert */}
            {feeEstimate.is_surging && (
              <div className="surge-alert">
                <p>Network is experiencing high congestion.</p>
                <p>Fees are {feeEstimate.surge_level} above normal.</p>
                <p className="tip">💡 Consider waiting if not urgent</p>
              </div>
            )}

            {/* Cache Age Notice */}
            {feeEstimate.cache_age_seconds !== null && (
              <div className="cache-notice">
                {feeEstimate.cache_age_seconds < 60
                  ? 'Updated just now'
                  : `Updated ${Math.floor(feeEstimate.cache_age_seconds / 60)}m ago`}
              </div>
            )}
          </div>

          <div className="fee-divider" />

          {/* Total Cost */}
          <div className="total-cost">
            <span className="label">Your Cost</span>
            <span className="amount">
              {amount} + {feeEstimate.fee_xlm.toFixed(8)} = {feeEstimate.total_cost_xlm.toFixed(8)} XLM
            </span>
            <span className="balance">
              Wallet Balance: {userBalance.toFixed(8)} XLM
            </span>
          </div>
        </div>
      )}

      {/* Error State */}
      {error && <div className="error-message">{error}</div>}

      {/* Loading State */}
      {loading && <div className="loading">Calculating fee...</div>}

      {/* Buttons */}
      <div className="button-group">
        <button
          className="confirm-btn"
          onClick={handleConfirm}
          disabled={!canAfford || !amount || loading}
        >
          {!canAfford ? '❌ Insufficient Balance' : 'Confirm Donation'}
        </button>
        <button className="cancel-btn" onClick={handleCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

function formatCurrency(amount, currency) {
  const symbols = {
    USD: '$',
    EUR: '€',
    GBP: '£',
    JPY: '¥',
  };
  return `${symbols[currency] || currency} ${amount.toFixed(6)}`;
}
```

## Step 5: Surge Pricing Handling

### Display Surge Warnings

```rust
pub fn get_surge_warning(fee_info: &FeeInfo) -> Option<String> {
    if !fee_info.is_surge_pricing {
        return None;
    }

    let surge_percent = fee_info.surge_percent as i64 - 100;
    
    if surge_percent > 300 {
        Some(format!(
            "🔴 CRITICAL SURGE: Fees are {}% higher than normal. \
             Consider waiting if possible.",
            surge_percent
        ))
    } else if surge_percent > 100 {
        Some(format!(
            "🟡 HIGH SURGE: Fees are {}% higher than normal. \
             Proceed with caution.",
            surge_percent
        ))
    } else {
        Some(format!(
            "🟠 ELEVATED: Fees are {}% higher than normal.",
            surge_percent
        ))
    }
}
```

## Step 6: Confirmation Screen

### Display Fee Summary Before Signing

```rust
pub struct DonationConfirmation {
    pub donation_amount: f64,
    pub network_fee_xlm: f64,
    pub total_xlm: f64,
    pub converted_donation: f64,
    pub converted_fee: f64,
    pub converted_total: f64,
    pub currency: String,
    pub recipient: String,
    pub surge_warning: Option<String>,
}

pub async fn prepare_confirmation(
    donation_service: &DonationService,
    donation_amount: f64,
    currency: Currency,
    recipient: String,
) -> Result<DonationConfirmation> {
    // Estimate fee (2 operations)
    let (fee_info, converted_fee) = donation_service
        .fee_service
        .estimate_fee_in_currency(2, currency)
        .await?;

    // Convert donation amount to display currency
    let converted_donation = donation_service
        .fee_service
        .converter  // Would need to expose this
        .convert_xlm_fee(donation_amount, currency)?;

    // Get surge warning
    let surge_warning = get_surge_warning(&fee_info);

    Ok(DonationConfirmation {
        donation_amount,
        network_fee_xlm: fee_info.total_fee_xlm,
        total_xlm: donation_amount + fee_info.total_fee_xlm,
        converted_donation,
        converted_fee,
        converted_total: converted_donation + converted_fee,
        currency: currency.code().to_string(),
        recipient,
        surge_warning,
    })
}
```

## Step 7: Health Monitoring

### Monitor Horizon Availability

```rust
pub async fn monitor_horizon_health(
    donation_service: &DonationService,
) -> Result<String> {
    // Check if Horizon is available
    match donation_service.fee_service.estimate_fee(1).await {
        Ok(_) => Ok("✅ Network is healthy".to_string()),
        Err(FeeError::Timeout) => {
            Ok("⚠️ Horizon request timed out (using cached fee)".to_string())
        }
        Err(FeeError::HorizonUnavailable(_)) => {
            Err("❌ Horizon is unavailable. Please try again later.")
        }
        Err(e) => Err(format!("Fee service error: {}", e)),
    }
}
```

## Step 8: Error Handling

### Graceful Fallback Strategy

```rust
pub async fn estimate_fee_with_fallback(
    donation_service: &DonationService,
    operation_count: u32,
    currency: Currency,
) -> Result<(FeeInfo, f64)> {
    match donation_service
        .fee_service
        .estimate_fee_in_currency(operation_count, currency)
        .await
    {
        Ok((info, converted)) => Ok((info, converted)),
        
        Err(FeeError::Timeout) => {
            log::warn!("Fee estimation timed out, using cached value");
            // Use last known fee from cache
            match donation_service.fee_service.estimate_fee(operation_count).await {
                Ok(info) => Ok((info, 0.0)), // Return XLM only
                Err(e) => Err(e.into()),
            }
        }
        
        Err(FeeError::HorizonUnavailable(_)) => {
            log::error!("Horizon unavailable, using standard fee");
            // Use standard fee (100 stroops per operation)
            let standard_fee = FeeInfo::new(100, operation_count, false, 100.0)?;
            Ok((standard_fee, 0.0))
        }
        
        Err(e) => Err(e.into()),
    }
}
```

## CSS Styling

### Donation Modal Styles

```css
.donation-modal {
  max-width: 500px;
  padding: 2rem;
  border-radius: 8px;
  box-shadow: 0 4px 6px rgba(0, 0, 0, 0.1);
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
}

.input-section {
  margin-bottom: 2rem;
}

.input-section input {
  width: 100%;
  padding: 0.75rem;
  font-size: 1rem;
  border: 2px solid #e0e0e0;
  border-radius: 4px;
  transition: border-color 0.2s;
}

.input-section input:focus {
  outline: none;
  border-color: #007bff;
}

.fee-section {
  background: #f5f5f5;
  padding: 1rem;
  border-radius: 6px;
  margin-bottom: 1.5rem;
  border-left: 4px solid #007bff;
}

.fee-section.warning {
  border-left-color: #ff9800;
  background: #fff3e0;
}

.fee-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  font-weight: 600;
  margin-bottom: 1rem;
  font-size: 0.95rem;
}

.surge-badge {
  background: #ff9800;
  color: white;
  padding: 0.25rem 0.75rem;
  border-radius: 20px;
  font-size: 0.8rem;
  font-weight: bold;
}

.fee-breakdown {
  font-size: 0.9rem;
}

.fee-row {
  display: flex;
  justify-content: space-between;
  padding: 0.5rem 0;
  border-bottom: 1px solid #e0e0e0;
}

.fee-row:last-child {
  border-bottom: none;
}

.fee-row .label {
  color: #666;
}

.fee-row .value {
  font-weight: 600;
  color: #333;
  font-family: 'Monaco', 'Courier New', monospace;
}

.surge-alert {
  background: #fff3e0;
  border: 1px solid #ff9800;
  padding: 0.75rem;
  border-radius: 4px;
  margin-top: 1rem;
  font-size: 0.85rem;
  color: #e65100;
}

.surge-alert p {
  margin: 0.25rem 0;
}

.surge-alert .tip {
  font-weight: 600;
  margin-top: 0.5rem;
}

.cache-notice {
  font-size: 0.75rem;
  color: #999;
  margin-top: 0.5rem;
  text-align: right;
}

.fee-divider {
  height: 2px;
  background: #ddd;
  margin: 1rem 0;
}

.total-cost {
  text-align: right;
}

.total-cost .label {
  display: block;
  font-size: 0.85rem;
  color: #666;
  margin-bottom: 0.25rem;
}

.total-cost .amount {
  display: block;
  font-size: 1.2rem;
  font-weight: bold;
  color: #333;
  font-family: 'Monaco', 'Courier New', monospace;
  margin-bottom: 0.5rem;
}

.total-cost .balance {
  display: block;
  font-size: 0.85rem;
  color: #999;
}

.button-group {
  display: flex;
  gap: 1rem;
  margin-top: 2rem;
}

.confirm-btn, .cancel-btn {
  flex: 1;
  padding: 0.75rem;
  font-size: 1rem;
  border: none;
  border-radius: 4px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s;
}

.confirm-btn {
  background: #007bff;
  color: white;
}

.confirm-btn:hover:not(:disabled) {
  background: #0056b3;
}

.confirm-btn:disabled {
  background: #ccc;
  cursor: not-allowed;
  opacity: 0.6;
}

.cancel-btn {
  background: #f0f0f0;
  color: #333;
}

.cancel-btn:hover {
  background: #e0e0e0;
}

.error-message {
  background: #ffebee;
  color: #c62828;
  padding: 0.75rem;
  border-radius: 4px;
  margin-bottom: 1rem;
  font-size: 0.9rem;
}

.loading {
  text-align: center;
  color: #999;
  padding: 1rem;
  font-size: 0.9rem;
}
```

## Testing Integration

### Integration Test Example

```rust
#[tokio::test]
async fn test_donation_with_fee_estimate() {
    let donation_service = DonationService::new();
    
    // Set exchange rate
    donation_service
        .fee_service
        .set_exchange_rate(Currency::XLM, Currency::USD, 0.25)
        .await
        .unwrap();
    
    // Estimate fee for 2-operation donation
    let (fee_info, fee_usd) = donation_service
        .fee_service
        .estimate_fee_in_currency(2, Currency::USD)
        .await
        .unwrap();
    
    // Verify
    assert_eq!(fee_info.operation_count, 2);
    assert_eq!(fee_info.total_fee_xlm, 0.00002);
    assert!((fee_usd - 0.000005).abs() < 0.000001);
    assert!(!fee_info.is_surge_pricing);
}
```

## Deployment Checklist

- ✅ Fee service initialized at startup
- ✅ Exchange rates updated periodically
- ✅ Fee estimation API endpoint created
- ✅ Modal component integrated
- ✅ Surge pricing warnings displayed
- ✅ Error handling implemented
- ✅ Fallback to cached fees
- ✅ CSS styling applied
- ✅ Tests passing
- ✅ Monitoring in place

---

**Ready to deploy!** 🚀
