//! JSON output Adapter for the Portfolio view.
//!
//! Maps one complete, presentation-neutral [`PortfolioView`] onto a nested
//! inventory/performance schema and writes it as one compact envelope through
//! the injected writer. Unavailable and not-applicable calculated scalar facts
//! both map to `null`, while an available zero stays numeric zero. Raw dates
//! and values are written without tables, prose, color, or ANSI bytes.

use std::io::Write;

use serde::Serialize;

use crate::cli::output::write_json;
use crate::models::{
    FactAvailability, IndividualPrice, InventoryPosition, MarketDataLimitation,
    PortfolioInventorySection, PortfolioPerformance, PortfolioSnapshot, PortfolioView,
};

/// Maps the composed Portfolio view onto the nested JSON schema and writes one
/// compact envelope through the injected writer. Positions are ordered by
/// current value with unvalued positions last; ordering is an Adapter choice.
pub fn write_portfolio_json<W: Write>(writer: &mut W, view: &PortfolioView) -> anyhow::Result<()> {
    write_json(writer, "portfolio.get", &PortfolioViewJson::from(view))
}

#[derive(Serialize)]
struct PortfolioViewJson<'a> {
    base_currency: &'a str,
    inventory: InventoryJson<'a>,
    performance: PerformanceJson<'a>,
    nav_history: Vec<NavSnapshotJson>,
}

impl<'a> From<&'a PortfolioView> for PortfolioViewJson<'a> {
    fn from(view: &'a PortfolioView) -> Self {
        Self {
            base_currency: &view.inventory.base_currency,
            inventory: InventoryJson {
                performance_holdings: PortfolioInventorySectionJson::from(
                    &view.inventory.performance_holdings,
                ),
                monetary_holdings: PortfolioInventorySectionJson::from(
                    &view.inventory.monetary_holdings,
                ),
                total_value: scalar_fact(&view.inventory.total_value),
            },
            performance: PerformanceJson::from(&view.performance),
            nav_history: view.nav_history.iter().map(NavSnapshotJson::from).collect(),
        }
    }
}

#[derive(Serialize)]
struct InventoryJson<'a> {
    performance_holdings: PortfolioInventorySectionJson<'a>,
    monetary_holdings: PortfolioInventorySectionJson<'a>,
    /// Combined informational current estimate across both sections; it does
    /// not participate in performance measurement.
    total_value: Option<f64>,
}

#[derive(Serialize)]
struct PortfolioInventorySectionJson<'a> {
    positions: Vec<PositionJson<'a>>,
    aggregates: AggregatesJson,
    market_data_limitations: Vec<&'a MarketDataLimitation>,
}

impl<'a> From<&'a PortfolioInventorySection> for PortfolioInventorySectionJson<'a> {
    fn from(section: &'a PortfolioInventorySection) -> Self {
        let mut position_refs: Vec<&InventoryPosition> = section.positions.iter().collect();
        super::sort_position_refs_by_current_value(&mut position_refs);
        Self {
            positions: position_refs
                .iter()
                .map(|position| PositionJson::from(*position))
                .collect(),
            aggregates: AggregatesJson::from(&section.aggregates),
            market_data_limitations: section.market_data_limitations.iter().collect(),
        }
    }
}

#[derive(Serialize)]
struct AggregatesJson {
    current_value: Option<f64>,
    invested_cost: Option<f64>,
    dividends: Option<f64>,
    open_position_gain_loss: Option<f64>,
    open_position_gain_loss_pct: Option<f64>,
}

impl From<&crate::models::InventorySectionAggregates> for AggregatesJson {
    fn from(aggregates: &crate::models::InventorySectionAggregates) -> Self {
        Self {
            current_value: scalar_fact(&aggregates.current_value),
            invested_cost: scalar_fact(&aggregates.invested_cost),
            dividends: scalar_fact(&aggregates.dividends),
            open_position_gain_loss: scalar_fact(&aggregates.open_position_gain_loss),
            open_position_gain_loss_pct: scalar_fact(&aggregates.open_position_gain_loss_pct),
        }
    }
}

#[derive(Serialize)]
struct PositionJson<'a> {
    ticker: &'a str,
    name: &'a str,
    asset_type: &'a crate::models::AssetType,
    currency: &'a str,
    morningstar_code: Option<&'a str>,
    asset_class: Option<&'a str>,
    equity_style: Option<&'a str>,
    management: Option<&'a str>,
    quantity: f64,
    average_cost: Option<f64>,
    invested_cost: Option<f64>,
    dividends: Option<f64>,
    individual_price: Option<IndividualPriceJson<'a>>,
    current_value: Option<f64>,
    open_position_gain_loss: Option<f64>,
    open_position_gain_loss_pct: Option<f64>,
    market_data_limitations: Vec<&'a MarketDataLimitation>,
}

impl<'a> From<&'a InventoryPosition> for PositionJson<'a> {
    fn from(position: &'a InventoryPosition) -> Self {
        Self {
            ticker: &position.ticker,
            name: &position.name,
            asset_type: &position.asset_type,
            currency: &position.currency,
            morningstar_code: position.morningstar_code.as_deref(),
            asset_class: position.asset_class.as_deref(),
            equity_style: position.equity_style.as_deref(),
            management: position.management.as_deref(),
            quantity: position.quantity,
            average_cost: scalar_fact(&position.average_cost),
            invested_cost: scalar_fact(&position.invested_cost),
            dividends: scalar_fact(&position.dividends),
            individual_price: IndividualPriceJson::from_fact(&position.individual_price),
            current_value: scalar_fact(&position.current_value),
            open_position_gain_loss: scalar_fact(&position.open_position_gain_loss),
            open_position_gain_loss_pct: scalar_fact(&position.open_position_gain_loss_pct),
            market_data_limitations: position.market_data_limitations.iter().collect(),
        }
    }
}

