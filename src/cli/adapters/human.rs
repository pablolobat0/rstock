//! Human output Adapter for the Portfolio view.
//!
//! Renders one complete, presentation-neutral [`PortfolioView`] as terminal
//! output through the injected writer: the current performance-holdings and
//! Monetary-holding inventory sections with their complete-or-unavailable
//! aggregates, the synchronized performance facts at their Effective valuation
//! date, the requested ready NAV history chart, and the four Market data
//! limitation scopes. Not applicable is rendered distinctly from unavailable.

use std::io::Write;

use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Color, Style};
use tabled::Table;
use textplots::{Chart, Plot, Shape};

use crate::cli::display::helpers::{
    color_for_value, color_value, format_eu, format_market_data_limitation_warning,
};
use crate::constants::display_date;
use crate::models::{
    AssetType, FactAvailability, InventoryPosition, InventorySectionAggregates,
    MarketDataLimitation, NavHistoryRequest, PeriodOutcome, PortfolioInventory,
    PortfolioInventorySection, PortfolioPerformance, PortfolioSnapshot, PortfolioView,
};

/// Renders the complete Portfolio view through the injected writer. The view
/// must already contain the requested ready NAV history; the Adapter performs
/// no portfolio, market-data, or history operations itself.
pub fn write_portfolio_human<W: Write>(
    writer: &mut W,
    view: &PortfolioView,
    nav_history_request: NavHistoryRequest,
) -> anyhow::Result<()> {
    write_inventory(writer, &view.inventory)?;
    write_performance(
        writer,
        &view.performance,
        &view.nav_history,
        nav_history_request,
    )?;
    write_limitations(writer, view)?;
    Ok(())
}

// --- Inventory sections ---

/// Presents the current inventory: one section per holding scope, each owning
/// its positions and complete-or-unavailable aggregates, plus the combined
/// informational Total value. Mixed-date inventory values never carry the
/// Effective valuation date.
fn write_inventory<W: Write>(writer: &mut W, inventory: &PortfolioInventory) -> anyhow::Result<()> {
    let has_performance_positions = !inventory.performance_holdings.positions.is_empty();
    let has_monetary_positions = !inventory.monetary_holdings.positions.is_empty();

    if !has_performance_positions && !has_monetary_positions {
        writeln!(writer, "No positions found.")?;
    }
    if has_performance_positions {
        write_performance_section(writer, &inventory.performance_holdings)?;
    }
    if has_monetary_positions {
        writeln!(writer)?;
        write_monetary_section(writer, &inventory.monetary_holdings)?;
    }
    if has_performance_positions || has_monetary_positions {
        writeln!(
            writer,
            "Total value: {}",
            fact_or_label(&inventory.total_value, |value| format_amount(*value))
        )?;
    }
    Ok(())
}

fn write_performance_section<W: Write>(
    writer: &mut W,
    section: &PortfolioInventorySection,
) -> anyhow::Result<()> {
    write_position_section(writer, "Performance holdings:", section, true)?;
    writeln!(writer, "{}", section_aggregates_line(&section.aggregates))?;
    Ok(())
}

fn write_monetary_section<W: Write>(
    writer: &mut W,
    section: &PortfolioInventorySection,
) -> anyhow::Result<()> {
    write_position_section(writer, "Monetary holdings:", section, false)?;
    writeln!(writer, "{}", section_aggregates_line(&section.aggregates))?;
    Ok(())
}

fn write_position_section<W: Write>(
    writer: &mut W,
    heading: &str,
    section: &PortfolioInventorySection,
    with_weight: bool,
) -> anyhow::Result<()> {
    writeln!(writer, "{heading}")?;
    let sorted = sorted_by_value(&section.positions);
    let performance_total = section.aggregates.current_value.value().copied();
    let rows: Vec<PositionRow> = sorted
        .iter()
        .map(|position| position_row(position, performance_total, with_weight))
        .collect();
    let mut table = Table::new(&rows);
    apply_modern_style(&mut table);
    // The weight column is rendered empty for the Monetary section, whose
    // holdings are excluded from performance weights.
    for col in 4..=13 {
        table.modify(Columns::single(col), Alignment::right());
    }
    for (index, position) in sorted.iter().enumerate() {
        let FactAvailability::Available(gain_loss) = position.open_position_gain_loss else {
            continue;
        };
        let color = color_for_value(gain_loss);
        table.modify(Cell::new(index + 1, 11), color.clone());
        table.modify(Cell::new(index + 1, 12), color);
    }
    writeln!(writer, "{table}")?;
    Ok(())
}

