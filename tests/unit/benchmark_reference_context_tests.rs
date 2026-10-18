use super::benchmark_reference_context_lines;
use crate::models::{
    BenchmarkReferenceInfo, MarketDataLimitation, MarketDataLimitationClassification,
    MarketDataSubject,
};
use chrono::NaiveDate;

#[test]
fn human_reference_context_includes_proxy_basis_and_fx_limitation() {
    let reference = BenchmarkReferenceInfo {
        id: "legacy-acwi-etf-proxy".to_owned(),
        display_name: "iShares MSCI ACWI ETF (ACWI) proxy".to_owned(),
        instrument_ticker: "ACWI".to_owned(),
        source: "yahoo_finance".to_owned(),
        source_identifier: "ACWI".to_owned(),
        currency: "USD".to_owned(),
        adjustment_provenance: "auto_adjust enabled".to_owned(),
        return_basis_qualification: "total-return equivalence unverified".to_owned(),
    };
    let limitation = MarketDataLimitation {
        subject: MarketDataSubject::FxRate {
            currency: "USD".to_owned(),
        },
        latest_available_date: Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap()),
        requested_end_date: NaiveDate::from_ymd_opt(2025, 1, 10).unwrap(),
        classification: MarketDataLimitationClassification::ActionableStaleData,
    };

    let rendered = benchmark_reference_context_lines(&reference, &[limitation]).join("\n");
    assert!(rendered.contains("iShares MSCI ACWI ETF (ACWI) proxy [legacy-acwi-etf-proxy]"));
    assert!(rendered.contains("yahoo_finance ACWI (USD)"));
    assert!(rendered.contains("auto_adjust enabled"));
    assert!(rendered.contains("total-return equivalence unverified"));
    assert!(rendered.contains("FX rate USD has latest rate from 01-01-2025"));
}
