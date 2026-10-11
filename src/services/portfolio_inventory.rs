use std::collections::HashMap;

use anyhow::Context;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use futures::stream::{self, StreamExt};
use sea_orm::DatabaseConnection;

use crate::constants::{
    format_date, is_benchmark_ticker, BASE_CURRENCY, DATE_FORMAT, FLOAT_EPSILON,
};
use crate::db::repos::{asset_repo, transaction_repo};
use crate::models::{
    DailyMovementCoverage, DailyMovementExclusionReason, DailyPricedHoldingsMovement,
    FactAvailability, IndividualPrice, InventoryPosition, InventorySectionAggregates,
    MarketDataLimitation, MarketDataLimitationClassification, MarketDataSubject,
    PortfolioInventory, PortfolioInventorySection,
};
use crate::services::ledger;
use crate::services::market_data::MarketData;

const CURRENT_POSITION_CONCURRENCY_LIMIT: usize = 4;

/// Projects current Transaction ledger holdings into the focused Portfolio
/// inventory: separate performance-holding and Monetary-holding sections, each
/// owning its positions, complete-or-unavailable aggregates, and Market data
/// limitations, plus the combined informational Total value.
///
/// This operation never requests NAV readiness, so it never creates or rebuilds
/// NAV history. It prepares only the Individual price and transaction-date FX
/// market data its own facts need.
pub async fn get_portfolio_inventory(
    db: &DatabaseConnection,
    market_data: &MarketData,
) -> anyhow::Result<PortfolioInventory> {
    let current_date = market_data.today();
    let today = format_date(current_date);
    let transactions = transaction_repo::find_all_ordered_by_date(db, None, Some(&today)).await?;
    if transactions.is_empty() {
        return Ok(empty_portfolio_inventory());
    }

    let mut transactions_by_asset: HashMap<i32, Vec<crate::models::Transaction>> = HashMap::new();
    for transaction in transactions {
        transactions_by_asset
            .entry(transaction.asset_id)
            .or_default()
            .push(transaction);
    }
    let assets = asset_repo::find_by_ids(db, transactions_by_asset.keys().copied()).await?;
    let end_date = format_date(current_date - Duration::days(1));

    // Prepare historical prices and FX before projecting ledger facts so that
    // transaction-date FX is cached (fetch/persist, then read) and cost and
    // dividend facts are identical across repeated requests.
    let Some(prepare_scope) = ledger_prepare_scope(&assets, &transactions_by_asset, &end_date)?
    else {
        return Ok(empty_portfolio_inventory());
    };
    if !prepare_scope.assets.is_empty() {
        market_data
            .prepare_individual_price_market_data(
                db,
                &prepare_scope.assets,
                &prepare_scope.start_date,
                &end_date,
            )
            .await?;
    }

    let projections =
        project_open_holdings(db, market_data, assets, &prepare_scope.replays).await?;
    if projections.is_empty() {
        return Ok(empty_portfolio_inventory());
    }

    let mut performance_positions = Vec::new();
    let mut monetary_positions = Vec::new();
    let mut performance_movement = Vec::new();
    let mut monetary_movement = Vec::new();
    let mut performance_limitations = Vec::new();
    let mut monetary_limitations = Vec::new();
    let projected_positions = stream::iter(projections)
        .map(|projection| async move {
            let is_monetary = projection.asset.is_monetary();
            let (position, movement) =
                inventory_position_from_projection(db, market_data, projection).await?;
            Ok::<_, anyhow::Error>((is_monetary, position, movement))
        })
        .buffered(CURRENT_POSITION_CONCURRENCY_LIMIT)
        .collect::<Vec<_>>()
        .await;
    for projected_position in projected_positions {
        let (is_monetary, position, movement) = projected_position?;
        if is_monetary {
            extend_unique_limitations(
                &mut monetary_limitations,
                position.market_data_limitations.clone(),
            );
            monetary_positions.push(position);
            monetary_movement.push(movement);
        } else {
            extend_unique_limitations(
                &mut performance_limitations,
                position.market_data_limitations.clone(),
            );
            performance_positions.push(position);
            performance_movement.push(movement);
        }
    }

    let performance_section = build_inventory_section(
        performance_positions,
        performance_limitations,
        performance_movement,
        current_date,
    );
    let monetary_section = build_inventory_section(
        monetary_positions,
        monetary_limitations,
        monetary_movement,
        current_date,
    );
    let total_value = section_total_value(&performance_section, &monetary_section);

    Ok(PortfolioInventory {
        base_currency: BASE_CURRENCY.to_owned(),
        performance_holdings: performance_section,
        monetary_holdings: monetary_section,
        total_value,
    })
}

