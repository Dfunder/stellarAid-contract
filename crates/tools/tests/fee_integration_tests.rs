use fee::{
    cache::FeeCache, calculator::{calculate_fee, stroops_to_xlm, xlm_to_stroops, FeeConfig},
    currency::{Currency, CurrencyConverter}, history::FeeHistory, service::FeeServiceConfig,
    surge_pricing::{SurgePricingAnalyzer, SurgePricingConfig},
};

/// Calculator module tests
#[test]
fn test_single_operation_fee_calculation() {
    let fee = calculate_fee(100, 1).unwrap();
    assert_eq!(fee, 100);
}

#[test]
fn test_donation_operation_fee_calculation() {
    // Typical donation: 2 operations (payment + contract invoke)
    let fee = calculate_fee(100, 2).unwrap();
    assert_eq!(fee, 200);

    let xlm = stroops_to_xlm(fee);
    assert_eq!(xlm, 0.00002);
}

#[test]
fn test_complex_transaction_fee_calculation() {
    // Complex transaction: 5 operations
    let fee = calculate_fee(100, 5).unwrap();
    assert_eq!(fee, 500);

    let xlm = stroops_to_xlm(fee);
    assert!((xlm - 0.00005).abs() < f64::EPSILON);
}

#[test]
fn test_stroops_xlm_conversion_roundtrip() {
    // Test roundtrip conversion
    let original_xlm = 1.5;
    let stroops = xlm_to_stroops(original_xlm);
    let converted_back = stroops_to_xlm(stroops);

    assert!((converted_back - original_xlm).abs() < 0.00000001);
}

#[test]
fn test_fee_config_creation() {
    let config = FeeConfig::default();
    assert_eq!(config.base_fee_stroops, 100);
    assert_eq!(config.min_fee_xlm, 0.00001);
    assert!(config.max_fee_xlm > 0.0);
}

/// Surge pricing tests
#[test]
fn test_surge_pricing_normal_fee() {
    let config = SurgePricingConfig::default();
    let mut analyzer = SurgePricingAnalyzer::new(config);

    let analysis = analyzer.analyze(100).unwrap();
    assert!(!analysis.is_surge);
    assert_eq!(analysis.surge_percent, 100.0);
}

#[test]
fn test_surge_pricing_elevated_fee() {
    let config = SurgePricingConfig::default();
    let mut analyzer = SurgePricingAnalyzer::new(config);

    let analysis = analyzer.analyze(120).unwrap();
    assert!(!analysis.is_surge); // Elevated but not surging yet
    assert_eq!(analysis.surge_percent, 120.0);
}

#[test]
fn test_surge_pricing_high_fee() {
    let config = SurgePricingConfig::default();
    let mut analyzer = SurgePricingAnalyzer::new(config);

    let analysis = analyzer.analyze(200).unwrap();
    assert!(analysis.is_surge);
    assert_eq!(analysis.surge_percent, 200.0);
}

#[test]
fn test_surge_pricing_critical_fee() {
    let config = SurgePricingConfig::default();
    let mut analyzer = SurgePricingAnalyzer::new(config);

    let analysis = analyzer.analyze(500).unwrap();
    assert!(analysis.is_surge);
    assert_eq!(analysis.surge_percent, 500.0);
}

#[test]
fn test_surge_pricing_trend_detection() {
    let config = SurgePricingConfig::default();
    let mut analyzer = SurgePricingAnalyzer::new(config);

    // Simulate increasing fees
    for fee in 100..110 {
        analyzer.analyze(fee).unwrap();
    }

    let final_analysis = analyzer.analyze(150).unwrap();
    assert_eq!(final_analysis.trend, fee::surge_pricing::FeeTrend::Increasing);
}

/// Caching tests
#[test]
fn test_fee_cache_set_and_retrieve() {
    let mut cache = FeeCache::default_ttl();
    cache.set(100).unwrap();

    assert_eq!(cache.get(), Some(100));
}

#[test]
fn test_fee_cache_validity_check() {
    let mut cache = FeeCache::new(10); // 10 second TTL
    cache.set(100).unwrap();

    assert!(cache.is_valid());
}

#[test]
fn test_fee_cache_clear() {
    let mut cache = FeeCache::default_ttl();
    cache.set(100).unwrap();
    assert!(cache.has_data());

    cache.clear();
    assert!(!cache.has_data());
    assert!(cache.get().is_none());
}

