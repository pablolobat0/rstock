//! Saved Custom benchmark blends.
//!
//! A Custom blend is a flat, long-only, fully allocated target definition
//! whose wealth is computed from its own components: arithmetic
//! beginning-of-interval component returns that are compounded, with monthly
//! target resets and return-driven weight drift between resets. Definitions
//! are immutable — there is no edit operation and no automatic revision — and
//! a saved definition keeps the component conventions it was admitted with.
//!
//! The admission window is a real comparison boundary, never a fabricated
//! date: by default it is the portfolio's ready first invested Complete
//! closing snapshot through the portfolio's effective complete comparison
//! end, and an explicitly selected later start keeps its own complete window.
//! Requested boundaries are resolved to completed weekday closes by calendar
//! rule before any component availability is inspected, and every component
//! must hold the required anchor and end closes exactly.
//!
//! Market data acquisition and EUR conversion stay in `MarketData`; this
//! module only admits components, resolves their coverage, and calculates the
//! blend's own dated wealth.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::{
    benchmark_assignment_repo, benchmark_reference_repo, custom_blend_repo, portfolio_history_repo,
};
use crate::models::{
    BlendComponent, BlendComponentArgument, BlendTargetReset, BlendWealthPoint,
    CustomBlendDefinition, CustomBlendDeletion, CustomBlendPreview,
};
use crate::services::market_data::MarketData;

/// The reserved component identity of the synthetic cash component.
pub const CASH_COMPONENT_KEY: &str = "cash";

/// Absolute tolerance, in target-weight fraction, for the 100% weight sum.
pub const WEIGHT_TOLERANCE: f64 = 1e-4;

/// The monthly target reset rule of a saved Custom blend.
pub const RESET_RULE: &str = "monthly";

/// How the blend's component weights behave between resets.
pub const WEIGHT_CONVENTION: &str =
    "Beginning-of-interval target weights with return-driven drift between monthly resets";

/// How the synthetic cash component behaves.
pub const CASH_CONVENTION: &str =
    "Synthetic 0% return cash; no market data, no fees, and no transactions";

/// The admission basis of a blend admitted against the portfolio comparison history.
pub const ADMISSION_BASIS_FULL_HISTORY: &str = "full_portfolio_comparison_history";

/// The admission basis of a blend admitted from an explicitly selected start.
pub const ADMISSION_BASIS_EXPLICIT_START: &str = "explicit_later_start";

/// The accounting meaning of a monthly target reset.
pub const ACCOUNTING_NOTE: &str = "Monthly target resets are frictionless synthetic accounting: no transactions, fees, or rebalancing events are created";

/// The saved-definition kind of a Custom blend in the reference catalog.
pub const CUSTOM_BLEND_KIND: &str = "custom_blend";

/// The source identity of a Custom blend: it has no market data source.
const CUSTOM_BLEND_SOURCE: &str = "custom_blend";

/// One requested component of a Custom blend.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendComponentRequest {
    pub identity: String,
    pub weight: f64,
    /// The convention snapshot already recorded for this component, if the
    /// component comes from a saved definition.
    pub recorded: Option<BlendComponent>,
}

/// One admitted component with the aligned Base currency closes it uses.
#[derive(Clone, Debug, PartialEq)]
pub struct AdmittedComponent {
    pub component: BlendComponent,
    pub series: BTreeMap<NaiveDate, f64>,
    /// Whether the Base currency conversion of this component is incomplete.
    pub fx_conversion_incomplete: bool,
}

impl AdmittedComponent {
    /// The Base currency conversion note of this component.
    fn conversion_note(&self) -> &'static str {
        if self.fx_conversion_incomplete {
            " (its Base currency conversion is missing or incomplete)"
        } else {
            ""
        }
    }
}

/// The comparison window a blend is admitted against or previewed over.
///
/// The requested dates are the boundary the blend was asked for; the actual
/// dates are the completed weekday closes that boundary resolves to by
/// calendar rule. The actual dates are the required closes every component
/// must hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendWindow {
    pub requested_start: NaiveDate,
    pub requested_end: NaiveDate,
    pub actual_start: NaiveDate,
    pub actual_end: NaiveDate,
}

