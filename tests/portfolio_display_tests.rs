use rstock::cli::display::portfolio::portfolio_totals_summary;
use rstock::models::PortfolioResult;

#[test]
fn portfolio_summary_keeps_non_positive_lifetime_dividends_and_open_position_gain_loss_visible() {
    let summary = portfolio_totals_summary(&PortfolioResult {
        base_currency: "EUR".to_string(),
        rows: Vec::new(),
        monetary_positions: Vec::new(),
        total_monetary_value: Some(0.0),
        total_invested: 100.0,
        total_current_value: 90.0,
        total_dividends: -1.5,
        total_open_position_gain_loss: -10.0,
        total_open_position_gain_loss_pct: -10.0,
        snapshot_date: None,
        nav: None,
        daily_change: None,
        daily_change_pct: None,
        inception_date: None,
        ytd_return: None,
        one_year_return: None,
        three_year_return: None,
        five_year_return: None,
        ytd_metrics: None,
        one_year_metrics: None,
        three_year_metrics: None,
        five_year_metrics: None,
        market_data_limitations: Vec::new(),
        monetary_market_data_limitations: Vec::new(),
    });
    assert!(summary.contains("Lifetime Dividends: -1,50"));
    assert!(summary.contains("Open-position Gain/Loss:"));
}