// --- Private helpers ---

struct HoldingProjection {
    asset: crate::models::Asset,
    total_qty: f64,
    total_invested: Option<f64>,
    dividends_received: Option<f64>,
    market_data_limitations: Vec<MarketDataLimitation>,
}

/// The subset of current ledger assets and the price/FX range that must be
/// prepared before any ledger fact that depends on historical FX is projected.
struct LedgerPrepareScope {
    assets: Vec<crate::models::Asset>,
    start_date: String,
    replays: HashMap<i32, ledger::LedgerReplay>,
}

/// Derives the open-holding asset set and earliest transaction date from the
/// ledger alone (no market data reads) so that historical prices and FX can be
/// prepared before currency-dependent cost and dividend facts are projected.
fn ledger_prepare_scope(
    assets: &[crate::models::Asset],
    transactions_by_asset: &HashMap<i32, Vec<crate::models::Transaction>>,
    end_date: &str,
) -> anyhow::Result<Option<LedgerPrepareScope>> {
    let mut prepare_assets = Vec::new();
    let mut replays = HashMap::new();
    let mut earliest_transaction_date: Option<NaiveDate> = None;
    let mut has_open_holding = false;
    for asset in assets {
        if is_benchmark_ticker(&asset.ticker) {
            continue;
        }
        let Some(transactions) = transactions_by_asset.get(&asset.id) else {
            continue;
        };
        let replay = ledger::replay_transactions(asset.id, transactions)
            .map_err(|error| anyhow::anyhow!(error))?;
        if replay.final_quantity > FLOAT_EPSILON {
            has_open_holding = true;
            prepare_assets.push(asset.clone());
            let earliest = NaiveDate::parse_from_str(
                replay
                    .transitions
                    .first()
                    .map_or(end_date, |transition| transition.entry.date.as_str()),
                DATE_FORMAT,
            )?;
            earliest_transaction_date = Some(match earliest_transaction_date {
                Some(previous) => previous.min(earliest),
                None => earliest,
            });
            replays.insert(asset.id, replay);
        }
    }
    if !has_open_holding {
        return Ok(None);
    }
    let start_date = match earliest_transaction_date {
        Some(date) => format_date(date).min(end_date.to_owned()),
        None => end_date.to_owned(),
    };
    Ok(Some(LedgerPrepareScope {
        assets: prepare_assets,
        start_date,
        replays,
    }))
}

fn empty_portfolio_inventory() -> PortfolioInventory {
    let empty_aggregates = InventorySectionAggregates {
        current_value: FactAvailability::Available(0.0),
        invested_cost: FactAvailability::Available(0.0),
        dividends: FactAvailability::Available(0.0),
        open_position_gain_loss: FactAvailability::Available(0.0),
        open_position_gain_loss_pct: FactAvailability::Available(0.0),
    };
    PortfolioInventory {
        base_currency: BASE_CURRENCY.to_owned(),
        performance_holdings: PortfolioInventorySection {
            positions: Vec::new(),
            aggregates: empty_aggregates.clone(),
            daily_priced_holdings_movement: not_applicable_movement(),
            market_data_limitations: Vec::new(),
        },
        monetary_holdings: PortfolioInventorySection {
            positions: Vec::new(),
            aggregates: empty_aggregates,
            daily_priced_holdings_movement: not_applicable_movement(),
            market_data_limitations: Vec::new(),
        },
        total_value: FactAvailability::Available(0.0),
    }
}