impl BlendWindow {
    /// Resolves a requested window to its required completed weekday closes.
    ///
    /// A weekend boundary resolves to the preceding completed weekday by
    /// calendar rule, before any component availability is inspected. The
    /// window is never advanced or shortened because of missing data.
    pub fn new(requested_start: NaiveDate, requested_end: NaiveDate) -> Self {
        Self {
            requested_start,
            requested_end,
            actual_start: resolve_completed_weekday(requested_start),
            actual_end: resolve_completed_weekday(requested_end),
        }
    }
}

/// The dated wealth series of one Custom blend.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendWealthSeries {
    pub anchor_date: String,
    pub last_observation_date: String,
    pub wealth: Vec<BlendWealthPoint>,
    pub target_resets: Vec<BlendTargetReset>,
    pub cumulative_return_pct: f64,
}

/// The request to save one new Custom blend.
#[derive(Clone, Debug, PartialEq)]
pub struct BlendCreateRequest {
    pub name: String,
    pub components: Vec<BlendComponentArgument>,
    pub start_date: Option<NaiveDate>,
}

/// The portfolio comparison boundary a blend is admitted against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortfolioComparisonBoundary {
    /// The first invested Complete closing snapshot of the portfolio.
    pub first_invested_date: String,
    /// The effective complete comparison end of the portfolio history.
    pub effective_end_date: String,
}

/// Saves one new immutable Custom blend.
///
/// The definition is admitted and persisted as a whole: a component that
/// cannot be admitted leaves no saved blend behind.
pub async fn create_custom_blend(
    db: &DatabaseConnection,
    market_data: &MarketData,
    request: BlendCreateRequest,
) -> anyhow::Result<CustomBlendDefinition> {
    let (blend_id, display_name) = blend_identity(&request.name)?;
    if benchmark_reference_repo::exists(db, &blend_id).await? {
        anyhow::bail!(
            "Custom blend '{blend_id}' already exists; saved blends are immutable, so a changed strategy is a new definition"
        );
    }
    let requested = requested_components(&request.components)?;
    let window = admission_window(db, market_data, request.start_date).await?;
    let admitted = admit_components(db, market_data, &requested, &window).await?;

    let definition = CustomBlendDefinition {
        id: blend_id.clone(),
        display_name: display_name.clone(),
        created_at: format!("{}T00:00:00Z", format_date(market_data.today())),
        currency: BASE_CURRENCY.to_owned(),
        reset_rule: RESET_RULE.to_owned(),
        weight_convention: WEIGHT_CONVENTION.to_owned(),
        cash_convention: CASH_CONVENTION.to_owned(),
        weight_tolerance: WEIGHT_TOLERANCE,
        requested_start_date: format_date(window.requested_start),
        requested_end_date: format_date(window.requested_end),
        admitted_start_date: format_date(window.actual_start),
        admitted_end_date: format_date(window.actual_end),
        admission_basis: admission_basis(request.start_date).to_owned(),
        components: admitted
            .iter()
            .map(|component| component.component.clone())
            .collect(),
    };

    let transaction = db.begin().await?;
    benchmark_reference_repo::insert_definition(&transaction, &catalog_definition(&definition))
        .await?;
    custom_blend_repo::insert_blend_attributes(&transaction, &definition).await?;
    custom_blend_repo::insert_components(&transaction, &blend_id, &definition.components).await?;
    transaction.commit().await?;
    custom_blend_repo::find_by_id(db, &blend_id).await
}

/// Every saved Custom blend, in a stable order.
pub async fn list_custom_blends(
    db: &DatabaseConnection,
) -> anyhow::Result<Vec<CustomBlendDefinition>> {
    custom_blend_repo::find_all(db).await
}

/// One saved Custom blend definition with its components.
pub async fn show_custom_blend(
    db: &DatabaseConnection,
    blend_id: &str,
) -> anyhow::Result<CustomBlendDefinition> {
    custom_blend_repo::find_by_id(db, blend_id).await
}