fn sorted_by_value(positions: &[InventoryPosition]) -> Vec<&InventoryPosition> {
    let mut sorted: Vec<&InventoryPosition> = positions.iter().collect();
    super::sort_position_refs_by_current_value(&mut sorted);
    sorted
}

// --- Performance facts ---
fn write_performance<W: Write>(
    writer: &mut W,
    performance: &PortfolioPerformance,
    nav_history: &[PortfolioSnapshot],
    nav_history_request: NavHistoryRequest,
) -> anyhow::Result<()> {
    let PortfolioPerformance::Available(available) = performance else {
        let state = match performance {
            PortfolioPerformance::Unavailable { .. } => "unavailable",
            PortfolioPerformance::NotApplicable => "not applicable",
            PortfolioPerformance::Available(..) => unreachable!("handled by the let-else above"),
        };
        writeln!(writer, "\nPerformance: {state}")?;
        return write_nav_chart(writer, nav_history, nav_history_request);
    };

    writeln!(writer, "\nPerformance:")?;
    let as_of = format!(
        " (as of {})",
        display_date(&available.effective_valuation_date)
    );
    writeln!(
        writer,
        "Performance value: {}{as_of}",
        format_eu(&format!("{:.2}", available.synchronized_value))
    )?;
    writeln!(
        writer,
        "NAV: {}{as_of}",
        format_eu(&format!("{:.2}", available.nav))
    )?;
    writeln!(
        writer,
        "Inception: {}",
        display_date(&available.inception_date)
    )?;

    let periods = [
        ("YTD", &available.periods.ytd),
        ("1Y", &available.periods.one_year),
        ("3Y(CAGR)", &available.periods.three_years),
        ("5Y(CAGR)", &available.periods.five_years),
        ("All(CAGR)", &available.periods.all),
    ];
    write_metrics_table(writer, &periods)?;
    write_nav_chart(writer, nav_history, nav_history_request)
}

/// The YTD, 1Y, 3Y, 5Y, and All period outcomes with their independently
/// available return and risk facts. Each period's facts stay grouped, and the
/// two absent states keep their distinct labels.
/// One metric row: the row label, the number formatting, and the fact getter.
type MetricRow = (
    &'static str,
    MetricFormat,
    fn(&PeriodOutcome) -> &FactAvailability<f64>,
);

fn write_metrics_table<W: Write>(
    writer: &mut W,
    periods: &[(&str, &PeriodOutcome)],
) -> anyhow::Result<()> {
    let mut builder = Builder::default();
    let mut header = vec![String::new()];
    header.extend(periods.iter().map(|(name, _)| (*name).to_owned()));
    builder.push_record(header);

    let metric_rows: [MetricRow; 6] = [
        ("Return", MetricFormat::SignedPercent, |outcome| {
            &outcome.return_pct
        }),
        ("Volatility", MetricFormat::Percent, |outcome| {
            &outcome.volatility
        }),
        ("Max Drawdown", MetricFormat::Percent, |outcome| {
            &outcome.max_drawdown
        }),
        ("Sharpe", MetricFormat::Plain, |outcome| &outcome.sharpe),
        ("Sortino", MetricFormat::Plain, |outcome| &outcome.sortino),
        ("Beta", MetricFormat::Plain, |outcome| &outcome.beta),
    ];
    for (label, format, fact) in metric_rows {
        let mut row = vec![label.to_owned()];
        row.extend(
            periods
                .iter()
                .map(|(_, outcome)| fact_or_label(fact(outcome), |value| format.format(*value))),
        );
        builder.push_record(row);
    }

    let mut table = builder.build();
    apply_modern_style(&mut table);
    // Apply colors after building so ANSI codes do not break alignment.
    for (column, (_, outcome)) in periods.iter().enumerate() {
        let column = column + 1; // offset for the label column
        if let FactAvailability::Available(return_pct) = &outcome.return_pct {
            table.modify(Cell::new(1, column), color_for_value(*return_pct));
        }
        if let FactAvailability::Available(_) = &outcome.max_drawdown {
            table.modify(Cell::new(3, column), Color::FG_RED);
        }
        if let FactAvailability::Available(sharpe) = &outcome.sharpe {
            table.modify(Cell::new(4, column), color_for_value(*sharpe));
        }
        if let FactAvailability::Available(sortino) = &outcome.sortino {
            table.modify(Cell::new(5, column), color_for_value(*sortino));
        }
    }
    writeln!(writer, "{table}")?;
    Ok(())
}

#[derive(Clone, Copy)]
enum MetricFormat {
    SignedPercent,
    Percent,
    Plain,
}