async fn project_open_holdings(
    db: &DatabaseConnection,
    market_data: &MarketData,
    assets: Vec<crate::models::Asset>,
    replays: &HashMap<i32, ledger::LedgerReplay>,
) -> anyhow::Result<Vec<HoldingProjection>> {
    let mut projections = Vec::new();
    for asset in assets {
        if is_benchmark_ticker(&asset.ticker) {
            continue;
        }
        let Some(replay) = replays.get(&asset.id) else {
            continue;
        };
        let projection = project_holding(db, market_data, asset, replay).await?;
        if projection.total_qty > FLOAT_EPSILON {
            projections.push(projection);
        }
    }
    Ok(projections)
}

async fn project_holding(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: crate::models::Asset,
    replay: &ledger::LedgerReplay,
) -> anyhow::Result<HoldingProjection> {
    let first_date = replay
        .transitions
        .first()
        .map(|transition| transition.entry.date.as_str())
        .context("open holding has no transactions")?;
    let rates = market_data
        .get_asset_exchange_rates(db, &asset, first_date, &format_date(market_data.today()))
        .await?;
    let enriched = ledger::enrich_replay(replay, &asset.currency, BASE_CURRENCY, &rates)
        .map_err(|error| anyhow::anyhow!(error))?;
    let mut market_data_limitations = Vec::new();
    if asset.currency != BASE_CURRENCY {
        for transition in &enriched.transitions {
            if (transition.buy_contribution.is_none()
                && transition.entry_type() == ledger::LedgerEntryType::Buy)
                || (transition.dividend_income.is_none()
                    && transition.entry_type() == ledger::LedgerEntryType::Dividend)
            {
                let date =
                    NaiveDate::parse_from_str(&transition.transition.entry.date, DATE_FORMAT)
                        .context("invalid transaction date")?;
                extend_unique_limitations(
                    &mut market_data_limitations,
                    vec![MarketDataLimitation {
                        subject: MarketDataSubject::FxRate {
                            currency: asset.currency.clone(),
                        },
                        latest_available_date: None,
                        requested_end_date: date,
                        classification: MarketDataLimitationClassification::ActionableMissingData,
                    }],
                );
            }
        }
    }

    Ok(HoldingProjection {
        asset,
        total_qty: enriched.final_quantity,
        total_invested: enriched.remaining_cost,
        dividends_received: enriched.dividends,
        market_data_limitations,
    })
}

async fn inventory_position_from_projection(
    db: &DatabaseConnection,
    market_data: &MarketData,
    projection: HoldingProjection,
) -> anyhow::Result<(InventoryPosition, MovementCandidate)> {
    let asset = projection.asset.clone();
    let individual_price = market_data
        .individual_price_if_available(db, &projection.asset)
        .await?;
    let individual_price_fact = match (
        individual_price.native_price,
        individual_price.price_date.clone(),
    ) {
        (Some(native_price), Some(price_date)) => FactAvailability::Available(IndividualPrice {
            native_price,
            price_date,
        }),
        _ => FactAvailability::Unavailable,
    };
    let current_value = match (&individual_price_fact, individual_price.fx_rate) {
        (FactAvailability::Available(price), Some(fx_rate)) => {
            FactAvailability::Available(projection.total_qty * price.native_price * fx_rate)
        }
        _ => FactAvailability::Unavailable,
    };
    let invested_cost = match projection.total_invested {
        Some(cost) => FactAvailability::Available(cost),
        None => FactAvailability::Unavailable,
    };
    let average_cost = invested_cost
        .value()
        .map_or(FactAvailability::Unavailable, |cost| {
            FactAvailability::Available(cost / projection.total_qty)
        });
    let dividends = match projection.dividends_received {
        Some(dividends) => FactAvailability::Available(dividends),
        None => FactAvailability::Unavailable,
    };
    let open_position_gain_loss = dependent_fact(
        current_value.value().copied(),
        invested_cost.value().copied(),
        |value, cost| value - cost,
    );
    let open_position_gain_loss_pct = dependent_fact(
        open_position_gain_loss.value().copied(),
        invested_cost.value().copied(),
        gain_loss_pct,
    );

    let position = InventoryPosition {
        ticker: asset.ticker.clone(),
        name: asset.name.clone(),
        asset_type: asset.asset_type.clone(),
        currency: asset.currency.clone(),
        morningstar_code: asset.morningstar_code.clone(),
        asset_class: asset.asset_class.clone(),
        equity_style: asset.equity_style.clone(),
        management: asset.management.clone(),
        quantity: projection.total_qty,
        average_cost,
        invested_cost,
        dividends,
        individual_price: individual_price_fact,
        current_value,
        open_position_gain_loss,
        open_position_gain_loss_pct,
        market_data_limitations: {
            let mut limitations = projection.market_data_limitations;
            extend_unique_limitations(&mut limitations, individual_price.limitations.clone());
            limitations
        },
    };
    let movement = movement_candidate(
        db,
        market_data,
        &asset,
        projection.total_qty,
        &individual_price,
    )
    .await?;
    Ok((position, movement))
}

