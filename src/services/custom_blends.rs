//! Saved Custom benchmark blends.
//!
//! A Custom blend is a flat, long-only, fully allocated target definition
//! whose wealth is computed from its own components: arithmetic
//! beginning-of-interval component returns that are compounded, with monthly
//! target resets and return-driven weight drift between resets. Definitions
//! are immutable — there is no edit operation and no automatic revision — and
//! a saved definition keeps the component conventions it was admitted with.
//!
//! Market data acquisition and EUR conversion stay in `MarketData`; this
//! module only admits components, resolves their coverage, and calculates the
//! blend's own dated wealth.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use sea_orm::{DatabaseConnection, TransactionTrait};

use crate::constants::{format_date, BASE_CURRENCY, DATE_FORMAT};
use crate::db::repos::{benchmark_assignment_repo, benchmark_reference_repo, custom_blend_repo};
use crate::models::{
    BlendComponent, BlendComponentArgument, BlendTargetReset, BlendWealthPoint,
    CustomBlendDefinition, CustomBlendDeletion, CustomBlendPreview,
};
use crate::services::market_data::MarketData;

/// The reserved component identity of the synthetic cash component.
pub const CASH_COMPONENT_KEY: &str = "cash";

/// The full portfolio comparison history: the default admission window.
///
/// A saved blend is admitted against the whole comparison history unless the
/// author explicitly selects a later start. The window never shortens
/// automatically to make a component admissible.
pub const FULL_COMPARISON_HISTORY_START: &str = "2000-01-01";

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

/// The admission basis of a blend admitted against the full comparison history.
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

/// One admitted component with the aligned Base currency series it uses.
#[derive(Clone, Debug, PartialEq)]
pub struct AdmittedComponent {
    pub component: BlendComponent,
    pub series: Vec<(String, f64)>,
}