/// Currency converter tests
#[test]
fn test_currency_converter_set_rate() {
    let mut converter = CurrencyConverter::new();
    converter
        .set_rate(Currency::XLM, Currency::USD, 0.25)
        .unwrap();

    let rate = converter.get_rate(Currency::XLM, Currency::USD).unwrap();
    assert_eq!(rate, 0.25);
}

#[test]
fn test_currency_converter_xlm_to_usd() {
    let mut converter = CurrencyConverter::new();
    converter
        .set_rate(Currency::XLM, Currency::USD, 0.25)
        .unwrap();

    let usd = converter.convert_xlm_fee(1.0, Currency::USD).unwrap();
    assert_eq!(usd, 0.25);
}

#[test]
fn test_currency_converter_donation_fee() {
    let mut converter = CurrencyConverter::new();
    converter
        .set_rate(Currency::XLM, Currency::USD, 0.25)
        .unwrap();

    // Donation fee in XLM
    let fee_xlm = stroops_to_xlm(200); // 2 operations

    // Convert to USD
    let fee_usd = converter.convert_xlm_fee(fee_xlm, Currency::USD).unwrap();

    // 0.00002 XLM * 0.25 USD/XLM = 0.000005 USD
    assert!((fee_usd - 0.000005).abs() < 0.000001);
}

#[test]
fn test_currency_converter_various_currencies() {
    let mut converter = CurrencyConverter::new();

    // Set rates for multiple currencies
    converter
        .set_rate(Currency::XLM, Currency::USD, 0.25)
        .unwrap();
    converter
        .set_rate(Currency::XLM, Currency::EUR, 0.23)
        .unwrap();
    converter
        .set_rate(Currency::XLM, Currency::GBP, 0.20)
        .unwrap();

    // Test conversions
    let usd = converter.convert_xlm_fee(1.0, Currency::USD).unwrap();
    let eur = converter.convert_xlm_fee(1.0, Currency::EUR).unwrap();
    let gbp = converter.convert_xlm_fee(1.0, Currency::GBP).unwrap();

    assert_eq!(usd, 0.25);
    assert_eq!(eur, 0.23);
    assert_eq!(gbp, 0.20);
}

/// History tracking tests
#[test]
fn test_fee_history_add_record() {
    let mut history = FeeHistory::new(10);
    history.add(100, "Horizon".to_string()).unwrap();

    assert_eq!(history.len(), 1);
    assert_eq!(history.oldest().unwrap().base_fee_stroops, 100);
}

#[test]
fn test_fee_history_multiple_records() {
    let mut history = FeeHistory::new(10);
    history.add(100, "Horizon".to_string()).unwrap();
    history.add(110, "Horizon".to_string()).unwrap();
    history.add(120, "Horizon".to_string()).unwrap();

    assert_eq!(history.len(), 3);
    assert_eq!(history.oldest().unwrap().base_fee_stroops, 100);
    assert_eq!(history.latest().unwrap().base_fee_stroops, 120);
}

#[test]
fn test_fee_history_capacity_limit() {
    let mut history = FeeHistory::new(3);

    // Add 5 records (more than capacity of 3)
    for i in 0..5 {
        history.add(100 + i, "Horizon".to_string()).unwrap();
    }

    // Should only have 3 most recent
    assert_eq!(history.len(), 3);
    assert_eq!(history.oldest().unwrap().base_fee_stroops, 102);
    assert_eq!(history.latest().unwrap().base_fee_stroops, 104);
}

#[test]
fn test_fee_history_statistics() {
    let mut history = FeeHistory::new(100);
    history.add(100, "Horizon".to_string()).unwrap();
    history.add(150, "Horizon".to_string()).unwrap();
    history.add(200, "Horizon".to_string()).unwrap();

    let stats = history.stats().unwrap();
    assert_eq!(stats.min_fee, 100);
    assert_eq!(stats.max_fee, 200);
    assert_eq!(stats.avg_fee, 150.0);
    assert_eq!(stats.median_fee, 150);
}

#[test]
fn test_fee_history_prune() {
    let mut history = FeeHistory::new(100);

    // Add old record
    history.add(100, "Horizon".to_string()).unwrap();

    // Add recent records
    history.add(150, "Horizon".to_string()).unwrap();
    history.add(200, "Horizon".to_string()).unwrap();

    assert_eq!(history.len(), 3);

    // Prune records older than 1 second (should remove none as all are recent)
    history.prune_older_than(1);
    assert_eq!(history.len(), 3);
}