struct MovementCandidate {
    eligible: bool,
    coverage: DailyMovementCoverage,
    movement: Option<f64>,
    limitations: Vec<MarketDataLimitation>,
}

struct BaselineValuation {
    native_price: f64,
    price_date: String,
    fx_rate: Option<f64>,
    fx_date: Option<String>,
}

async fn movement_candidate(
    db: &DatabaseConnection,
    market_data: &MarketData,
    asset: &crate::models::Asset,
    quantity: f64,
    current: &crate::models::IndividualPriceAvailability,
) -> anyhow::Result<MovementCandidate> {
    use crate::db::repos::{daily_price_repo, exchange_rate_repo};
    use crate::models::AssetType;

    let eligible = matches!(asset.asset_type, AssetType::Stock | AssetType::Etf);
    let today = market_data.today();
    let no_coverage = |reason: DailyMovementExclusionReason| DailyMovementCoverage {
        ticker: asset.ticker.clone(),
        baseline_date: None,
        baseline_fx_date: None,
        prior_base_currency_value: None,
        current_base_currency_value: None,
        exclusion_reason: Some(reason),
    };
    if !eligible {
        return Ok(MovementCandidate {
            eligible,
            coverage: no_coverage(DailyMovementExclusionReason::OutsideMovementScope),
            movement: None,
            limitations: Vec::new(),
        });
    }
    let prior_date = today - Duration::days(1);
    let prior_str = format_date(prior_date);
    let Some((baseline_price, baseline_date)) =
        daily_price_repo::find_price_and_date_at_or_before(db, asset.id, &prior_str).await?
    else {
        return Ok(MovementCandidate {
            eligible,
            coverage: no_coverage(DailyMovementExclusionReason::NoPriorPriceObservation),
            movement: None,
            limitations: vec![MarketDataLimitation {
                subject: MarketDataSubject::Asset {
                    ticker: asset.ticker.clone(),
                    name: asset.name.clone(),
                    asset_type: asset.asset_type.clone(),
                },
                latest_available_date: None,
                requested_end_date: today,
                classification: MarketDataLimitationClassification::ActionableMissingData,
            }],
        });
    };
    let (prior_fx, baseline_fx_date) = if asset.currency == BASE_CURRENCY {
        (Some(1.0), Some(baseline_date.clone()))
    } else {
        exchange_rate_repo::find_rate_and_date_at_or_before(
            db,
            &asset.currency,
            BASE_CURRENCY,
            &format_date(today - Duration::days(1)),
        )
        .await?
        .map_or((None, None), |(rate, date)| (Some(rate), Some(date)))
    };
    Ok(movement_from_baseline(
        asset,
        quantity,
        current,
        today,
        BaselineValuation {
            native_price: baseline_price,
            price_date: baseline_date,
            fx_rate: prior_fx,
            fx_date: baseline_fx_date,
        },
    ))
}