/// The dated comparison window a blend is admitted against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BlendWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
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
    let window = admission_window(market_data, request.start_date)?;
    let admitted = admit_components(db, market_data, &requested, window).await?;

    let definition = CustomBlendDefinition {
        id: blend_id.clone(),
        display_name: display_name.clone(),
        created_at: format!("{}T00:00:00Z", format_date(market_data.today())),
        currency: BASE_CURRENCY.to_owned(),
        reset_rule: RESET_RULE.to_owned(),
        weight_convention: WEIGHT_CONVENTION.to_owned(),
        cash_convention: CASH_CONVENTION.to_owned(),
        weight_tolerance: WEIGHT_TOLERANCE,
        admitted_start_date: format_date(window.start),
        admitted_end_date: format_date(window.end),
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
/// The preview covers the blend's admitted start through the latest completed
/// date. It never shortens the window to hide missing coverage: a component
/// without complete history over the window is reported as an error.
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
    let window = BlendWindow { start, end };
    let requested = requested_components_from_definition(&definition)?;
    let admitted = admit_components(db, market_data, &requested, window).await?;
    let series = calculate_blend_wealth(&admitted, window)?;
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
        window_start_date: format_date(window.start),
        window_end_date: format_date(window.end),
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
/// A definition that is still assigned or selected as the Strategy cannot be
/// deleted. Deleting a definition never removes component price, FX, ledger,
/// or NAV history.
pub async fn delete_custom_blend(
    db: &DatabaseConnection,
    blend_id: &str,
) -> anyhow::Result<CustomBlendDeletion> {
    let definition = custom_blend_repo::find_by_id(db, blend_id).await?;
    let assignments = benchmark_assignment_repo::find_by_reference(db, blend_id).await?;
    if !assignments.is_empty() {
        anyhow::bail!(
            "Custom blend '{blend_id}' is in use by {} active assignment(s); remove or replace them before deleting the definition",
            assignments.len()
        );
    }
    if custom_blend_repo::find_strategy_selection(db)
        .await?
        .as_deref()
        == Some(blend_id)
    {
        anyhow::bail!(
            "Custom blend '{blend_id}' is the selected Strategy; unselect it before deleting the definition"
        );
    }
    let deleted = custom_blend_repo::delete_blend(db, blend_id).await?;
    Ok(CustomBlendDeletion {
        blend_id: blend_id.to_owned(),
        display_name: definition.display_name,
        deleted,
        preserved_history_note:
            "Deleted definition only; component price, FX, ledger, and NAV history are preserved"
                .to_owned(),
    })
}

/// The running state of one blend: component values, drifted weights, wealth.
struct BlendState {
    values: Vec<f64>,
    weights: Vec<f64>,
    targets: Vec<f64>,
    wealth: f64,
}

/// Calculates the dated, rebased wealth series of one blend.
///
/// Component returns are arithmetic beginning-of-interval returns and the
/// blend wealth compounds them. Target weights are initialized at the anchor
/// close, reset at the close immediately before each new calendar month, and
/// drift with component wealth between resets.
pub fn calculate_blend_wealth(
    components: &[AdmittedComponent],
    window: BlendWindow,
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
    let observations: Vec<BTreeMap<String, f64>> = components
        .iter()
        .map(|component| {
            component
                .series
                .iter()
                .map(|(date, value)| (date.clone(), *value))
                .collect()
        })
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
        let reset = apply_interval(&mut state, &observations, components, pair[0], pair[1])?;
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

/// Applies one interval to the blend state.
///
/// The monthly reset happens at the close immediately preceding the new
/// calendar month, before the first return interval of that month. The
/// interval itself uses the beginning-of-interval weights, and the weights
/// drift with component wealth afterwards.
fn apply_interval(
    state: &mut BlendState,
    observations: &[BTreeMap<String, f64>],
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
    let previous_date = format_date(previous);
    let current_date = format_date(current);
    let mut interval_return = 0.0;
    for (index, component) in components.iter().enumerate() {
        interval_return += state.weights[index]
            * component_return(
                &observations[index],
                &component.component,
                &previous_date,
                &current_date,
            )?;
    }
    state.wealth *= 1.0 + interval_return;
    for (index, component) in components.iter().enumerate() {
        let component_return = component_return(
            &observations[index],
            &component.component,
            &previous_date,
            &current_date,
        )?;
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
/// non-nested reference whose qualified daily Base currency series covers
/// every weekday of the window. A component that cannot be admitted is
/// reported; no component is omitted and no weight is renormalized.
async fn admit_components(
    db: &DatabaseConnection,
    market_data: &MarketData,
    requested: &[BlendComponentRequest],
    window: BlendWindow,
) -> anyhow::Result<Vec<AdmittedComponent>> {
    let mut admitted = Vec::with_capacity(requested.len());
    for request in requested {
        admitted.push(admit_component(db, market_data, request, window).await?);
    }
    Ok(admitted)
}

/// Admits one requested component over the window.
async fn admit_component(
    db: &DatabaseConnection,
    market_data: &MarketData,
    request: &BlendComponentRequest,
    window: BlendWindow,
) -> anyhow::Result<AdmittedComponent> {
    if request.identity == CASH_COMPONENT_KEY {
        return Ok(AdmittedComponent {
            component: request
                .recorded
                .clone()
                .unwrap_or_else(|| cash_component(request.weight)),
            series: Vec::new(),
        });
    }
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
            recorded.currency == definition.currency
                && recorded.return_basis == definition.return_basis_qualification
                && recorded.hedge_policy == definition.hedge_policy,
            "component '{}' no longer matches the conventions recorded for this blend",
            request.identity
        );
    }
    let prepared = market_data
        .selected_benchmark_reference_market_data(
            db,
            &request.identity,
            &format_date(window.start),
            &format_date(window.end),
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
    let series = prepared.genuine_benchmark_series;
    let observed: BTreeSet<String> = series.iter().map(|(date, _)| date.clone()).collect();
    if let Some(missing) = first_missing_weekday(&observed, window) {
        anyhow::bail!(
            "component '{}' has no source observation on {}; a Custom blend requires complete daily history over the declared window",
            request.identity,
            missing
        );
    }
    let component = match request.recorded.as_ref() {
        Some(recorded) => BlendComponent {
            target_weight: request.weight,
            target_weight_percent: request.weight * 100.0,
            ..recorded.clone()
        },
        None => component_from_definition(&definition, request.weight),
    };
    Ok(AdmittedComponent { component, series })
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

/// The admission window: the full comparison history unless the author
/// explicitly selected a later start.
fn admission_window(
    market_data: &MarketData,
    start_date: Option<NaiveDate>,
) -> anyhow::Result<BlendWindow> {
    let start = match start_date {
        Some(start) => start,
        None => parse_date(FULL_COMPARISON_HISTORY_START)?,
    };
    let end = market_data.today() - Duration::days(1);
    anyhow::ensure!(
        start <= end,
        "the declared comparison start {} is after the last completed date {}",
        format_date(start),
        format_date(end)
    );
    Ok(BlendWindow { start, end })
}

/// The admission basis of a window.
fn admission_basis(start_date: Option<NaiveDate>) -> &'static str {
    match start_date {
        Some(_) => ADMISSION_BASIS_EXPLICIT_START,
        None => ADMISSION_BASIS_FULL_HISTORY,
    }
}

/// The honest earlier-period limitation of a blend admitted from a later start.
fn earlier_period_limitation(definition: &CustomBlendDefinition) -> Option<String> {
    if definition.admission_basis == ADMISSION_BASIS_EXPLICIT_START {
        Some(format!(
            "periods before {} are unavailable for this blend: its comparison was explicitly started on {}",
            definition.admitted_start_date, definition.admitted_start_date
        ))
    } else {
        None
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
            "Component conventions are recorded per component; no blend-level adjustment is applied"
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
        hedge_policy: definition.hedge_policy.clone(),
        universe: definition.universe.clone(),
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
        hedge_policy: "Not applicable".to_owned(),
        universe: "Cash".to_owned(),
        source_provenance: "Synthetic cash component of a Custom blend; no market data source"
            .to_owned(),
        verification_state: "synthetic".to_owned(),
    }
}

/// The arithmetic return of one component over one interval.
fn component_return(
    observations: &BTreeMap<String, f64>,
    component: &BlendComponent,
    previous: &str,
    current: &str,
) -> anyhow::Result<f64> {
    if component.is_cash {
        return Ok(0.0);
    }
    let previous_value = observations.get(previous).copied().with_context(|| {
        format!(
            "component '{}' has no observation on {previous}",
            component.component_key
        )
    })?;
    let current_value = observations.get(current).copied().with_context(|| {
        format!(
            "component '{}' has no observation on {current}",
            component.component_key
        )
    })?;
    anyhow::ensure!(
        previous_value.is_finite() && previous_value > 0.0,
        "invalid observation for component '{}' on {previous}",
        component.component_key
    );
    anyhow::ensure!(
        current_value.is_finite() && current_value > 0.0,
        "invalid observation for component '{}' on {current}",
        component.component_key
    );
    Ok(current_value / previous_value - 1.0)
}

/// The dates every market component observed inside the window.
fn aligned_dates(components: &[AdmittedComponent], window: BlendWindow) -> Vec<NaiveDate> {
    let mut common: Option<BTreeSet<NaiveDate>> = None;
    for component in components {
        if component.component.is_cash {
            continue;
        }
        let dates: BTreeSet<NaiveDate> = component
            .series
            .iter()
            .filter_map(|(date, _)| parse_date(date).ok())
            .filter(|date| *date >= window.start && *date <= window.end)
            .collect();
        common = Some(match common {
            Some(existing) => existing.intersection(&dates).copied().collect(),
            None => dates,
        });
    }
    common.unwrap_or_default().into_iter().collect()
}

/// The first weekday of the window without a source observation.
fn first_missing_weekday(observed: &BTreeSet<String>, window: BlendWindow) -> Option<String> {
    let mut day = window.start;
    while day <= window.end {
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun)
            && !observed.contains(&format_date(day))
        {
            return Some(format_date(day));
        }
        day += Duration::days(1);
    }
    None
}

/// Parses one stored date.
fn parse_date(date: &str) -> anyhow::Result<NaiveDate> {
    NaiveDate::parse_from_str(date, DATE_FORMAT).with_context(|| format!("malformed date '{date}'"))
}
