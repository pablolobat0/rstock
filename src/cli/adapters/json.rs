//! JSON output Adapter for the Portfolio view.
//!
//! Maps one complete, presentation-neutral [`PortfolioView`] onto a nested
//! inventory/performance schema and writes it as one compact envelope through
//! the injected writer. Unavailable and not-applicable calculated scalar facts
//! both map to `null`, while an available zero stays numeric zero. Raw dates
//! and values are written without tables, prose, color, or ANSI bytes.

use std::io::Write;

use chrono::NaiveDate;
use serde::Serialize;

use crate::cli::output::write_json;
use crate::models::{
    BenchmarkComparisonPeriod, BenchmarkPeriod, BlendComponent, ComparisonAvailability,
    ComparisonFact, DailyMovementCoverage, DailyPricedHoldingsMovement, DatedWealthPoint,
    ExposureBenchmarkComparison, ExposureBenchmarkReference, FactAvailability, IndividualPrice,
    InventoryPosition, MarketDataLimitation, MarketDataLimitationClassification, MarketDataSubject,
    OmittedSeedInterval, PortfolioInventorySection, PortfolioPerformance, PortfolioSnapshot,
    PortfolioView, StrategyComparison,
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
    /// The exposure-matched primary benchmark with its availability, aligned
    /// subject/reference series, standard rows, and reference provenance.
    exposure_benchmark: ExposureBenchmarkJson<'a>,
    /// The explicitly selected Strategy benchmark with its own availability,
    /// aligned subject/reference series, standard rows, and the selected
    /// definition's provenance.
    strategy_benchmark: StrategyBenchmarkJson<'a>,
}

/// The selected Strategy benchmark mapped onto the Adapter's own schema.
///
/// The Adapter owns this mapping: the domain outcome is not serialized
/// directly, so the JSON contract stays an explicit Adapter decision. The
/// section keeps its own role and availability, so a Strategy comparison is
/// never confused with the exposure-matched primary comparison.
#[derive(Serialize)]
struct StrategyBenchmarkJson<'a> {
    availability: ComparisonAvailabilityJson,
    role: &'a str,
    method: &'a str,
    subject_label: &'a str,
    reference_label: &'a str,
    currency: &'a str,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
    /// The anchor of the Strategy benchmark itself, before it is aligned with
    /// the portfolio.
    strategy_anchor_date: Option<&'a str>,
    /// The periods the Strategy benchmark cannot cover.
    earlier_period_limitation: Option<&'a str>,
    reset_rule: Option<&'a str>,
    weight_convention: Option<&'a str>,
    cash_convention: Option<&'a str>,
    components: Vec<BlendComponentJson<'a>>,
    definition_kind: Option<&'a str>,
    selected_at: Option<&'a str>,
    selection_basis: Option<&'a str>,
    subject_return_pct: Option<f64>,
    reference_return_pct: Option<f64>,
    excess_return_percentage_points: Option<f64>,
    subject_wealth: Vec<WealthPointJson<'a>>,
    reference_wealth: Vec<WealthPointJson<'a>>,
    standard_periods: Vec<BenchmarkPeriodJson<'a>>,
    limitations: Vec<MarketDataLimitationJson<'a>>,
}