/// Previews the dated, rebased wealth series of one saved Custom blend.
///
/// The preview spans the blend's admitted start through the current completed
/// end, and validates that whole window exactly: a component without the
/// required anchor or end close, or with an internal weekday gap, is reported
/// instead of the preview silently shortening.
pub async fn preview_custom_blend(
    db: &DatabaseConnection,
    market_data: &MarketData,
    blend_id: &str,
) -> anyhow::Result<CustomBlendPreview> {
    let definition = custom_blend_repo::find_by_id(db, blend_id).await?;
    let start = parse_date(&definition.admitted_start_date)?;
    let end = market_data.today() - Duration::days(1);
    anyhow::ensure!(
        start <= end,
        "Custom blend '{blend_id}' starts on {} but no completed comparison date is available",
        definition.admitted_start_date
    );
    let window = BlendWindow::new(start, end);
    let requested = requested_components_from_definition(&definition)?;
    let admitted = admit_components(db, market_data, &requested, &window).await?;
    let series = calculate_blend_wealth(&admitted, &window)?;
    Ok(CustomBlendPreview {
        blend_id: definition.id.clone(),
        display_name: definition.display_name.clone(),
        currency: definition.currency.clone(),
        reset_rule: definition.reset_rule.clone(),
        weight_convention: definition.weight_convention.clone(),
        cash_convention: definition.cash_convention.clone(),
        weight_tolerance: definition.weight_tolerance,
        admitted_start_date: definition.admitted_start_date.clone(),
        admitted_end_date: definition.admitted_end_date.clone(),
        admission_basis: definition.admission_basis.clone(),
        components: definition.components.clone(),
        requested_start_date: format_date(window.requested_start),
        requested_end_date: format_date(window.requested_end),
        anchor_date: series.anchor_date.clone(),
        last_observation_date: series.last_observation_date.clone(),
        rebased_to: 100.0,
        cumulative_return_pct: series.cumulative_return_pct,
        wealth: series.wealth,
        target_resets: series.target_resets,
        earlier_period_limitation: earlier_period_limitation(&definition),
        accounting_note: ACCOUNTING_NOTE.to_owned(),
    })
}

/// Deletes one unused saved Custom blend definition.
///
/// The usage check and the deletion share one caller-owned transaction, so a
/// definition that is assigned cannot be deleted and a failed deletion leaves
/// the saved definition, its components, and every price, FX, ledger, and NAV
/// row unchanged.
pub async fn delete_custom_blend(
    db: &DatabaseConnection,
    blend_id: &str,
) -> anyhow::Result<CustomBlendDeletion> {
    let definition = custom_blend_repo::find_by_id(db, blend_id).await?;
    let transaction = db.begin().await?;
    let assignments = benchmark_assignment_repo::find_by_reference(&transaction, blend_id).await?;
    if !assignments.is_empty() {
        transaction.rollback().await?;
        anyhow::bail!(
            "Custom blend '{blend_id}' is in use by {} active assignment(s); remove or replace them before deleting the definition",
            assignments.len()
        );
    }
    let deleted = match custom_blend_repo::delete_blend(&transaction, blend_id).await {
        Ok(deleted) => deleted,
        Err(error) => {
            transaction.rollback().await?;
            return Err(anyhow::anyhow!(
                "Custom blend '{blend_id}' could not be deleted and remains saved: {error}"
            ));
        }
    };
    transaction.commit().await?;
    Ok(CustomBlendDeletion {
        blend_id: blend_id.to_owned(),
        display_name: definition.display_name,
        deleted,
        preserved_history_note:
            "Deleted definition only; component price, FX, ledger, and NAV history are preserved"
                .to_owned(),
    })
}

/// Calculates the dated, rebased wealth series of one blend.
///
/// Component returns are arithmetic beginning-of-interval returns and the
/// blend wealth compounds them. Target weights are initialized at the anchor
/// close, reset at the close immediately before each new calendar month, and
/// drift with component wealth between resets.
pub fn calculate_blend_wealth(
    components: &[AdmittedComponent],
    window: &BlendWindow,
) -> anyhow::Result<BlendWealthSeries> {
    anyhow::ensure!(
        !components.is_empty(),
        "a Custom blend needs at least one component"
    );
    anyhow::ensure!(
        components
            .iter()
            .any(|component| !component.component.is_cash),
        "a Custom blend needs at least one market component with verifiable history; cash alone has no comparable series"
    );
    let dates = aligned_dates(components, window);
    anyhow::ensure!(
        dates.len() >= 2,
        "a Custom blend needs at least two aligned observations; {} are available in the admitted window",
        dates.len()
    );

    let declared_total: f64 = components
        .iter()
        .map(|component| component.component.target_weight)
        .sum();
    anyhow::ensure!(
        declared_total > 0.0,
        "a Custom blend needs at least one component with a positive target weight"
    );
    let targets: Vec<f64> = components
        .iter()
        .map(|component| component.component.target_weight / declared_total)
        .collect();

    let mut state = BlendState {
        values: targets.iter().map(|target| target * 100.0).collect(),
        weights: targets.clone(),
        targets,
        wealth: 100.0,
    };
    let anchor_date = format_date(dates[0]);
    let mut points = vec![BlendWealthPoint {
        date: anchor_date.clone(),
        wealth: state.wealth,
    }];
    let mut target_resets = Vec::new();

    for pair in dates.windows(2) {
        let reset = apply_interval(&mut state, components, pair[0], pair[1])?;
        if let Some(reset) = reset {
            target_resets.push(reset);
        }
        points.push(BlendWealthPoint {
            date: format_date(pair[1]),
            wealth: state.wealth,
        });
    }

    let last_observation_date = points
        .last()
        .map(|point| point.date.clone())
        .unwrap_or_default();
    Ok(BlendWealthSeries {
        anchor_date,
        last_observation_date,
        cumulative_return_pct: (state.wealth / 100.0 - 1.0) * 100.0,
        wealth: points,
        target_resets,
    })
}

