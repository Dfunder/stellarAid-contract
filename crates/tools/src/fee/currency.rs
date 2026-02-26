use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::error::{FeeError, FeeResult};

/// Supported currencies for fee display
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Currency {
    XLM,
    USD,
    EUR,
    GBP,
    JPY,
    CNY,
    INR,
    BRL,
    AUD,
    CAD,
}

impl Currency {
    /// Get currency code
    pub fn code(&self) -> &str {
        match self {
            Currency::XLM => "XLM",
            Currency::USD => "USD",
            Currency::EUR => "EUR",
            Currency::GBP => "GBP",
            Currency::JPY => "JPY",
            Currency::CNY => "CNY",
            Currency::INR => "INR",
            Currency::BRL => "BRL",
            Currency::AUD => "AUD",
            Currency::CAD => "CAD",
        }
    }

    /// Get currency symbol
    pub fn symbol(&self) -> &str {
        match self {
            Currency::XLM => "XLM",
            Currency::USD => "$",
            Currency::EUR => "€",
            Currency::GBP => "£",
            Currency::JPY => "¥",
            Currency::CNY => "¥",
            Currency::INR => "₹",
            Currency::BRL => "R$",
            Currency::AUD => "A$",
            Currency::CAD => "C$",
        }
    }

    /// Parse from code string
    pub fn from_code(code: &str) -> FeeResult<Self> {
        match code.to_uppercase().as_str() {
            "XLM" => Ok(Currency::XLM),
            "USD" => Ok(Currency::USD),
            "EUR" => Ok(Currency::EUR),
            "GBP" => Ok(Currency::GBP),
            "JPY" => Ok(Currency::JPY),
            "CNY" => Ok(Currency::CNY),
            "INR" => Ok(Currency::INR),
            "BRL" => Ok(Currency::BRL),
            "AUD" => Ok(Currency::AUD),
            "CAD" => Ok(Currency::CAD),
            _ => Err(FeeError::InvalidCurrency(code.to_string())),
        }
    }

    /// Get all supported currencies
    pub fn all() -> Vec<Self> {
        vec![
            Currency::XLM,
            Currency::USD,
            Currency::EUR,
            Currency::GBP,
            Currency::JPY,
            Currency::CNY,
            Currency::INR,
            Currency::BRL,
            Currency::AUD,
            Currency::CAD,
        ]
    }
}

/// Exchange rate data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeRate {
    /// Base currency (XLM)
    pub base: Currency,
    /// Target currency
    pub target: Currency,
    /// Exchange rate (1 base = rate × target)
    pub rate: f64,
    /// When this rate was fetched
    pub fetched_at: chrono::DateTime<chrono::Utc>,
}

impl ExchangeRate {
    /// Create new exchange rate
    pub fn new(base: Currency, target: Currency, rate: f64) -> FeeResult<Self> {
        if rate <= 0.0 {
            return Err(FeeError::CurrencyConversionFailed(
                "exchange rate must be positive".to_string(),
            ));
        }

        Ok(Self {
            base,
            target,
            rate,
            fetched_at: chrono::Utc::now(),
        })
    }

    /// Get age of exchange rate in seconds
    pub fn age_seconds(&self) -> i64 {
        (chrono::Utc::now() - self.fetched_at).num_seconds()
    }

    /// Check if rate is fresh
    pub fn is_fresh(&self, max_age_seconds: i64) -> bool {
        self.age_seconds() < max_age_seconds
    }
}

/// Currency converter
pub struct CurrencyConverter {
    rates: HashMap<String, ExchangeRate>,
}

impl CurrencyConverter {
    /// Create new currency converter
    pub fn new() -> Self {
        Self {
            rates: HashMap::new(),
        }
    }

    /// Set exchange rate for currency pair
    pub fn set_rate(
        &mut self,
        base: Currency,
        target: Currency,
        rate: f64,
    ) -> FeeResult<()> {
        let rate_obj = ExchangeRate::new(base, target, rate)?;
        let key = format!("{}/{}", base.code(), target.code());
        self.rates.insert(key, rate_obj);
        Ok(())
    }

    /// Get exchange rate
    pub fn get_rate(&self, base: Currency, target: Currency) -> FeeResult<f64> {
        if base == target {
            return Ok(1.0);
        }

        let key = format!("{}/{}", base.code(), target.code());
        self.rates
            .get(&key)
            .map(|r| r.rate)
            .ok_or_else(|| {
                FeeError::CurrencyConversionFailed(format!(
                    "rate not available for {}/{}",
                    base.code(),
                    target.code()
                ))
            })
    }

    /// Convert amount from one currency to another
    pub fn convert(&self, amount: f64, from: Currency, to: Currency) -> FeeResult<f64> {
        if from == to {
            return Ok(amount);
        }

        if amount < 0.0 {
            return Err(FeeError::CurrencyConversionFailed(
                "amount cannot be negative".to_string(),
            ));
        }

        let rate = self.get_rate(from, to)?;
        Ok(amount * rate)
    }

    /// Convert XLM fee to target currency
    pub fn convert_xlm_fee(&self, xlm_amount: f64, target: Currency) -> FeeResult<f64> {
        if target == Currency::XLM {
            return Ok(xlm_amount);
        }

        let rate = self.get_rate(Currency::XLM, target)?;
        Ok(xlm_amount * rate)
    }