impl<'a> From<&'a StrategyComparison> for StrategyBenchmarkJson<'a> {
    fn from(comparison: &'a StrategyComparison) -> Self {
        Self {
            availability: ComparisonAvailabilityJson::from(&comparison.availability),
            role: &comparison.role,
            method: &comparison.method,
            subject_label: &comparison.subject_label,
            reference_label: &comparison.reference_label,
            currency: &comparison.currency,
            start_date: comparison.start_date.as_deref(),
            end_date: comparison.end_date.as_deref(),
            strategy_anchor_date: comparison.strategy_anchor_date.as_deref(),
            earlier_period_limitation: comparison.earlier_period_limitation.as_deref(),
            reset_rule: comparison.reset_rule.as_deref(),
            weight_convention: comparison.weight_convention.as_deref(),
            cash_convention: comparison.cash_convention.as_deref(),
            components: comparison
                .components
                .iter()
                .map(BlendComponentJson::from)
                .collect(),
            definition_kind: comparison.definition_kind.as_deref(),
            selected_at: comparison.selected_at.as_deref(),
            selection_basis: comparison.selection_basis.as_deref(),
            subject_return_pct: comparison.subject_return_pct,
            reference_return_pct: comparison.reference_return_pct,
            excess_return_percentage_points: comparison.excess_return_percentage_points,
            subject_wealth: comparison
                .subject_wealth
                .iter()
                .map(WealthPointJson::from)
                .collect(),
            reference_wealth: comparison
                .reference_wealth
                .iter()
                .map(WealthPointJson::from)
                .collect(),
            standard_periods: comparison
                .standard_periods
                .iter()
                .map(BenchmarkPeriodJson::from)
                .collect(),
            limitations: comparison
                .limitations
                .iter()
                .map(MarketDataLimitationJson::from)
                .collect(),
        }
    }
}

/// One component of the selected Strategy benchmark, with the conventions it
/// was admitted with.
#[derive(Serialize)]
struct BlendComponentJson<'a> {
    component_key: &'a str,
    component_reference_id: Option<&'a str>,
    display_name: &'a str,
    target_weight: f64,
    target_weight_percent: f64,
    is_cash: bool,
    currency: &'a str,
    return_basis: &'a str,
    adjustment: &'a str,
    hedge_policy: &'a str,
    universe: &'a str,
    source: &'a str,
    source_identifier: &'a str,
    cadence: &'a str,
    source_provenance: &'a str,
    verification_state: &'a str,
}

impl<'a> From<&'a BlendComponent> for BlendComponentJson<'a> {
    fn from(component: &'a BlendComponent) -> Self {
        Self {
            component_key: &component.component_key,
            component_reference_id: component.component_reference_id.as_deref(),
            display_name: &component.display_name,
            target_weight: component.target_weight,
            target_weight_percent: component.target_weight_percent,
            is_cash: component.is_cash,
            currency: &component.currency,
            return_basis: &component.return_basis,
            adjustment: &component.adjustment,
            hedge_policy: &component.hedge_policy,
            universe: &component.universe,
            source: &component.source,
            source_identifier: &component.source_identifier,
            cadence: &component.cadence,
            source_provenance: &component.source_provenance,
            verification_state: &component.verification_state,
        }
    }
}

/// The exposure-matched benchmark mapped onto the Adapter's own schema.
///
/// The Adapter owns this mapping: the domain outcome is not serialized
/// directly, so the JSON contract stays an explicit Adapter decision.
#[derive(Serialize)]
struct ExposureBenchmarkJson<'a> {
    availability: ComparisonAvailabilityJson,
    role: &'a str,
    method: &'a str,
    subject_label: &'a str,
    reference_label: &'a str,
    currency: &'a str,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
    inception_date: Option<&'a str>,
    omitted_seed_interval: Option<OmittedSeedIntervalJson<'a>>,
    retrospective: bool,
    subject_return_pct: Option<f64>,
    reference_return_pct: Option<f64>,
    excess_return_percentage_points: Option<f64>,
    subject_wealth: Vec<WealthPointJson<'a>>,
    reference_wealth: Vec<WealthPointJson<'a>>,
    standard_periods: Vec<BenchmarkPeriodJson<'a>>,
    references: Vec<ExposureBenchmarkReferenceJson<'a>>,
    reference_limitations: Vec<MarketDataLimitationJson<'a>>,
}