/// The running state of one blend: component values, drifted weights, wealth.
struct BlendState {
    values: Vec<f64>,
    weights: Vec<f64>,
    targets: Vec<f64>,
    wealth: f64,
}

/// Applies one interval to the blend state.
///
/// The monthly reset happens at the close immediately preceding the new
/// calendar month, before the first return interval of that month. The
/// interval itself uses the beginning-of-interval weights, and the weights
/// drift with component wealth afterwards.
fn apply_interval(
    state: &mut BlendState,
    components: &[AdmittedComponent],
    previous: NaiveDate,
    current: NaiveDate,
) -> anyhow::Result<Option<BlendTargetReset>> {
    let mut reset = None;
    if (current.year(), current.month()) != (previous.year(), previous.month()) {
        let total: f64 = state.values.iter().sum();
        for (index, value) in state.values.iter_mut().enumerate() {
            *value = state.targets[index] * total;
        }
        state.weights.clone_from_slice(&state.targets);
        reset = Some(BlendTargetReset {
            reset_date: format_date(previous),
            effective_from_date: format_date(current),
        });
    }
    let mut interval_return = 0.0;
    for (index, component) in components.iter().enumerate() {
        interval_return += state.weights[index] * component_return(component, previous, current)?;
    }
    state.wealth *= 1.0 + interval_return;
    for (index, component) in components.iter().enumerate() {
        let component_return = component_return(component, previous, current)?;
        state.values[index] *= 1.0 + component_return;
    }
    // Between resets the weights drift with component wealth.
    let total: f64 = state.values.iter().sum();
    for (index, value) in state.values.iter().enumerate() {
        state.weights[index] = value / total;
    }
    Ok(reset)
}

/// Admits every requested component over the window.
///
/// A component is admitted only when its definition is a verified,
/// non-nested reference whose qualified daily Base currency series holds the
/// required anchor and end closes and every interior weekday. A component
/// that cannot be admitted is reported; no component is omitted and no weight
/// is renormalized.
async fn admit_components(
    db: &DatabaseConnection,
    market_data: &MarketData,
    requested: &[BlendComponentRequest],
    window: &BlendWindow,
) -> anyhow::Result<Vec<AdmittedComponent>> {
    let mut admitted = Vec::with_capacity(requested.len());
    for request in requested {
        admitted.push(admit_component(db, market_data, request, window).await?);
    }
    require_window_coverage(&admitted, window)?;
    Ok(admitted)
}