impl MetricFormat {
    fn format(self, value: f64) -> String {
        match self {
            MetricFormat::SignedPercent => signed_percent(value),
            MetricFormat::Percent => format_eu(&format!("{value:.2}%")),
            MetricFormat::Plain => format_eu(&format!("{value:.2}")),
        }
    }
}

/// The requested ready NAV history chart: an Adapter concern over the history
/// the composer already obtained, never a second history query.
fn write_nav_chart<W: Write>(
    writer: &mut W,
    snapshots: &[PortfolioSnapshot],
    nav_history_request: NavHistoryRequest,
) -> anyhow::Result<()> {
    if snapshots.len() < 2 {
        writeln!(writer, "\nNot enough data to display NAV chart.")?;
        return Ok(());
    }

    let points: Vec<(f32, f32)> = snapshots
        .iter()
        .enumerate()
        .map(|(index, snapshot)| (index as f32, snapshot.nav as f32))
        .collect();
    let xmax = (snapshots.len() - 1) as f32;

    writeln!(writer, "\nNAV — {}", period_label(nav_history_request))?;
    let shape = Shape::Lines(&points);
    let mut chart = Chart::new(180, 60, 0.0, xmax);
    let plotted = chart.lineplot(&shape);
    plotted.axis();
    plotted.figures();
    writeln!(writer, "{plotted}")?;
    writeln!(
        writer,
        "  {}  →  {}",
        display_date(&snapshots[0].date),
        display_date(&snapshots[snapshots.len() - 1].date)
    )?;
    Ok(())
}

fn period_label(nav_history_request: NavHistoryRequest) -> &'static str {
    match nav_history_request {
        NavHistoryRequest::OneMonth => "1M",
        NavHistoryRequest::ThreeMonths => "3M",
        NavHistoryRequest::SixMonths => "6M",
        NavHistoryRequest::Ytd => "YTD",
        NavHistoryRequest::OneYear => "1Y",
        NavHistoryRequest::ThreeYears => "3Y",
        NavHistoryRequest::FiveYears => "5Y",
        NavHistoryRequest::All => "All",
    }
}

// --- Limitation scopes ---

/// Prints the four independent Market data limitation scopes under distinct
/// labels. A limitation in one scope never implies that another scope is
/// invalid, and an empty scope prints nothing.
fn write_limitations<W: Write>(writer: &mut W, view: &PortfolioView) -> anyhow::Result<()> {
    let scopes: [(&str, &[MarketDataLimitation]); 4] = [
        (
            "NAV/history market data limitations:",
            nav_history_limitations(view),
        ),
        (
            "Benchmark risk market data limitations:",
            benchmark_risk_limitations(view),
        ),
        (
            "Performance-holdings market data limitations:",
            &view.inventory.performance_holdings.market_data_limitations,
        ),
        (
            "Monetary-holdings market data limitations:",
            &view.inventory.monetary_holdings.market_data_limitations,
        ),
    ];
    for (label, limitations) in scopes {
        if limitations.is_empty() {
            continue;
        }
        writeln!(writer)?;
        writeln!(writer, "{label}")?;
        for limitation in limitations {
            writeln!(
                writer,
                "- {}",
                format_market_data_limitation_warning(limitation)
            )?;
        }
    }
    Ok(())
}

fn nav_history_limitations(view: &PortfolioView) -> &[MarketDataLimitation] {
    match &view.performance {
        PortfolioPerformance::Available(available) => &available.nav_history_limitations,
        PortfolioPerformance::Unavailable {
            nav_history_limitations,
        } => nav_history_limitations,
        PortfolioPerformance::NotApplicable => &[],
    }
}

fn benchmark_risk_limitations(view: &PortfolioView) -> &[MarketDataLimitation] {
    match &view.performance {
        PortfolioPerformance::Available(available) => &available.benchmark_risk_limitations,
        _ => &[],
    }
}

// --- Fact and amount rendering helpers ---

/// One fact's human label: the rendered value when available, otherwise the
/// semantic state label that distinguishes not applicable from unavailable.
fn fact_or_label<T>(fact: &FactAvailability<T>, render: impl FnOnce(&T) -> String) -> String {
    match fact {
        FactAvailability::Available(value) => render(value),
        FactAvailability::Unavailable => "unavailable".to_owned(),
        FactAvailability::NotApplicable => "not applicable".to_owned(),
    }
}