impl<'a> From<&'a ExposureBenchmarkComparison> for ExposureBenchmarkJson<'a> {
    fn from(comparison: &'a ExposureBenchmarkComparison) -> Self {
        Self {
            availability: ComparisonAvailabilityJson::from(&comparison.availability),
            role: &comparison.role,
            method: &comparison.method,
            subject_label: &comparison.subject_label,
            reference_label: &comparison.reference_label,
            currency: &comparison.currency,
            start_date: comparison.start_date.as_deref(),
            end_date: comparison.end_date.as_deref(),
            inception_date: comparison.inception_date.as_deref(),
            omitted_seed_interval: comparison
                .omitted_seed_interval
                .as_ref()
                .map(OmittedSeedIntervalJson::from),
            retrospective: comparison.retrospective,
            subject_return_pct: comparison.subject_return_pct,
            reference_return_pct: comparison.reference_return_pct,
            excess_return_percentage_points: comparison.excess_return_percentage_points,
            subject_wealth: comparison
                .subject_wealth
                .iter()
                .map(WealthPointJson::from)
                .collect(),
            reference_wealth: comparison
                .reference_wealth
                .iter()
                .map(WealthPointJson::from)
                .collect(),
            standard_periods: comparison
                .standard_periods
                .iter()
                .map(BenchmarkPeriodJson::from)
                .collect(),
            references: comparison
                .references
                .iter()
                .map(ExposureBenchmarkReferenceJson::from)
                .collect(),
            reference_limitations: comparison
                .reference_limitations
                .iter()
                .map(MarketDataLimitationJson::from)
                .collect(),
        }
    }
}

/// One contributing reference mapped onto the Adapter's own schema.
#[derive(Serialize)]
struct ExposureBenchmarkReferenceJson<'a> {
    reference_id: &'a str,
    display_name: &'a str,
    assets: &'a [String],
    first_weight_pct: Option<f64>,
    last_weight_pct: Option<f64>,
    max_weight_pct: Option<f64>,
    native_currency: &'a str,
    comparison_currency: &'a str,
    return_basis: &'a str,
    adjustment: &'a str,
    hedge_policy: &'a str,
    cadence: Option<&'a str>,
    source: &'a str,
    source_identifier: &'a str,
    source_provenance: &'a str,
    verification_state: &'a str,
    assigned_at: Option<&'a str>,
    limitations: Vec<MarketDataLimitationJson<'a>>,
}

impl<'a> From<&'a ExposureBenchmarkReference> for ExposureBenchmarkReferenceJson<'a> {
    fn from(reference: &'a ExposureBenchmarkReference) -> Self {
        Self {
            reference_id: &reference.reference_id,
            display_name: &reference.display_name,
            assets: &reference.assets,
            first_weight_pct: reference.first_weight_pct,
            last_weight_pct: reference.last_weight_pct,
            max_weight_pct: reference.max_weight_pct,
            native_currency: &reference.native_currency,
            comparison_currency: &reference.comparison_currency,
            return_basis: &reference.return_basis,
            adjustment: &reference.adjustment,
            hedge_policy: &reference.hedge_policy,
            cadence: reference.cadence.as_deref(),
            source: &reference.source,
            source_identifier: &reference.source_identifier,
            source_provenance: &reference.source_provenance,
            verification_state: &reference.verification_state,
            assigned_at: reference.assigned_at.as_deref(),
            limitations: reference
                .limitations
                .iter()
                .map(MarketDataLimitationJson::from)
                .collect(),
        }
    }
}

/// The omitted seed-to-first-close interval mapped onto the Adapter's schema.
#[derive(Serialize)]
struct OmittedSeedIntervalJson<'a> {
    seed_date: &'a str,
    anchor_date: &'a str,
}

impl<'a> From<&'a OmittedSeedInterval> for OmittedSeedIntervalJson<'a> {
    fn from(interval: &'a OmittedSeedInterval) -> Self {
        Self {
            seed_date: &interval.seed_date,
            anchor_date: &interval.anchor_date,
        }
    }
}

/// One dated point of an aligned wealth series.
#[derive(Serialize)]
struct WealthPointJson<'a> {
    date: &'a str,
    wealth: f64,
}

impl<'a> From<&'a DatedWealthPoint> for WealthPointJson<'a> {
    fn from(point: &'a DatedWealthPoint) -> Self {
        Self {
            date: &point.date,
            wealth: point.wealth,
        }
    }
}