/// Admits one requested component over the window.
async fn admit_component(
    db: &DatabaseConnection,
    market_data: &MarketData,
    request: &BlendComponentRequest,
    window: &BlendWindow,
) -> anyhow::Result<AdmittedComponent> {
    if request.identity == CASH_COMPONENT_KEY {
        return Ok(AdmittedComponent {
            component: request
                .recorded
                .clone()
                .unwrap_or_else(|| cash_component(request.weight)),
            series: BTreeMap::new(),
            fx_conversion_incomplete: false,
        });
    }
    let definition = resolve_component_definition(db, request).await?;
    let prepared = market_data
        .selected_benchmark_reference_market_data(
            db,
            &request.identity,
            &format_date(window.actual_start),
            &format_date(window.actual_end),
        )
        .await?;
    let Some(qualification) = prepared.source_qualification.as_ref() else {
        anyhow::bail!(
            "component '{}' has no verified return-basis, adjustment, currency, hedge, and cadence qualification",
            request.identity
        );
    };
    anyhow::ensure!(
        qualification.currency == definition.currency
            && qualification.return_basis == definition.return_basis_qualification
            && qualification.adjustment == definition.adjustment_qualification
            && qualification.hedge_policy == definition.hedge_policy,
        "component '{}' does not match its declared currency, adjustment, return basis, or hedge policy",
        request.identity
    );
    anyhow::ensure!(
        qualification.cadence == "daily",
        "component '{}' is not a daily series (cadence: {}); a Custom blend needs daily component history",
        request.identity,
        qualification.cadence
    );
    if let Some(recorded) = request.recorded.as_ref() {
        anyhow::ensure!(
            recorded.cadence == qualification.cadence,
            "component '{}' no longer reports the cadence recorded for this blend (recorded: {}; reported: {})",
            request.identity,
            recorded.cadence,
            qualification.cadence
        );
    }
    let series = parsed_series(&prepared.genuine_benchmark_series)?;
    let component = match request.recorded.as_ref() {
        Some(recorded) => BlendComponent {
            target_weight: request.weight,
            target_weight_percent: request.weight * 100.0,
            ..recorded.clone()
        },
        None => component_from_definition(&definition, &qualification.cadence, request.weight),
    };
    Ok(AdmittedComponent {
        component,
        series,
        fx_conversion_incomplete: prepared.limitations.iter().any(|limitation| {
            matches!(
                limitation.subject,
                crate::models::MarketDataSubject::FxRate { .. }
            )
        }),
    })
}

/// Resolves the catalog definition of one requested component.
///
/// A component must be a known, verified, non-nested reference, and a saved
/// definition must still match the conventions it was admitted with.
async fn resolve_component_definition(
    db: &DatabaseConnection,
    request: &BlendComponentRequest,
) -> anyhow::Result<benchmark_reference_repo::BenchmarkReference> {
    if !benchmark_reference_repo::exists(db, &request.identity).await? {
        if custom_blend_repo::exists(db, &request.identity).await? {
            anyhow::bail!(
                "component '{}' is a saved Custom blend; nested blends are not supported",
                request.identity
            );
        }
        anyhow::bail!(
            "component '{}' is not a known benchmark definition",
            request.identity
        );
    }
    let definition = benchmark_reference_repo::find_by_id(db, &request.identity).await?;
    anyhow::ensure!(
        definition.kind != CUSTOM_BLEND_KIND,
        "component '{}' is a saved Custom blend; nested blends are not supported",
        request.identity
    );
    anyhow::ensure!(
        definition.verification_state == "verified",
        "component '{}' is not a verified usable series (verification state: {})",
        request.identity,
        definition.verification_state
    );
    if let Some(recorded) = request.recorded.as_ref() {
        anyhow::ensure!(
            recorded.source == definition.source
                && recorded.source_identifier == definition.source_identifier
                && recorded.currency == definition.currency
                && recorded.return_basis == definition.return_basis_qualification
                && recorded.adjustment == definition.adjustment_qualification
                && recorded.hedge_policy == definition.hedge_policy,
            "component '{}' no longer matches the source, lookup identity, currency, return basis, adjustment, or hedge policy recorded for this blend",
            request.identity
        );
    }
    Ok(definition)
}

/// Requires the anchor and end closes and strict interior weekday coverage.
fn require_window_coverage(
    components: &[AdmittedComponent],
    window: &BlendWindow,
) -> anyhow::Result<()> {
    for component in components {
        if component.component.is_cash {
            continue;
        }
        let key = component.component.component_key.clone();
        let conversion = component.conversion_note();
        anyhow::ensure!(
            component.series.contains_key(&window.actual_start),
            "component '{key}' has no anchor close on {}; a Custom blend requires every component to hold the required anchor close{conversion}",
            format_date(window.actual_start)
        );
        anyhow::ensure!(
            component.series.contains_key(&window.actual_end),
            "component '{key}' has no end close on {}; a Custom blend requires every component to hold the required end close{conversion}",
            format_date(window.actual_end)
        );
    }
    let mut day = window.actual_start + Duration::days(1);
    while day < window.actual_end {
        if is_weekday(day) {
            for component in components {
                if component.component.is_cash {
                    continue;
                }
                anyhow::ensure!(
                    component.series.contains_key(&day),
                    "component '{}' has no observation on {}; a Custom blend requires complete daily coverage over the whole window{}",
                    component.component.component_key,
                    format_date(day),
                    component.conversion_note()
                );
            }
        }
        day += Duration::days(1);
    }
    Ok(())
}