fn movement_from_baseline(
    asset: &crate::models::Asset,
    quantity: f64,
    current: &crate::models::IndividualPriceAvailability,
    today: NaiveDate,
    baseline: BaselineValuation,
) -> MovementCandidate {
    let today_text = format_date(today);
    let has_today_quote = current.price_date.as_deref() == Some(today_text.as_str());
    let current_native = if has_today_quote {
        current.native_price
    } else {
        Some(baseline.native_price)
    };
    let current_fx = if asset.currency == BASE_CURRENCY {
        Some(1.0)
    } else {
        current.fx_rate
    };
    let (prior_value, current_value, movement) =
        match (baseline.fx_rate, current_native, current_fx) {
            (Some(prior_fx), Some(current_native), Some(current_fx)) => {
                let prior_value = quantity * baseline.native_price * prior_fx;
                let current_value = quantity * current_native * current_fx;
                (
                    Some(prior_value),
                    Some(current_value),
                    Some(current_value - prior_value),
                )
            }
            _ => (None, None, None),
        };
    let mut limitations = current.limitations.clone();
    if !has_today_quote {
        limitations.push(MarketDataLimitation {
            subject: MarketDataSubject::Asset {
                ticker: asset.ticker.clone(),
                name: asset.name.clone(),
                asset_type: asset.asset_type.clone(),
            },
            latest_available_date: NaiveDate::parse_from_str(&baseline.price_date, DATE_FORMAT)
                .ok(),
            requested_end_date: today,
            classification: MarketDataLimitationClassification::ActionableReportingLag,
        });
    }
    if baseline.fx_rate.is_none() || current_fx.is_none() {
        limitations.push(MarketDataLimitation {
            subject: MarketDataSubject::FxRate {
                currency: asset.currency.clone(),
            },
            latest_available_date: None,
            requested_end_date: today,
            classification: MarketDataLimitationClassification::ActionableMissingData,
        });
    }
    let exclusion_reason = movement
        .is_none()
        .then_some(DailyMovementExclusionReason::MissingCurrentPriceOrFx);
    MovementCandidate {
        eligible: true,
        coverage: DailyMovementCoverage {
            ticker: asset.ticker.clone(),
            baseline_date: Some(baseline.price_date),
            baseline_fx_date: baseline.fx_date,
            prior_base_currency_value: prior_value,
            current_base_currency_value: current_value,
            exclusion_reason,
        },
        movement,
        limitations,
    }
}

/// Builds a fact from two independently available inputs; the fact is
/// unavailable when either input is unavailable.
fn dependent_fact<T>(
    left: Option<f64>,
    right: Option<f64>,
    calculate: impl FnOnce(f64, f64) -> T,
) -> FactAvailability<T> {
    match (left, right) {
        (Some(left), Some(right)) => FactAvailability::Available(calculate(left, right)),
        _ => FactAvailability::Unavailable,
    }
}

fn build_inventory_section(
    positions: Vec<InventoryPosition>,
    mut limitations: Vec<MarketDataLimitation>,
    candidates: Vec<MovementCandidate>,
    today: NaiveDate,
) -> PortfolioInventorySection {
    let open_position_gain_loss = complete_fact_sum(
        positions
            .iter()
            .map(|position| position.open_position_gain_loss.value().copied()),
    );
    let invested_cost = complete_fact_sum(
        positions
            .iter()
            .map(|position| position.invested_cost.value().copied()),
    );
    let aggregates = InventorySectionAggregates {
        current_value: complete_fact_sum(
            positions
                .iter()
                .map(|position| position.current_value.value().copied()),
        ),
        invested_cost: invested_cost.clone(),
        dividends: complete_fact_sum(
            positions
                .iter()
                .map(|position| position.dividends.value().copied()),
        ),
        open_position_gain_loss: open_position_gain_loss.clone(),
        open_position_gain_loss_pct: combine_gain_loss_pct(
            &open_position_gain_loss,
            &invested_cost,
        ),
    };
    for position in &positions {
        extend_unique_limitations(&mut limitations, position.market_data_limitations.clone());
    }
    let movement = build_daily_movement(candidates, today);
    PortfolioInventorySection {
        positions,
        aggregates,
        daily_priced_holdings_movement: movement,
        market_data_limitations: limitations,
    }
}

fn not_applicable_movement() -> DailyPricedHoldingsMovement {
    DailyPricedHoldingsMovement {
        movement: FactAvailability::NotApplicable,
        movement_pct: FactAvailability::NotApplicable,
        covered_prior_value: FactAvailability::NotApplicable,
        included_positions: Vec::new(),
        excluded_positions: Vec::new(),
        limitations: Vec::new(),
    }
}