/// One standard comparison period mapped onto the Adapter's own schema.
#[derive(Serialize)]
struct BenchmarkPeriodJson<'a> {
    period: BenchmarkComparisonPeriod,
    requested_start_date: Option<&'a str>,
    start_date: Option<&'a str>,
    end_date: Option<&'a str>,
    subject_return_pct: ComparisonFactJson,
    reference_return_pct: ComparisonFactJson,
    excess_return_percentage_points: ComparisonFactJson,
    beta: ComparisonFactJson,
    correlation: ComparisonFactJson,
    tracking_error_annualized_pct: ComparisonFactJson,
    information_ratio: ComparisonFactJson,
}

impl<'a> From<&'a BenchmarkPeriod> for BenchmarkPeriodJson<'a> {
    fn from(period: &'a BenchmarkPeriod) -> Self {
        Self {
            period: period.period,
            requested_start_date: period.requested_start_date.as_deref(),
            start_date: period.start_date.as_deref(),
            end_date: period.end_date.as_deref(),
            subject_return_pct: ComparisonFactJson::from(&period.subject_return_pct),
            reference_return_pct: ComparisonFactJson::from(&period.reference_return_pct),
            excess_return_percentage_points: ComparisonFactJson::from(
                &period.excess_return_percentage_points,
            ),
            beta: ComparisonFactJson::from(&period.beta),
            correlation: ComparisonFactJson::from(&period.correlation),
            tracking_error_annualized_pct: ComparisonFactJson::from(
                &period.tracking_error_annualized_pct,
            ),
            information_ratio: ComparisonFactJson::from(&period.information_ratio),
        }
    }
}

/// The availability of one comparison outcome.
#[derive(Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
enum ComparisonAvailabilityJson {
    Available,
    Unavailable { reason: String },
    NotApplicable { reason: String },
}

impl<'a> From<&'a ComparisonAvailability> for ComparisonAvailabilityJson {
    fn from(availability: &'a ComparisonAvailability) -> Self {
        match availability {
            ComparisonAvailability::Available => Self::Available,
            ComparisonAvailability::Unavailable { reason } => Self::Unavailable {
                reason: reason.clone(),
            },
            ComparisonAvailability::NotApplicable { reason } => Self::NotApplicable {
                reason: reason.clone(),
            },
        }
    }
}

/// One calculated comparison fact.
#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
enum ComparisonFactJson {
    Available { value: f64 },
    Unavailable { reason: String },
    NotApplicable { reason: String },
}

impl<'a> From<&'a ComparisonFact<f64>> for ComparisonFactJson {
    fn from(fact: &'a ComparisonFact<f64>) -> Self {
        match fact {
            ComparisonFact::Available { value } => Self::Available { value: *value },
            ComparisonFact::Unavailable { reason } => Self::Unavailable {
                reason: reason.clone(),
            },
            ComparisonFact::NotApplicable { reason } => Self::NotApplicable {
                reason: reason.clone(),
            },
        }
    }
}

/// One Market data limitation mapped onto the Adapter's own schema.
#[derive(Serialize)]
struct MarketDataLimitationJson<'a> {
    subject: MarketDataSubjectJson<'a>,
    latest_available_date: Option<NaiveDate>,
    requested_end_date: NaiveDate,
    classification: &'static str,
}

impl<'a> From<&'a MarketDataLimitation> for MarketDataLimitationJson<'a> {
    fn from(limitation: &'a MarketDataLimitation) -> Self {
        Self {
            subject: MarketDataSubjectJson::from(&limitation.subject),
            latest_available_date: limitation.latest_available_date,
            requested_end_date: limitation.requested_end_date,
            classification: classification_label(&limitation.classification),
        }
    }
}

/// The classified subject of one Market data limitation.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum MarketDataSubjectJson<'a> {
    Asset {
        ticker: &'a str,
        name: &'a str,
        asset_type: String,
    },
    BenchmarkReference {
        id: &'a str,
        display_name: &'a str,
    },
    FxRate {
        currency: &'a str,
    },
}