/// Validates the declared components: finite nonnegative weights that total
/// 100% under the documented tolerance, and no repeated component.
fn requested_components(
    arguments: &[BlendComponentArgument],
) -> anyhow::Result<Vec<BlendComponentRequest>> {
    anyhow::ensure!(
        !arguments.is_empty(),
        "a Custom blend needs at least one component"
    );
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut total = 0.0;
    let mut requested = Vec::with_capacity(arguments.len());
    for argument in arguments {
        let identity = argument.identity.trim();
        anyhow::ensure!(
            !identity.is_empty(),
            "a Custom blend component needs an identity"
        );
        anyhow::ensure!(
            argument.weight_percent.is_finite(),
            "component '{identity}' weight must be a finite number"
        );
        anyhow::ensure!(
            argument.weight_percent >= 0.0,
            "component '{identity}' weight {} is negative; a Custom blend is long-only and admits no short",
            argument.weight_percent
        );
        anyhow::ensure!(
            seen.insert(identity.to_ascii_lowercase()),
            "duplicate component '{identity}'; a Custom blend admits each component once"
        );
        let weight = argument.weight_percent / 100.0;
        total += weight;
        requested.push(BlendComponentRequest {
            identity: identity.to_owned(),
            weight,
            recorded: None,
        });
    }
    anyhow::ensure!(
        (total - 1.0).abs() <= WEIGHT_TOLERANCE,
        "Custom blend target weights total {:.4}%; they must total 100% within {:.2} percentage points",
        total * 100.0,
        WEIGHT_TOLERANCE * 100.0
    );
    Ok(requested)
}

/// Rebuilds the component requests of a saved definition.
fn requested_components_from_definition(
    definition: &CustomBlendDefinition,
) -> anyhow::Result<Vec<BlendComponentRequest>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut total = 0.0;
    let mut requested = Vec::with_capacity(definition.components.len());
    for component in &definition.components {
        anyhow::ensure!(
            seen.insert(component.component_key.to_ascii_lowercase()),
            "saved Custom blend '{}' repeats component '{}'",
            definition.id,
            component.component_key
        );
        total += component.target_weight;
        requested.push(BlendComponentRequest {
            identity: component
                .component_reference_id
                .clone()
                .unwrap_or_else(|| component.component_key.clone()),
            weight: component.target_weight,
            recorded: Some(component.clone()),
        });
    }
    anyhow::ensure!(
        (total - 1.0).abs() <= WEIGHT_TOLERANCE,
        "saved Custom blend '{}' target weights total {:.4}%; they must total 100% within {:.2} percentage points",
        definition.id,
        total * 100.0,
        WEIGHT_TOLERANCE * 100.0
    );
    Ok(requested)
}

/// The comparison window a new blend is admitted against.
///
/// The default window is the portfolio's ready comparison boundary: its first
/// invested Complete closing snapshot through its effective complete
/// comparison end. An explicitly selected later start keeps its own complete
/// window and does not need a portfolio boundary.
async fn admission_window(
    db: &DatabaseConnection,
    market_data: &MarketData,
    start_date: Option<NaiveDate>,
) -> anyhow::Result<BlendWindow> {
    let current_end = market_data.today() - Duration::days(1);
    if let Some(start) = start_date {
        anyhow::ensure!(
            start <= current_end,
            "the declared comparison start {} is after the last completed date {}",
            format_date(start),
            format_date(current_end)
        );
        return Ok(BlendWindow::new(start, current_end));
    }
    let Some(boundary) = portfolio_comparison_boundary(db).await? else {
        anyhow::bail!(
            "the portfolio has no ready complete comparison history, so there is no full comparison window to admit a Custom blend against; build the portfolio history first or select an explicit later start"
        );
    };
    let start = parse_date(&boundary.first_invested_date)?;
    let end = parse_date(&boundary.effective_end_date)?;
    anyhow::ensure!(
        start <= end,
        "the portfolio comparison history is inconsistent: its first invested closing snapshot {} is after its effective comparison end {}",
        boundary.first_invested_date,
        boundary.effective_end_date
    );
    Ok(BlendWindow::new(start, end))
}

