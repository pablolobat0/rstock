//! Portfolio CLI output Adapter seam tests.
//!
//! The human and JSON output Adapters write one complete
//! [`rstock::models::PortfolioView`] through injected `Write` targets
//! (ADR-0004). These tests assert the JSON schema structurally without
//! relying on object field order, and the human output through semantic
//! states and labels without relying on spacing or ANSI formatting.

#![allow(clippy::float_cmp)]

pub mod common;

use rstock::cli::adapters;
use rstock::models::{
    DailyMovementCoverage, DailyMovementExclusionReason, DailyPricedHoldingsMovement,
    FactAvailability, InventoryPosition, InventorySectionAggregates, PortfolioInventory,
    PortfolioInventorySection, PortfolioPerformance, PortfolioView,
};
use serde_json::{json, Value};

// --- View construction helpers ---

fn zero_aggregates() -> InventorySectionAggregates {
    let zero = FactAvailability::Available(0.0);
    InventorySectionAggregates {
        current_value: zero.clone(),
        invested_cost: zero.clone(),
        dividends: zero.clone(),
        open_position_gain_loss: zero.clone(),
        open_position_gain_loss_pct: zero,
    }
}

fn section(positions: Vec<InventoryPosition>) -> PortfolioInventorySection {
    PortfolioInventorySection {
        positions,
        aggregates: zero_aggregates(),
        daily_priced_holdings_movement: DailyPricedHoldingsMovement {
            movement: FactAvailability::NotApplicable,
            movement_pct: FactAvailability::NotApplicable,
            covered_prior_value: FactAvailability::NotApplicable,
            included_positions: vec![],
            excluded_positions: vec![],
            limitations: vec![],
        },
        market_data_limitations: Vec::new(),
    }
}

/// An empty Portfolio view: known-zero sections, not-applicable performance,
/// and no ready history.
fn empty_view() -> PortfolioView {
    PortfolioView {
        inventory: PortfolioInventory {
            base_currency: "EUR".to_owned(),
            performance_holdings: section(vec![]),
            monetary_holdings: section(vec![]),
            total_value: FactAvailability::Available(0.0),
        },
        performance: PortfolioPerformance::NotApplicable,
        nav_history: Vec::new(),
    }
}

fn write_json(view: &PortfolioView) -> Value {
    let mut output = Vec::new();
    adapters::write_portfolio_json(&mut output, view)
        .expect("JSON adapter should write the outcome");
    common::parse_json_envelope(&output)
}

struct FailingWriter;