#[derive(Serialize)]
struct IndividualPriceJson<'a> {
    native_price: f64,
    price_date: &'a str,
}

impl<'a> IndividualPriceJson<'a> {
    /// The Individual price fact with its observation date, or `null` when the
    /// price is unavailable.
    fn from_fact(fact: &'a FactAvailability<IndividualPrice>) -> Option<Self> {
        match fact {
            FactAvailability::Available(price) => Some(Self {
                native_price: price.native_price,
                price_date: &price.price_date,
            }),
            FactAvailability::Unavailable | FactAvailability::NotApplicable => None,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum PerformanceJson<'a> {
    Available {
        effective_valuation_date: String,
        /// Performance-holdings value synchronized with the Effective
        /// valuation date; it excludes Monetary holdings and stays distinct
        /// from the mixed-date inventory values.
        synchronized_value: f64,
        nav: f64,
        inception_date: String,
        /// Boxed because the five period outcomes dwarf the enum's other
        /// variants; serialization is unchanged.
        periods: Box<PeriodsJson>,
        nav_history_limitations: Vec<&'a MarketDataLimitation>,
        benchmark_risk_limitations: Vec<&'a MarketDataLimitation>,
    },
    Unavailable {
        nav_history_limitations: Vec<&'a MarketDataLimitation>,
    },
    NotApplicable,
}

impl<'a> From<&'a PortfolioPerformance> for PerformanceJson<'a> {
    fn from(performance: &'a PortfolioPerformance) -> Self {
        match performance {
            PortfolioPerformance::Available(available) => Self::Available {
                effective_valuation_date: available.effective_valuation_date.clone(),
                synchronized_value: available.synchronized_value,
                nav: available.nav,
                inception_date: available.inception_date.clone(),
                periods: Box::new(PeriodsJson::from(&available.periods)),
                nav_history_limitations: available.nav_history_limitations.iter().collect(),
                benchmark_risk_limitations: available.benchmark_risk_limitations.iter().collect(),
            },
            PortfolioPerformance::Unavailable {
                nav_history_limitations,
            } => Self::Unavailable {
                nav_history_limitations: nav_history_limitations.iter().collect(),
            },
            PortfolioPerformance::NotApplicable => Self::NotApplicable,
        }
    }
}

#[derive(Serialize)]
struct PeriodsJson {
    ytd: PeriodOutcomeJson,
    one_year: PeriodOutcomeJson,
    three_years: PeriodOutcomeJson,
    five_years: PeriodOutcomeJson,
    /// Since inception.
    all: PeriodOutcomeJson,
}

impl From<&crate::models::PerformancePeriods> for PeriodsJson {
    fn from(periods: &crate::models::PerformancePeriods) -> Self {
        Self {
            ytd: PeriodOutcomeJson::from(&periods.ytd),
            one_year: PeriodOutcomeJson::from(&periods.one_year),
            three_years: PeriodOutcomeJson::from(&periods.three_years),
            five_years: PeriodOutcomeJson::from(&periods.five_years),
            all: PeriodOutcomeJson::from(&periods.all),
        }
    }
}

/// One period's cohesive return and risk facts. Unavailable and
/// not-applicable facts both map to `null`; only an available value remains
/// numeric.
#[derive(Serialize)]
struct PeriodOutcomeJson {
    return_pct: Option<f64>,
    volatility: Option<f64>,
    max_drawdown: Option<f64>,
    beta: Option<f64>,
    sharpe: Option<f64>,
    sortino: Option<f64>,
}

impl From<&crate::models::PeriodOutcome> for PeriodOutcomeJson {
    fn from(outcome: &crate::models::PeriodOutcome) -> Self {
        Self {
            return_pct: scalar_fact(&outcome.return_pct),
            volatility: scalar_fact(&outcome.volatility),
            max_drawdown: scalar_fact(&outcome.max_drawdown),
            beta: scalar_fact(&outcome.beta),
            sharpe: scalar_fact(&outcome.sharpe),
            sortino: scalar_fact(&outcome.sortino),
        }
    }
}

#[derive(Serialize)]
struct NavSnapshotJson {
    date: String,
    asset_value: f64,
    total_value: f64,
    outstanding_shares: f64,
    nav: f64,
}

impl From<&PortfolioSnapshot> for NavSnapshotJson {
    fn from(snapshot: &PortfolioSnapshot) -> Self {
        Self {
            date: snapshot.date.clone(),
            asset_value: snapshot.asset_value,
            total_value: snapshot.total_value,
            outstanding_shares: snapshot.outstanding_shares,
            nav: snapshot.nav,
        }
    }
}

/// Maps one calculated scalar fact: an available value keeps its value, while
/// unavailable and not-applicable states both collapse to `null` for JSON
/// consumers. The human Adapter retains the distinction between the two
/// absent states instead.
fn scalar_fact<T: Copy>(fact: &FactAvailability<T>) -> Option<T> {
    fact.value().copied()
}