/// The portfolio's ready comparison boundary.
///
/// The boundary reads the already-persisted Complete NAV history: the first
/// invested Complete closing snapshot of a positive non-Monetary holding
/// through the effective complete comparison end. It never rebuilds NAV and
/// never replays the Transaction ledger.
pub async fn portfolio_comparison_boundary(
    db: &DatabaseConnection,
) -> anyhow::Result<Option<PortfolioComparisonBoundary>> {
    let Some(first_invested) = custom_blend_repo::find_first_invested_closing_date(db).await?
    else {
        return Ok(None);
    };
    let snapshot = portfolio_history_repo::find_at_or_before(db, &first_invested).await?;
    anyhow::ensure!(
        snapshot.is_some_and(|snapshot| snapshot.date == first_invested),
        "the first invested closing snapshot {first_invested} has no Complete NAV snapshot; the portfolio history is inconsistent"
    );
    let Some(latest) = portfolio_history_repo::find_latest(db).await? else {
        return Ok(None);
    };
    Ok(Some(PortfolioComparisonBoundary {
        first_invested_date: first_invested,
        effective_end_date: latest.date,
    }))
}

/// The admission basis of a window.
fn admission_basis(start_date: Option<NaiveDate>) -> &'static str {
    match start_date {
        Some(_) => ADMISSION_BASIS_EXPLICIT_START,
        None => ADMISSION_BASIS_FULL_HISTORY,
    }
}

/// The honest earlier-period limitation of a saved blend.
fn earlier_period_limitation(definition: &CustomBlendDefinition) -> Option<String> {
    match definition.admission_basis.as_str() {
        ADMISSION_BASIS_EXPLICIT_START => Some(format!(
            "the comparison was explicitly started on {}; the periods before that date are unavailable for this blend",
            definition.requested_start_date
        )),
        ADMISSION_BASIS_FULL_HISTORY => Some(format!(
            "the comparison starts at the portfolio's first invested closing snapshot {}; the periods before it are outside the portfolio's invested history",
            definition.requested_start_date
        )),
        _ => None,
    }
}

/// The stable identity of a saved blend and its display name.
fn blend_identity(name: &str) -> anyhow::Result<(String, String)> {
    let display_name = name.trim();
    anyhow::ensure!(!display_name.is_empty(), "a Custom blend needs a name");
    let slug: String = display_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    let slug: String = slug
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    anyhow::ensure!(
        !slug.is_empty(),
        "a Custom blend name must contain at least one letter or digit"
    );
    let slug: String = slug.chars().take(48).collect();
    let slug = slug.trim_end_matches('-').to_owned();
    anyhow::ensure!(
        !slug.is_empty(),
        "a Custom blend name must contain at least one letter or digit"
    );
    Ok((format!("custom-blend-{slug}"), display_name.to_owned()))
}

/// The catalog row of one saved blend.
fn catalog_definition(
    definition: &CustomBlendDefinition,
) -> benchmark_reference_repo::BenchmarkReference {
    benchmark_reference_repo::BenchmarkReference {
        id: definition.id.clone(),
        display_name: definition.display_name.clone(),
        instrument_ticker: String::new(),
        source: CUSTOM_BLEND_SOURCE.to_owned(),
        source_identifier: String::new(),
        currency: definition.currency.clone(),
        adjustment_provenance:
            "Component adjustment conventions are recorded per component; no blend-level adjustment is applied"
                .to_owned(),
        adjustment_qualification:
            "Component adjustment conventions are recorded per component".to_owned(),
        return_basis_qualification:
            "Arithmetic beginning-of-interval component returns, compounded".to_owned(),
        role: "Custom".to_owned(),
        universe: "Custom blend".to_owned(),
        hedge_policy: "Not applicable".to_owned(),
        source_provenance:
            "User-defined Custom blend; its components and their conventions are listed with the definition"
                .to_owned(),
        history_boundary: format!(
            "Component history admitted from {} through {}",
            definition.admitted_start_date, definition.admitted_end_date
        ),
        verification_state: "verified".to_owned(),
        official_comparator: None,
        objective_comparator: None,
        kind: CUSTOM_BLEND_KIND.to_owned(),
    }
}