impl std::io::Write for FailingWriter {
    fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "simulated writer failure",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn json_adapter_propagates_writer_failures() {
    let error = adapters::write_portfolio_json(&mut FailingWriter, &empty_view())
        .expect_err("writer failure should be returned to the caller");

    assert!(format!("{error:#}").contains("simulated writer failure"));
}

// --- JSON adapter ---

#[test]
fn portfolio_json_renders_the_nested_schema_for_not_applicable_performance() {
    let value = write_json(&empty_view());

    assert_eq!(value["command"], "portfolio.get");
    let data = &value["data"];
    assert_eq!(data["base_currency"], "EUR");

    // Inventory: separate sections with complete-or-unavailable aggregates.
    let performance_holdings = &data["inventory"]["performance_holdings"];
    assert_eq!(performance_holdings["positions"], json!([]));
    assert_eq!(performance_holdings["aggregates"]["current_value"], 0.0);
    assert_eq!(performance_holdings["aggregates"]["invested_cost"], 0.0);
    assert_eq!(performance_holdings["aggregates"]["dividends"], 0.0);
    assert_eq!(
        performance_holdings["aggregates"]["open_position_gain_loss"],
        0.0
    );
    assert_eq!(
        performance_holdings["aggregates"]["open_position_gain_loss_pct"],
        0.0
    );
    assert_eq!(performance_holdings["market_data_limitations"], json!([]));
    assert_eq!(
        performance_holdings["daily_priced_holdings_movement"]["movement_availability"],
        "not_applicable"
    );
    assert_eq!(
        performance_holdings["daily_priced_holdings_movement"]["included_positions"],
        json!([])
    );
    assert_eq!(
        performance_holdings["daily_priced_holdings_movement"]["excluded_positions"],
        json!([])
    );
    assert_eq!(
        performance_holdings["daily_priced_holdings_movement"]["limitations"],
        json!([])
    );
    let monetary_holdings = &data["inventory"]["monetary_holdings"];
    assert_eq!(monetary_holdings["positions"], json!([]));
    assert_eq!(monetary_holdings["aggregates"]["current_value"], 0.0);
    assert_eq!(monetary_holdings["market_data_limitations"], json!([]));
    // The combined informational total stays an inventory fact.
    assert_eq!(data["inventory"]["total_value"], 0.0);

    // Performance is an explicit state, not nullable placeholder facts.
    assert_eq!(data["performance"]["state"], "not_applicable");
    assert!(data["performance"].get("nav").is_none());

    // The requested ready history is part of the outcome.
    assert_eq!(data["nav_history"], json!([]));
}

/// A limitation inside one scope must not leak into another: the four
/// structured scopes are preserved as distinct arrays.
fn limitation(ticker: &str, name: &str) -> rstock::models::MarketDataLimitation {
    rstock::models::MarketDataLimitation {
        subject: rstock::models::MarketDataSubject::Asset {
            ticker: ticker.to_owned(),
            name: name.to_owned(),
            asset_type: rstock::models::AssetType::Fund,
        },
        latest_available_date: Some(chrono::NaiveDate::from_ymd_opt(2025, 6, 9).unwrap()),
        requested_end_date: chrono::NaiveDate::from_ymd_opt(2025, 6, 10).unwrap(),
        classification: rstock::models::MarketDataLimitationClassification::ActionableReportingLag,
    }
}

#[test]
fn portfolio_json_maps_available_unavailable_and_not_applicable_facts_into_the_nested_schema() {
    let value = write_json(&populated_view());
    let data = &value["data"];

    // Positions sit inside their sections; ordering is by current value with
    // unvalued positions last.
    let positions = &data["inventory"]["performance_holdings"]["positions"];
    assert_eq!(positions.as_array().map(Vec::len), Some(2));
    assert_eq!(positions[0]["ticker"], "IE00XFAKE001");
    assert_eq!(positions[1]["ticker"], "IE00XFAKE002");

    // Descriptive metadata stays plain; calculated facts map through
    // availability.
    assert_eq!(positions[0]["name"], "History Fund");
    assert_eq!(positions[0]["asset_type"], "fund");
    assert_eq!(positions[0]["asset_class"], "equity");
    assert_eq!(positions[0]["equity_style"], "blend");
    assert_eq!(positions[0]["morningstar_code"], "F000XFAKE1");
    assert_eq!(positions[0]["quantity"], 2.0);
    assert_eq!(positions[0]["individual_price"]["native_price"], 9.0);
    assert_eq!(positions[0]["individual_price"]["price_date"], "2025-06-09");
    assert_eq!(positions[0]["current_value"], 18.0);
    assert_eq!(positions[0]["dividends"], 1.5);
    assert_eq!(positions[0]["open_position_gain_loss"], -2.0);
    assert_eq!(positions[0]["open_position_gain_loss_pct"], -10.0);

    // Unavailable facts are null; the unpriced position keeps its ledger
    // facts while its price-dependent facts disappear.
    assert!(positions[1]["individual_price"].is_null());
    assert!(positions[1]["current_value"].is_null());
    assert!(positions[1]["open_position_gain_loss"].is_null());
    assert_eq!(positions[1]["average_cost"], 12.0);
    assert_eq!(positions[1]["quantity"], 3.0);
    assert_eq!(positions[1]["morningstar_code"], "F000XFAKE2");

    // Available zero remains numeric zero in per-position facts and
    // aggregates.
    assert_eq!(
        data["inventory"]["monetary_holdings"]["positions"][0]["dividends"],
        0.0
    );
    assert_eq!(
        data["inventory"]["monetary_holdings"]["positions"][0]["open_position_gain_loss"],
        0.0
    );

    // Section aggregates: complete across scope or unavailable, never partial.
    let performance_aggregates = &data["inventory"]["performance_holdings"]["aggregates"];
    assert!(performance_aggregates["current_value"].is_null());
    assert_eq!(performance_aggregates["invested_cost"], 56.0);
    assert_eq!(performance_aggregates["dividends"], 1.5);
    assert!(performance_aggregates["open_position_gain_loss"].is_null());
    assert_eq!(
        data["inventory"]["monetary_holdings"]["aggregates"]["current_value"],
        100.0
    );
    assert_eq!(
        data["inventory"]["monetary_holdings"]["aggregates"]["dividends"],
        0.0
    );
    assert!(data["inventory"]["total_value"].is_null());

    // The available performance state carries the synchronized facts, the
    // five cohesive period outcomes, and its two limitation scopes.
    let performance = &data["performance"];
    assert_eq!(performance["state"], "available");
    assert_eq!(performance["effective_valuation_date"], "2025-06-09");
    assert_eq!(performance["synchronized_value"], 18.0);
    assert_eq!(performance["nav"], 100.0);
    assert_eq!(performance["inception_date"], "2025-06-01");

    let three_years = &performance["periods"]["three_years"];
    assert_eq!(three_years["return_pct"], 12.5);
    assert_eq!(three_years["volatility"], 9.0);
    assert_eq!(three_years["max_drawdown"], -15.0);
    assert_eq!(three_years["beta"], 1.1);
    assert_eq!(three_years["sharpe"], 0.9);
    assert_eq!(three_years["sortino"], 1.3);

    // Unavailable (missing benchmark input) and not-applicable (undefined
    // math or insufficient history) facts both map to null.
    let ytd = &performance["periods"]["ytd"];
    assert_eq!(ytd["return_pct"], 2.0);
    assert_eq!(ytd["volatility"], 1.5);
    assert_eq!(ytd["max_drawdown"], -3.0);
    assert!(ytd["beta"].is_null());
    assert_eq!(ytd["sharpe"], 0.4);
    assert!(ytd["sortino"].is_null());
    assert!(performance["periods"]["one_year"]["return_pct"].is_null());
    assert!(performance["periods"]["five_years"]["beta"].is_null());
    assert_eq!(performance["periods"]["all"]["sharpe"], 0.6);

    // The four structured limitation scopes stay separated.
    assert_eq!(
        performance["nav_history_limitations"][0]["subject"]["ticker"],
        "XFAKE1"
    );
    assert_eq!(
        performance["benchmark_risk_limitations"][0]["subject"]["ticker"],
        "ACWI"
    );
    assert_eq!(
        data["inventory"]["performance_holdings"]["market_data_limitations"][0]["subject"]
            ["ticker"],
        "XFAKE2"
    );
    assert_eq!(
        data["inventory"]["monetary_holdings"]["market_data_limitations"][0]["subject"]["ticker"],
        "XFAKEM1"
    );
    // Per-position limitations remain visible inside the section.
    assert_eq!(
        positions[1]["market_data_limitations"][0]["subject"]["ticker"],
        "XFAKE2"
    );

    // The requested ready history is included with raw snapshot values.
    assert_eq!(data["nav_history"][0]["date"], "2025-06-09");
    assert_eq!(data["nav_history"][0]["asset_value"], 18.0);
    assert_eq!(data["nav_history"][0]["total_value"], 18.0);
    assert_eq!(data["nav_history"][0]["outstanding_shares"], 0.18);
    assert_eq!(data["nav_history"][0]["nav"], 100.0);
}

#[test]
fn portfolio_json_exposes_daily_movement_values_coverage_and_limitations() {
    let mut view = populated_view();
    view.inventory
        .performance_holdings
        .daily_priced_holdings_movement = DailyPricedHoldingsMovement {
        movement: FactAvailability::Available(4.0),
        movement_pct: FactAvailability::Available(10.0),
        covered_prior_value: FactAvailability::Available(40.0),
        included_positions: vec![DailyMovementCoverage {
            ticker: "XFAKE1".into(),
            baseline_date: Some("2025-06-09".into()),
            baseline_fx_date: Some("2025-06-09".into()),
            prior_base_currency_value: Some(40.0),
            current_base_currency_value: Some(44.0),
            exclusion_reason: None,
        }],
        excluded_positions: vec![DailyMovementCoverage {
            ticker: "XFAKE2".into(),
            baseline_date: None,
            baseline_fx_date: None,
            prior_base_currency_value: None,
            current_base_currency_value: None,
            exclusion_reason: Some(DailyMovementExclusionReason::MissingCurrentPriceOrFx),
        }],
        limitations: vec![limitation("XFAKE2", "Missing FX")],
    };

    let value = write_json(&view);
    let movement =
        &value["data"]["inventory"]["performance_holdings"]["daily_priced_holdings_movement"];
    assert_eq!(movement["movement"], 4.0);
    assert_eq!(movement["movement_availability"], "available");
    assert_eq!(movement["movement_pct"], 10.0);
    assert_eq!(movement["covered_prior_value"], 40.0);
    assert_eq!(
        movement["included_positions"][0]["baseline_date"],
        "2025-06-09"
    );
    assert_eq!(
        movement["excluded_positions"][0]["exclusion_reason"],
        "missing current price or required FX input"
    );
    assert_eq!(movement["limitations"].as_array().map(Vec::len), Some(1));
}

fn unpriced_position(limitation: rstock::models::MarketDataLimitation) -> InventoryPosition {
    InventoryPosition {
        ticker: "IE00XFAKE002".to_owned(),
        name: "Unpriced Fund".to_owned(),
        asset_type: rstock::models::AssetType::Fund,
        currency: "EUR".to_owned(),
        morningstar_code: Some("F000XFAKE2".to_owned()),
        asset_class: Some("equity".to_owned()),
        equity_style: None,
        management: None,
        quantity: 3.0,
        average_cost: FactAvailability::Available(12.0),
        invested_cost: FactAvailability::Available(36.0),
        dividends: FactAvailability::Available(0.0),
        individual_price: FactAvailability::Unavailable,
        current_value: FactAvailability::Unavailable,
        open_position_gain_loss: FactAvailability::Unavailable,
        open_position_gain_loss_pct: FactAvailability::Unavailable,
        market_data_limitations: vec![limitation],
    }
}

fn priced_position() -> InventoryPosition {
    InventoryPosition {
        ticker: "IE00XFAKE001".to_owned(),
        name: "History Fund".to_owned(),
        asset_type: rstock::models::AssetType::Fund,
        currency: "USD".to_owned(),
        morningstar_code: Some("F000XFAKE1".to_owned()),
        asset_class: Some("equity".to_owned()),
        equity_style: Some("blend".to_owned()),
        management: Some("active".to_owned()),
        quantity: 2.0,
        average_cost: FactAvailability::Available(10.0),
        invested_cost: FactAvailability::Available(20.0),
        dividends: FactAvailability::Available(1.5),
        individual_price: FactAvailability::Available(rstock::models::IndividualPrice {
            native_price: 9.0,
            price_date: "2025-06-09".to_owned(),
        }),
        current_value: FactAvailability::Available(18.0),
        open_position_gain_loss: FactAvailability::Available(-2.0),
        open_position_gain_loss_pct: FactAvailability::Available(-10.0),
        market_data_limitations: Vec::new(),
    }
}

fn monetary_position(limitation: rstock::models::MarketDataLimitation) -> InventoryPosition {
    InventoryPosition {
        ticker: "IE00XFAKEM1".to_owned(),
        name: "Money Fund".to_owned(),
        asset_type: rstock::models::AssetType::Fund,
        currency: "EUR".to_owned(),
        morningstar_code: Some("F000XFAKM1".to_owned()),
        asset_class: Some("monetary".to_owned()),
        equity_style: None,
        management: None,
        quantity: 1.0,
        average_cost: FactAvailability::Available(100.0),
        invested_cost: FactAvailability::Available(100.0),
        dividends: FactAvailability::Available(0.0),
        individual_price: FactAvailability::Available(rstock::models::IndividualPrice {
            native_price: 100.0,
            price_date: "2025-06-09".to_owned(),
        }),
        current_value: FactAvailability::Available(100.0),
        open_position_gain_loss: FactAvailability::Available(0.0),
        open_position_gain_loss_pct: FactAvailability::Available(0.0),
        market_data_limitations: vec![limitation],
    }
}

/// Populated view covering priced and unpriced holdings, mixed fact
/// availability, four limitation scopes, and one ready history point.
fn populated_view() -> PortfolioView {
    let nav_limitation = limitation("XFAKE1", "History Fund");
    let benchmark_limitation = limitation("ACWI", "Benchmark");
    let performance_section_limitation = limitation("XFAKE2", "Unpriced Fund");
    let monetary_section_limitation = limitation("XFAKEM1", "Money Fund");

    let not_applicable = FactAvailability::NotApplicable;
    PortfolioView {
        inventory: PortfolioInventory {
            base_currency: "EUR".to_owned(),
            performance_holdings: PortfolioInventorySection {
                positions: vec![
                    priced_position(),
                    unpriced_position(performance_section_limitation.clone()),
                ],
                aggregates: InventorySectionAggregates {
                    // One unvalued position makes the ordinary aggregate
                    // unavailable even though other position facts stay known.
                    current_value: FactAvailability::Unavailable,
                    invested_cost: FactAvailability::Available(56.0),
                    dividends: FactAvailability::Available(1.5),
                    open_position_gain_loss: FactAvailability::Unavailable,
                    open_position_gain_loss_pct: FactAvailability::Unavailable,
                },
                daily_priced_holdings_movement: DailyPricedHoldingsMovement {
                    movement: FactAvailability::Unavailable,
                    movement_pct: FactAvailability::Unavailable,
                    covered_prior_value: FactAvailability::Unavailable,
                    included_positions: vec![],
                    excluded_positions: vec![],
                    limitations: vec![],
                },
                market_data_limitations: vec![performance_section_limitation],
            },
            monetary_holdings: PortfolioInventorySection {
                positions: vec![monetary_position(monetary_section_limitation.clone())],
                aggregates: InventorySectionAggregates {
                    current_value: FactAvailability::Available(100.0),
                    invested_cost: FactAvailability::Available(100.0),
                    dividends: FactAvailability::Available(0.0),
                    open_position_gain_loss: FactAvailability::Available(0.0),
                    open_position_gain_loss_pct: FactAvailability::Available(0.0),
                },
                daily_priced_holdings_movement: DailyPricedHoldingsMovement {
                    movement: FactAvailability::NotApplicable,
                    movement_pct: FactAvailability::NotApplicable,
                    covered_prior_value: FactAvailability::NotApplicable,
                    included_positions: vec![],
                    excluded_positions: vec![],
                    limitations: vec![],
                },
                market_data_limitations: vec![monetary_section_limitation],
            },
            total_value: FactAvailability::Unavailable,
        },
        performance: PortfolioPerformance::Available(Box::new(
            rstock::models::AvailablePortfolioPerformance {
                effective_valuation_date: "2025-06-09".to_owned(),
                synchronized_value: 18.0,
                nav: 100.0,
                inception_date: "2025-06-01".to_owned(),
                periods: rstock::models::PerformancePeriods {
                    ytd: rstock::models::PeriodOutcome {
                        return_pct: FactAvailability::Available(2.0),
                        volatility: FactAvailability::Available(1.5),
                        max_drawdown: FactAvailability::Available(-3.0),
                        beta: FactAvailability::Unavailable,
                        sharpe: FactAvailability::Available(0.4),
                        sortino: not_applicable.clone(),
                    },
                    one_year: rstock::models::PeriodOutcome {
                        return_pct: not_applicable.clone(),
                        volatility: not_applicable.clone(),
                        max_drawdown: not_applicable.clone(),
                        beta: not_applicable.clone(),
                        sharpe: not_applicable.clone(),
                        sortino: not_applicable.clone(),
                    },
                    three_years: rstock::models::PeriodOutcome {
                        return_pct: FactAvailability::Available(12.5),
                        volatility: FactAvailability::Available(9.0),
                        max_drawdown: FactAvailability::Available(-15.0),
                        beta: FactAvailability::Available(1.1),
                        sharpe: FactAvailability::Available(0.9),
                        sortino: FactAvailability::Available(1.3),
                    },
                    five_years: rstock::models::PeriodOutcome {
                        return_pct: not_applicable.clone(),
                        volatility: not_applicable.clone(),
                        max_drawdown: not_applicable.clone(),
                        beta: not_applicable.clone(),
                        sharpe: not_applicable.clone(),
                        sortino: not_applicable.clone(),
                    },
                    all: rstock::models::PeriodOutcome {
                        return_pct: FactAvailability::Available(8.0),
                        volatility: FactAvailability::Available(10.0),
                        max_drawdown: FactAvailability::Available(-20.0),
                        beta: FactAvailability::Available(0.9),
                        sharpe: FactAvailability::Available(0.6),
                        sortino: FactAvailability::Available(0.8),
                    },
                },
                nav_history_limitations: vec![nav_limitation],
                benchmark_risk_limitations: vec![benchmark_limitation],
            },
        )),
        nav_history: vec![rstock::models::PortfolioSnapshot {
            date: "2025-06-09".to_owned(),
            asset_value: 18.0,
            total_value: 18.0,
            outstanding_shares: 0.18,
            nav: 100.0,
        }],
    }
}

/// Blocked initial NAV: performance is unavailable and carries only the
/// NAV/history readiness limitations; no synchronized facts are invented.
#[test]
fn portfolio_json_renders_unavailable_performance_with_readiness_limitations() {
    let nav_limitation = limitation("XFAKE1", "Blocked Stock");
    let mut view = empty_view();
    view.performance = PortfolioPerformance::Unavailable {
        nav_history_limitations: vec![nav_limitation],
    };

    let value = write_json(&view);
    let performance = &value["data"]["performance"];
    assert_eq!(performance["state"], "unavailable");
    assert!(performance.get("nav").is_none());
    assert!(performance.get("synchronized_value").is_none());
    assert!(performance.get("periods").is_none());
    assert_eq!(
        performance["nav_history_limitations"][0]["subject"]["ticker"],
        "XFAKE1"
    );
}

// --- Human adapter ---

use rstock::models::NavHistoryRequest;

fn write_human(view: &PortfolioView) -> String {
    let mut output = Vec::new();
    adapters::write_portfolio_human(&mut output, view, NavHistoryRequest::All)
        .expect("human adapter should write the outcome");
    String::from_utf8(output).expect("human output should be UTF-8")
}

#[test]
fn human_adapter_propagates_writer_failures() {
    let error =
        adapters::write_portfolio_human(&mut FailingWriter, &empty_view(), NavHistoryRequest::All)
            .expect_err("writer failure should be returned to the caller");

    assert!(format!("{error:#}").contains("simulated writer failure"));
}

#[test]
fn portfolio_human_marks_empty_inventory_and_not_applicable_performance() {
    let text = write_human(&empty_view());

    assert!(text.contains("No positions found."));
    assert!(text.contains("Performance: not applicable"));
    assert!(text.contains("Not enough data to display NAV chart."));
}

#[test]
fn portfolio_human_presents_sections_synchronized_facts_and_limitation_scopes() {
    let text = write_human(&populated_view());

    // Both inventory sections are presented under distinct headings with
    // their own aggregates.
    assert!(text.contains("Performance holdings:"));
    assert!(text.contains("Monetary holdings:"));
    assert!(text.contains(
        "Daily-priced holdings movement in EUR (covered inventory, not Portfolio performance):"
    ));
    let aggregate_lines: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("Invested:"))
        .collect();
    assert_eq!(
        aggregate_lines.len(),
        2,
        "both sections present their aggregates: {text}"
    );
    assert!(
        !aggregate_lines[0].contains("as of"),
        "mixed-date inventory aggregates must not carry the valuation date"
    );
    assert!(text.contains("Total value:"));