impl<'a> From<&'a MarketDataSubject> for MarketDataSubjectJson<'a> {
    fn from(subject: &'a MarketDataSubject) -> Self {
        match subject {
            MarketDataSubject::Asset {
                ticker,
                name,
                asset_type,
            } => Self::Asset {
                ticker,
                name,
                asset_type: asset_type.to_string(),
            },
            MarketDataSubject::BenchmarkReference { id, display_name } => {
                Self::BenchmarkReference { id, display_name }
            }
            MarketDataSubject::FxRate { currency } => Self::FxRate { currency },
        }
    }
}

/// The user-actionable classification of one Market data limitation.
fn classification_label(classification: &MarketDataLimitationClassification) -> &'static str {
    match classification {
        MarketDataLimitationClassification::ActionableMissingData => "actionable_missing_data",
        MarketDataLimitationClassification::ActionableReportingLag => "actionable_reporting_lag",
        MarketDataLimitationClassification::ActionableStaleData => "actionable_stale_data",
    }
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
            exposure_benchmark: ExposureBenchmarkJson::from(&view.exposure_benchmark),
            strategy_benchmark: StrategyBenchmarkJson::from(&view.strategy_benchmark),
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
    daily_priced_holdings_movement: DailyMovementJson<'a>,
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
            daily_priced_holdings_movement: DailyMovementJson::from(
                &section.daily_priced_holdings_movement,
            ),
            market_data_limitations: section.market_data_limitations.iter().collect(),
        }
    }
}

#[derive(Serialize)]
struct DailyMovementJson<'a> {
    movement: Option<f64>,
    movement_availability: &'static str,
    movement_pct: Option<f64>,
    movement_pct_availability: &'static str,
    covered_prior_value: Option<f64>,
    covered_prior_value_availability: &'static str,
    included_positions: Vec<DailyMovementCoverageJson<'a>>,
    excluded_positions: Vec<DailyMovementCoverageJson<'a>>,
    limitations: Vec<&'a MarketDataLimitation>,
}

impl<'a> From<&'a DailyPricedHoldingsMovement> for DailyMovementJson<'a> {
    fn from(value: &'a DailyPricedHoldingsMovement) -> Self {
        let (movement, movement_availability) = fact_json(&value.movement);
        let (movement_pct, movement_pct_availability) = fact_json(&value.movement_pct);
        let (covered_prior_value, covered_prior_value_availability) =
            fact_json(&value.covered_prior_value);
        Self {
            movement,
            movement_availability,
            movement_pct,
            movement_pct_availability,
            covered_prior_value,
            covered_prior_value_availability,
            included_positions: value
                .included_positions
                .iter()
                .map(DailyMovementCoverageJson::from)
                .collect(),
            excluded_positions: value
                .excluded_positions
                .iter()
                .map(DailyMovementCoverageJson::from)
                .collect(),
            limitations: value.limitations.iter().collect(),
        }
    }
}

#[derive(Serialize)]
struct DailyMovementCoverageJson<'a> {
    ticker: &'a str,
    baseline_date: Option<&'a str>,
    baseline_fx_date: Option<&'a str>,
    prior_base_currency_value: Option<f64>,
    current_base_currency_value: Option<f64>,
    exclusion_reason: Option<&'a str>,
}

impl<'a> From<&'a DailyMovementCoverage> for DailyMovementCoverageJson<'a> {
    fn from(coverage: &'a DailyMovementCoverage) -> Self {
        Self {
            ticker: &coverage.ticker,
            baseline_date: coverage.baseline_date.as_deref(),
            baseline_fx_date: coverage.baseline_fx_date.as_deref(),
            prior_base_currency_value: coverage.prior_base_currency_value,
            current_base_currency_value: coverage.current_base_currency_value,
            exclusion_reason: coverage
                .exclusion_reason
                .map(crate::models::DailyMovementExclusionReason::as_str),
        }
    }
}

fn fact_json(fact: &FactAvailability<f64>) -> (Option<f64>, &'static str) {
    match fact {
        FactAvailability::Available(value) => (Some(*value), "available"),
        FactAvailability::Unavailable => (None, "unavailable"),
        FactAvailability::NotApplicable => (None, "not_applicable"),
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
        benchmark_reference: &'a crate::models::BenchmarkReferenceInfo,
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
                benchmark_reference: &available.benchmark_reference,
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