/// The component snapshot of one catalog definition.
fn component_from_definition(
    definition: &benchmark_reference_repo::BenchmarkReference,
    cadence: &str,
    weight: f64,
) -> BlendComponent {
    BlendComponent {
        component_key: definition.id.clone(),
        component_reference_id: Some(definition.id.clone()),
        display_name: definition.display_name.clone(),
        target_weight: weight,
        target_weight_percent: weight * 100.0,
        is_cash: false,
        currency: definition.currency.clone(),
        return_basis: definition.return_basis_qualification.clone(),
        adjustment: definition.adjustment_qualification.clone(),
        hedge_policy: definition.hedge_policy.clone(),
        universe: definition.universe.clone(),
        source: definition.source.clone(),
        source_identifier: definition.source_identifier.clone(),
        cadence: cadence.to_owned(),
        source_provenance: definition.source_provenance.clone(),
        verification_state: definition.verification_state.clone(),
    }
}

/// The synthetic cash component.
fn cash_component(weight: f64) -> BlendComponent {
    BlendComponent {
        component_key: CASH_COMPONENT_KEY.to_owned(),
        component_reference_id: None,
        display_name: "Synthetic cash".to_owned(),
        target_weight: weight,
        target_weight_percent: weight * 100.0,
        is_cash: true,
        currency: BASE_CURRENCY.to_owned(),
        return_basis: "Synthetic 0% return; no price, no distribution, and no fee".to_owned(),
        adjustment: "Not applicable; no price is adjusted".to_owned(),
        hedge_policy: "Not applicable".to_owned(),
        universe: "Cash".to_owned(),
        source: "Synthetic cash component".to_owned(),
        source_identifier: CASH_COMPONENT_KEY.to_owned(),
        cadence: "synthetic".to_owned(),
        source_provenance: "Synthetic cash component of a Custom blend; no market data source"
            .to_owned(),
        verification_state: "synthetic".to_owned(),
    }
}

/// The arithmetic return of one component over one interval.
fn component_return(
    component: &AdmittedComponent,
    previous: NaiveDate,
    current: NaiveDate,
) -> anyhow::Result<f64> {
    if component.component.is_cash {
        return Ok(0.0);
    }
    let previous_value = component.series.get(&previous).copied().with_context(|| {
        format!(
            "component '{}' has no observation on {}",
            component.component.component_key,
            format_date(previous)
        )
    })?;
    let current_value = component.series.get(&current).copied().with_context(|| {
        format!(
            "component '{}' has no observation on {}",
            component.component.component_key,
            format_date(current)
        )
    })?;
    anyhow::ensure!(
        previous_value.is_finite() && previous_value > 0.0,
        "invalid observation for component '{}' on {}",
        component.component.component_key,
        format_date(previous)
    );
    anyhow::ensure!(
        current_value.is_finite() && current_value > 0.0,
        "invalid observation for component '{}' on {}",
        component.component.component_key,
        format_date(current)
    );
    Ok(current_value / previous_value - 1.0)
}

/// The dates every market component observed inside the window.
fn aligned_dates(components: &[AdmittedComponent], window: &BlendWindow) -> Vec<NaiveDate> {
    let mut common: Option<BTreeSet<NaiveDate>> = None;
    for component in components {
        if component.component.is_cash {
            continue;
        }
        let dates: BTreeSet<NaiveDate> = component
            .series
            .keys()
            .copied()
            .filter(|date| *date >= window.actual_start && *date <= window.actual_end)
            .collect();
        common = Some(match common {
            Some(existing) => existing.intersection(&dates).copied().collect(),
            None => dates,
        });
    }
    common.unwrap_or_default().into_iter().collect()
}

/// The parsed Base currency observations of one component.
fn parsed_series(series: &[(String, f64)]) -> anyhow::Result<BTreeMap<NaiveDate, f64>> {
    let mut parsed = BTreeMap::new();
    for (date, value) in series {
        let day = parse_date(date)?;
        anyhow::ensure!(
            value.is_finite() && *value > 0.0,
            "invalid Base currency observation for component series on {date}: {value}"
        );
        anyhow::ensure!(
            parsed.insert(day, *value).is_none(),
            "duplicate Base currency observation on {date}"
        );
    }
    Ok(parsed)
}

/// The completed weekday a requested boundary resolves to.
///
/// A weekend boundary resolves to the preceding completed weekday by calendar
/// rule, before any component availability is inspected.
fn resolve_completed_weekday(date: NaiveDate) -> NaiveDate {
    let mut resolved = date;
    while !is_weekday(resolved) {
        resolved -= Duration::days(1);
    }
    resolved
}

/// Whether a date is a completed weekday.
fn is_weekday(date: NaiveDate) -> bool {
    !matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

/// Parses one stored date.
fn parse_date(date: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("malformed date '{date}'"))
}