fn section_aggregates_line(aggregates: &InventorySectionAggregates) -> String {
    format!(
        "Invested: {}  Value: {}  Lifetime Dividends: {}  Open-position Gain/Loss: {}",
        fact_or_label(&aggregates.invested_cost, |value| format_amount(*value)),
        fact_or_label(&aggregates.current_value, |value| format_amount(*value)),
        fact_or_label(&aggregates.dividends, |value| format_amount(*value)),
        combined_gain_loss(aggregates),
    )
}

fn combined_gain_loss(aggregates: &InventorySectionAggregates) -> String {
    match (
        aggregates.open_position_gain_loss.value(),
        aggregates.open_position_gain_loss_pct.value(),
    ) {
        (Some(gain_loss), Some(pct)) => color_value(
            *gain_loss,
            &format!("{} ({})", signed_amount(*gain_loss), signed_percent(*pct)),
        ),
        (Some(gain_loss), None) => signed_amount(*gain_loss),
        // When only the percentage exists, or when both are absent, each
        // absent state keeps its own label instead of assuming one reason.
        (None, _) => fact_or_label(&aggregates.open_position_gain_loss_pct, |pct| {
            signed_percent(*pct)
        }),
    }
}

fn format_amount(value: f64) -> String {
    format_eu(&format!("{value:.2}"))
}

fn signed_amount(value: f64) -> String {
    format_eu(&format!("{}{value:.2}", sign(value)))
}

fn signed_percent(value: f64) -> String {
    format_eu(&format!("{}{value:.2}%", sign(value)))
}

fn sign(value: f64) -> &'static str {
    if value >= 0.0 {
        "+"
    } else {
        ""
    }
}

fn apply_modern_style(table: &mut Table) {
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );
}

// --- Table rows ---

/// One position row of either section. The Weight cell stays empty for the
/// Monetary section, whose holdings are excluded from performance weights.
fn position_row(
    position: &InventoryPosition,
    performance_total: Option<f64>,
    with_weight: bool,
) -> PositionRow {
    let weight = if with_weight {
        match (position.current_value.value(), performance_total) {
            (Some(value), Some(total)) if total.abs() > f64::EPSILON => {
                format_eu(&format!("{:.2}%", value / total * 100.0))
            }
            _ => "unavailable".to_owned(),
        }
    } else {
        String::new()
    };
    PositionRow {
        ticker: if position.asset_type == AssetType::Stock {
            position.ticker.clone()
        } else {
            String::new()
        },
        name: position.name.clone(),
        asset_type: position.asset_type.to_string(),
        currency: position.currency.clone(),
        quantity: format_quantity(position.quantity),
        avg_cost: fact_or_label(&position.average_cost, |value| format_amount(*value)),
        current_price: fact_or_label(&position.individual_price, |price| {
            format_amount(price.native_price)
        }),
        price_date: fact_or_label(&position.individual_price, |price| {
            display_date(&price.price_date)
        }),
        invested: fact_or_label(&position.invested_cost, |value| format_amount(*value)),
        value: fact_or_label(&position.current_value, |value| format_amount(*value)),
        dividends: fact_or_label(&position.dividends, |value| format_amount(*value)),
        open_position_gain_loss: fact_or_label(&position.open_position_gain_loss, |value| {
            signed_amount(*value)
        }),
        open_position_gain_loss_pct: fact_or_label(
            &position.open_position_gain_loss_pct,
            |value| signed_percent(*value),
        ),
        weight,
    }
}

fn format_quantity(quantity: f64) -> String {
    if quantity.fract() == 0.0 {
        format_eu(&format!("{}", quantity as i64))
    } else {
        format_eu(&format!("{quantity:.2}"))
    }
}

#[derive(tabled::Tabled)]
struct PositionRow {
    #[tabled(rename = "Ticker")]
    ticker: String,
    #[tabled(rename = "Name")]
    name: String,
    #[tabled(rename = "Type")]
    asset_type: String,
    #[tabled(rename = "Currency")]
    currency: String,
    #[tabled(rename = "Quantity")]
    quantity: String,
    #[tabled(rename = "Avg Cost")]
    avg_cost: String,
    #[tabled(rename = "Price")]
    current_price: String,
    #[tabled(rename = "Price Date")]
    price_date: String,
    #[tabled(rename = "Invested")]
    invested: String,
    #[tabled(rename = "Value")]
    value: String,
    #[tabled(rename = "Lifetime Dividends")]
    dividends: String,
    #[tabled(rename = "Open-position Gain/Loss")]
    open_position_gain_loss: String,
    #[tabled(rename = "Open-position Gain/Loss %")]
    open_position_gain_loss_pct: String,
    #[tabled(rename = "Weight")]
    weight: String,
}