fn build_daily_movement(
    candidates: Vec<MovementCandidate>,
    today: NaiveDate,
) -> DailyPricedHoldingsMovement {
    let weekday = !matches!(today.weekday(), Weekday::Sat | Weekday::Sun);
    let eligible_count = candidates
        .iter()
        .filter(|candidate| candidate.eligible)
        .count();
    if !weekday || eligible_count == 0 {
        return not_applicable_movement();
    }
    let mut included_positions = Vec::new();
    let mut excluded_positions = Vec::new();
    let mut movement_total = 0.0;
    let mut prior_total = 0.0;
    let mut movement_limitations = Vec::new();
    for candidate in candidates {
        extend_unique_limitations(&mut movement_limitations, candidate.limitations);
        if !candidate.eligible {
            excluded_positions.push(candidate.coverage);
        } else if let Some(movement) = candidate.movement {
            movement_total += movement;
            prior_total += candidate
                .coverage
                .prior_base_currency_value
                .unwrap_or_default();
            included_positions.push(candidate.coverage);
        } else {
            excluded_positions.push(candidate.coverage);
        }
    }
    if included_positions.is_empty() {
        return DailyPricedHoldingsMovement {
            movement: FactAvailability::Unavailable,
            movement_pct: FactAvailability::Unavailable,
            covered_prior_value: FactAvailability::Unavailable,
            included_positions,
            excluded_positions,
            limitations: movement_limitations,
        };
    }
    let percentage = if prior_total.abs() < FLOAT_EPSILON {
        FactAvailability::NotApplicable
    } else {
        FactAvailability::Available(movement_total / prior_total * 100.0)
    };
    DailyPricedHoldingsMovement {
        movement: FactAvailability::Available(movement_total),
        movement_pct: percentage,
        covered_prior_value: FactAvailability::Available(prior_total),
        included_positions,
        excluded_positions,
        limitations: movement_limitations,
    }
}

/// Sums an ordinary aggregate: available zero for an empty section, otherwise
/// unavailable when any included position fact is unavailable.
fn complete_fact_sum(values: impl Iterator<Item = Option<f64>>) -> FactAvailability<f64> {
    let mut sum = 0.0;
    for value in values {
        match value {
            Some(value) => sum += value,
            None => return FactAvailability::Unavailable,
        }
    }
    FactAvailability::Available(sum)
}

/// The Open-position gain/loss percentage for one fact: zero when the invested
/// cost is zero, else the relative gain/loss. Shared by position and section
/// facts so one zero-denominator policy applies everywhere.
fn gain_loss_pct(open_position_gain_loss: f64, invested_cost: f64) -> f64 {
    if invested_cost.abs() < FLOAT_EPSILON {
        0.0
    } else {
        (open_position_gain_loss / invested_cost) * 100.0
    }
}

fn combine_gain_loss_pct(
    open_position_gain_loss: &FactAvailability<f64>,
    invested_cost: &FactAvailability<f64>,
) -> FactAvailability<f64> {
    match (open_position_gain_loss.value(), invested_cost.value()) {
        (Some(gain_loss), Some(invested_cost)) => {
            FactAvailability::Available(gain_loss_pct(*gain_loss, *invested_cost))
        }
        _ => FactAvailability::Unavailable,
    }
}

/// The combined informational Total value is available only when both section
/// values are complete; it does not participate in performance measurement.
fn section_total_value(
    performance: &PortfolioInventorySection,
    monetary: &PortfolioInventorySection,
) -> FactAvailability<f64> {
    match (
        performance.aggregates.current_value.value(),
        monetary.aggregates.current_value.value(),
    ) {
        (Some(performance_value), Some(monetary_value)) => {
            FactAvailability::Available(performance_value + monetary_value)
        }
        _ => FactAvailability::Unavailable,
    }
}

fn extend_unique_limitations(
    limitations: &mut Vec<MarketDataLimitation>,
    additional: Vec<MarketDataLimitation>,
) {
    for limitation in additional {
        if !limitations.contains(&limitation) {
            limitations.push(limitation);
        }
    }
}