    /// Clear all rates
    pub fn clear(&mut self) {
        self.rates.clear();
    }

    /// Get number of cached rates
    pub fn rate_count(&self) -> usize {
        self.rates.len()
    }

    /// Check if specific rate is cached
    pub fn has_rate(&self, base: Currency, target: Currency) -> bool {
        if base == target {
            return true;
        }
        let key = format!("{}/{}", base.code(), target.code());
        self.rates.contains_key(&key)
    }
}

impl Default for CurrencyConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Formatted currency amount with symbol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormattedAmount {
    pub amount: f64,
    pub currency: Currency,
    pub symbol: String,
}

impl FormattedAmount {
    /// Create new formatted amount
    pub fn new(amount: f64, currency: Currency) -> Self {
        Self {
            amount,
            currency,
            symbol: currency.symbol().to_string(),
        }
    }

    /// Get formatted string
    pub fn to_string(&self) -> String {
        format!(
            "{} {:.8}",
            self.symbol,
            self.amount
        )
    }

    /// Get formatted string with specified precision
    pub fn to_string_precision(&self, precision: usize) -> String {
        format!(
            "{} {:.prec$}",
            self.symbol,
            self.amount,
            prec = precision
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_currency_code() {
        assert_eq!(Currency::XLM.code(), "XLM");
        assert_eq!(Currency::USD.code(), "USD");
        assert_eq!(Currency::EUR.code(), "EUR");
    }

    #[test]
    fn test_currency_symbol() {
        assert_eq!(Currency::XLM.symbol(), "XLM");
        assert_eq!(Currency::USD.symbol(), "$");
        assert_eq!(Currency::EUR.symbol(), "€");
    }

    #[test]
    fn test_currency_from_code() {
        assert_eq!(Currency::from_code("XLM").unwrap(), Currency::XLM);
        assert_eq!(Currency::from_code("usd").unwrap(), Currency::USD);
        assert!(Currency::from_code("INVALID").is_err());
    }

    #[test]
    fn test_currency_all() {
        let currencies = Currency::all();
        assert_eq!(currencies.len(), 10);
        assert!(currencies.contains(&Currency::XLM));
        assert!(currencies.contains(&Currency::USD));
    }

    #[test]
    fn test_exchange_rate_creation() {
        let rate = ExchangeRate::new(Currency::XLM, Currency::USD, 0.25).unwrap();
        assert_eq!(rate.rate, 0.25);
        assert_eq!(rate.base, Currency::XLM);
        assert_eq!(rate.target, Currency::USD);
    }

    #[test]
    fn test_exchange_rate_invalid() {
        let result = ExchangeRate::new(Currency::XLM, Currency::USD, -0.25);
        assert!(result.is_err());

        let result = ExchangeRate::new(Currency::XLM, Currency::USD, 0.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_converter_set_and_get_rate() {
        let mut converter = CurrencyConverter::new();
        converter
            .set_rate(Currency::XLM, Currency::USD, 0.25)
            .unwrap();

        let rate = converter.get_rate(Currency::XLM, Currency::USD).unwrap();
        assert_eq!(rate, 0.25);
    }

    #[test]
    fn test_converter_same_currency() {
        let converter = CurrencyConverter::new();
        let rate = converter.get_rate(Currency::XLM, Currency::XLM).unwrap();
        assert_eq!(rate, 1.0);
    }

    #[test]
    fn test_converter_convert() {
        let mut converter = CurrencyConverter::new();
        converter
            .set_rate(Currency::XLM, Currency::USD, 0.25)
            .unwrap();

        let usd = converter
            .convert(100.0, Currency::XLM, Currency::USD)
            .unwrap();
        assert_eq!(usd, 25.0);
    }

    #[test]
    fn test_converter_convert_xlm_fee() {
        let mut converter = CurrencyConverter::new();
        converter
            .set_rate(Currency::XLM, Currency::EUR, 0.22)
            .unwrap();

        let eur = converter.convert_xlm_fee(1.0, Currency::EUR).unwrap();
        assert_eq!(eur, 0.22);
    }

    #[test]
    fn test_converter_invalid_rate() {
        let converter = CurrencyConverter::new();
        let result = converter.get_rate(Currency::XLM, Currency::USD);
        assert!(result.is_err());
    }

    #[test]
    fn test_converter_clear() {
        let mut converter = CurrencyConverter::new();
        converter
            .set_rate(Currency::XLM, Currency::USD, 0.25)
            .unwrap();
        assert_eq!(converter.rate_count(), 1);

        converter.clear();
        assert_eq!(converter.rate_count(), 0);
    }

    #[test]
    fn test_formatted_amount() {
        let amount = FormattedAmount::new(1.5, Currency::USD);
        assert_eq!(amount.to_string_precision(2), "$ 1.50");
    }

    #[test]
    fn test_formatted_amount_xlm() {
        let amount = FormattedAmount::new(0.00001, Currency::XLM);
        let formatted = amount.to_string_precision(5);
        assert!(formatted.contains("XLM"));
    }
}