/// End-to-end integration tests
#[test]
fn test_fee_estimation_workflow() {
    // Simulating a complete fee estimation workflow
    // without network calls

    // 1. Calculate fee
    let fee_stroops = calculate_fee(100, 2).unwrap(); // 2 operations
    assert_eq!(fee_stroops, 200);

    // 2. Convert to XLM
    let fee_xlm = stroops_to_xlm(fee_stroops);
    assert_eq!(fee_xlm, 0.00002);

    // 3. Convert to USD
    let mut converter = CurrencyConverter::new();
    converter
        .set_rate(Currency::XLM, Currency::USD, 0.25)
        .unwrap();
    let fee_usd = converter.convert_xlm_fee(fee_xlm, Currency::USD).unwrap();
    assert!((fee_usd - 0.000005).abs() < 0.000001);

    // 4. Check for surge (normal fee)
    let mut surge_analyzer = SurgePricingAnalyzer::new(SurgePricingConfig::default());
    let analysis = surge_analyzer.analyze(100).unwrap();
    assert!(!analysis.is_surge);

    // 5. Cache results
    let mut cache = FeeCache::default_ttl();
    cache.set(100).unwrap();
    assert_eq!(cache.get(), Some(100));
}

#[test]
fn test_surge_pricing_workflow() {
    // Simulate network congestion detection

    // 1. Normal fee
    let mut analyzer = SurgePricingAnalyzer::new(SurgePricingConfig::default());
    let analysis1 = analyzer.analyze(100).unwrap();
    assert!(!analysis1.is_surge);

    // 2. Fees start increasing
    let analysis2 = analyzer.analyze(150).unwrap();
    assert_eq!(analysis2.surge_level, fee::surge_pricing::SurgePricingLevel::Elevated);

    // 3. Critical surge
    let analysis3 = analyzer.analyze(400).unwrap();
    assert_eq!(analysis3.surge_level, fee::surge_pricing::SurgePricingLevel::Critical);
    assert!(analysis3.is_surge);
    assert!(analysis3.recommendation.len() > 0);
}

#[test]
fn test_multi_currency_fee_display() {
    // Test displaying fees in multiple currencies

    let fee_xlm = 0.00001;

    let mut rates = CurrencyConverter::new();
    rates.set_rate(Currency::XLM, Currency::USD, 0.25).unwrap();
    rates.set_rate(Currency::XLM, Currency::EUR, 0.23).unwrap();
    rates.set_rate(Currency::XLM, Currency::GBP, 0.20).unwrap();
    rates.set_rate(Currency::XLM, Currency::JPY, 26.5).unwrap();

    let usd = rates.convert_xlm_fee(fee_xlm, Currency::USD).unwrap();
    let eur = rates.convert_xlm_fee(fee_xlm, Currency::EUR).unwrap();
    let gbp = rates.convert_xlm_fee(fee_xlm, Currency::GBP).unwrap();
    let jpy = rates.convert_xlm_fee(fee_xlm, Currency::JPY).unwrap();

    // Base fee: 100 stroops = 0.00001 XLM
    assert!((usd - 0.0000025).abs() < 0.00001);
    assert!((eur - 0.0000023).abs() < 0.00001);
    assert!((gbp - 0.000002).abs() < 0.00001);
    assert!((jpy - 0.000265).abs() < 0.0001);
}

#[test]
fn test_batch_fee_estimation() {
    // Test estimating fees for various operation counts

    let operation_counts = vec![1, 2, 3, 5, 10, 20];
    let mut fees = Vec::new();

    for count in &operation_counts {
        let fee = calculate_fee(100, *count).unwrap();
        fees.push(fee);
    }

    // Verify linear scaling
    assert_eq!(fees[0], 100); // 1 op
    assert_eq!(fees[1], 200); // 2 ops
    assert_eq!(fees[2], 300); // 3 ops
    assert_eq!(fees[3], 500); // 5 ops
    assert_eq!(fees[4], 1000); // 10 ops
    assert_eq!(fees[5], 2000); // 20 ops
}

#[test]
fn test_surge_pricing_level_names() {
    use fee::surge_pricing::SurgePricingLevel;

    assert_eq!(SurgePricingLevel::Normal.name(), "Normal");
    assert_eq!(SurgePricingLevel::Elevated.name(), "Elevated");
    assert_eq!(SurgePricingLevel::High.name(), "High");
    assert_eq!(SurgePricingLevel::Critical.name(), "Critical");
}

#[test]
fn test_fee_service_config_creation() {
    let config = FeeServiceConfig::default();
    assert!(!config.horizon_url.is_empty());
    assert_eq!(config.cache_ttl_secs, 300);
    assert!(config.enable_surge_detection);
}