    // Available performance: synchronized facts under distinct labels, with
    // the Effective valuation date associated only with them.
    let dated_lines: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("(as of 09-06-2025)"))
        .collect();
    assert_eq!(
        dated_lines.len(),
        2,
        "only the synchronized performance facts carry the valuation date: {text}"
    );
    assert!(dated_lines
        .iter()
        .all(|line| line.contains("Performance value:") || line.contains("NAV:")));
    assert!(text.contains("Inception:"));

    // Available zero stays numeric while absent states keep distinct labels.
    let sortino_line = line_containing(&text, "Sortino", "").expect("Sortino row should exist");
    assert!(sortino_line.contains("not applicable"), "{sortino_line}");
    let beta_line = line_containing(&text, "Beta", "").expect("Beta row should exist");
    assert!(beta_line.contains("unavailable"), "{beta_line}");
    // The money fund's zero lifetime dividends present as the numeric zero.
    assert!(aggregate_lines[1].contains("0,00"), "{text}");

    // All four limitation scopes print under distinct, present-tense labels.
    let scopes = [
        "NAV/history market data limitations:",
        "Benchmark risk market data limitations:",
        "Performance-holdings market data limitations:",
        "Monetary-holdings market data limitations:",
    ];
    for scope in scopes {
        assert!(
            text.contains(scope),
            "missing scope heading: {scope}\n{text}"
        );
    }
    // The NAV/history scope carries the readiness limitation, the benchmark
    // scope the beta one, and the inventory scopes their holdings' ones.
    let nav_history_scope = section_after(&text, "NAV/history market data limitations:")
        .expect("NAV/history limitation entries should follow the heading");
    assert!(nav_history_scope.contains("XFAKE1"));
    let benchmark_scope = section_after(&text, "Benchmark risk market data limitations:")
        .expect("benchmark limitation entries should follow the heading");
    assert!(benchmark_scope.contains("ACWI"));
    let performance_scope = section_after(&text, "Performance-holdings market data limitations:")
        .expect("performance-holdings limitation entries should follow the heading");
    assert!(performance_scope.contains("XFAKE2"));
    let monetary_scope = section_after(&text, "Monetary-holdings market data limitations:")
        .expect("monetary limitation entries should follow the heading");
    assert!(monetary_scope.contains("XFAKEM1"));
}

#[test]
fn portfolio_human_marks_unavailable_performance_without_measured_facts() {
    let mut view = empty_view();
    view.performance = PortfolioPerformance::Unavailable {
        nav_history_limitations: vec![limitation("XFAKE1", "Blocked Stock")],
    };
    let text = write_human(&view);

    assert!(text.contains("Performance: unavailable"));
    assert!(
        !text.contains("not applicable"),
        "unavailable state must not borrow the not-applicable label: {text}"
    );
    assert!(text.contains("NAV/history market data limitations:"));
    // No synchronized facts, metadata, or period table may appear.
    assert!(!text.contains("NAV:"));
    assert!(!text.contains("Inception:"));
}

/// The first line containing `needle` and, when `excluded` is non-empty, not
/// containing it.
fn line_containing<'a>(text: &'a str, needle: &str, excluded: &str) -> Option<&'a str> {
    text.lines()
        .find(|line| line.contains(needle) && (excluded.is_empty() || !line.contains(excluded)))
}

/// The text between one heading and the next top-level line.
fn section_after<'a>(text: &'a str, heading: &str) -> Option<&'a str> {
    let start = text.split(heading).nth(1)?;
    let end = start.find("\n\n").unwrap_or(start.len());
    Some(&start[..end])
}
