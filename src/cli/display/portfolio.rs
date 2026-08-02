use std::fmt::Write;

use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Color, Style};
use tabled::Table;

use crate::constants::{display_date, format_date};
use crate::models::{
    AssetType, CurrentPosition, MarketDataLimitation, MarketDataSubject, PeriodMetrics,
    PortfolioResult,
};

use super::types::MonetaryPortfolioRow;

use super::helpers::{
    color_for_value, color_value, format_eu, format_pct, format_plain, format_return_plain,
};

fn print_metrics_table(periods: &[(&str, Option<f64>, &Option<PeriodMetrics>)]) {
    let mut builder = Builder::default();

    // Header row
    let mut header = vec![String::new()];
    header.extend(periods.iter().map(|(name, _, _)| name.to_string()));
    builder.push_record(header);

    // Return (row 1)
    let mut row = vec!["Return".to_string()];
    row.extend(periods.iter().map(|(_, ret, _)| format_return_plain(*ret)));
    builder.push_record(row);

    // Volatility (row 2)
    let mut row = vec!["Volatility".to_string()];
    row.extend(
        periods
            .iter()
            .map(|(_, _, m)| format_pct(m.as_ref().and_then(|m| m.volatility))),
    );
    builder.push_record(row);

    // Max Drawdown (row 3)
    let mut row = vec!["Max Drawdown".to_string()];
    row.extend(
        periods
            .iter()
            .map(|(_, _, m)| format_pct(m.as_ref().and_then(|m| m.max_drawdown))),
    );
    builder.push_record(row);

    // Sharpe (row 4)
    let mut row = vec!["Sharpe".to_string()];
    row.extend(
        periods
            .iter()
            .map(|(_, _, m)| format_plain(m.as_ref().and_then(|m| m.sharpe))),
    );
    builder.push_record(row);

    // Sortino (row 5)
    let mut row = vec!["Sortino".to_string()];
    row.extend(
        periods
            .iter()
            .map(|(_, _, m)| format_plain(m.as_ref().and_then(|m| m.sortino))),
    );
    builder.push_record(row);

    // Beta (row 6)
    let mut row = vec!["Beta".to_string()];
    row.extend(
        periods
            .iter()
            .map(|(_, _, m)| format_plain(m.as_ref().and_then(|m| m.beta))),
    );
    builder.push_record(row);

    let mut table = builder.build();
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );

    // Apply colors after building so ANSI codes don't break alignment
    for (col, (_, ret, metrics)) in periods.iter().enumerate() {
        let col = col + 1; // offset for label column

        // Return
        if let Some(v) = ret {
            table.modify(Cell::new(1, col), color_for_value(*v));
        }

        // Max Drawdown — always red
        if metrics.as_ref().and_then(|m| m.max_drawdown).is_some() {
            table.modify(Cell::new(3, col), Color::FG_RED);
        }

        // Sharpe
        if let Some(v) = metrics.as_ref().and_then(|m| m.sharpe) {
            table.modify(Cell::new(4, col), color_for_value(v));
        }

        // Sortino
        if let Some(v) = metrics.as_ref().and_then(|m| m.sortino) {
            table.modify(Cell::new(5, col), color_for_value(v));
        }
    }

    println!("{table}");
}

#[allow(clippy::too_many_lines)]
pub fn print_portfolio(result: &PortfolioResult) {
    if result.rows.is_empty() && result.monetary_positions.is_empty() {
        println!("No positions found.");
    } else if !result.rows.is_empty() && !result.monetary_positions.is_empty() {
        println!("Portfolio:");
    }

    if !result.rows.is_empty() {
        let mut sorted_rows: Vec<_> = result.rows.iter().collect();
        sorted_rows.sort_by(|left, right| {
            right
                .current_value
                .unwrap_or(f64::NEG_INFINITY)
                .total_cmp(&left.current_value.unwrap_or(f64::NEG_INFINITY))
        });
        let display_rows: Vec<_> = sorted_rows
            .iter()
            .map(|position| position_display_row(position))
            .collect();

        let mut table = Table::new(&display_rows);
        table.with(
            Style::modern()
                .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                .verticals([(1, VerticalLine::inherit(Style::modern()))])
                .remove_horizontal()
                .remove_vertical(),
        );
        // Right-align numeric columns from quantity through gain/loss percentage.
        for col in 4..=13 {
            table.modify(Columns::single(col), Alignment::right());
        }
        for (i, r) in sorted_rows.iter().enumerate() {
            let Some(gain_loss) = r.open_position_gain_loss else {
                continue;
            };
            let color = if gain_loss >= 0.0 {
                Color::FG_GREEN
            } else {
                Color::FG_RED
            };
            // Gain/loss and percentage follow the dividends column.
            table.modify(Cell::new(i + 1, 11), color.clone());
            table.modify(Cell::new(i + 1, 12), color);
        }
        println!("{table}");

        println!();
        println!("{}", portfolio_totals_summary(result));
    }

    if !result.monetary_positions.is_empty() {
        print_monetary_positions(result);
    }

    if let Some(ref snapshot_date) = result.snapshot_date {
        println!();
        println!("As of:          {}", display_date(snapshot_date));
        println!(
            "Performance positions value: {}",
            format_optional_amount(result.total_current_value)
        );
        if let Some(nav) = result.nav {
            println!("NAV:            {}", format_eu(&format!("{nav:.2}")));
        }

        if let (Some(change), Some(change_pct)) = (result.daily_change, result.daily_change_pct) {
            let sign = if change >= 0.0 { "+" } else { "" };
            let text = format!(
                "{} ({})",
                format_eu(&format!("{sign}{change:.2}")),
                format_eu(&format!("{sign}{change_pct:.2}%")),
            );
            println!("Daily:          {}", color_value(change, &text));
        }

        if let Some(ref inception) = result.inception_date {
            println!("Inception:      {}", display_date(inception));
        }

        let periods = [
            ("YTD", result.ytd_return, &result.ytd_metrics),
            ("1Y", result.one_year_return, &result.one_year_metrics),
            (
                "3Y(CAGR)",
                result.three_year_return,
                &result.three_year_metrics,
            ),
            (
                "5Y(CAGR)",
                result.five_year_return,
                &result.five_year_metrics,
            ),
        ];

        print_metrics_table(&periods);
    }

    if !result.nav_market_data_limitations.is_empty() {
        println!();
        println!("NAV/history market data limitations:");
        for limitation in &result.nav_market_data_limitations {
            let warning = format_market_data_limitation_warning(limitation);
            println!("- {warning}");
        }
    }

    if !result.current_position_market_data_limitations.is_empty() {
        println!();
        println!("Current-position market data limitations:");
        for limitation in &result.current_position_market_data_limitations {
            let warning = format_market_data_limitation_warning(limitation);
            println!("- {warning}");
        }
    }

    if !result.monetary_market_data_limitations.is_empty() {
        println!();
        println!("Monetary market data limitations:");
        for limitation in &result.monetary_market_data_limitations {
            let warning = format_market_data_limitation_warning(limitation);
            println!("- {warning}");
        }
    }
}

fn print_monetary_positions(result: &PortfolioResult) {
    println!();
    println!("Monetary holdings:");

    let mut positions: Vec<&CurrentPosition> = result.monetary_positions.iter().collect();
    positions.sort_by(|left, right| {
        right
            .current_value
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&left.current_value.unwrap_or(f64::NEG_INFINITY))
    });
    let display_rows: Vec<MonetaryPortfolioRow> = positions
        .iter()
        .map(|position| position_display_row(position))
        .collect();
    let mut table = Table::new(&display_rows);
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );
    for col in 4..=12 {
        table.modify(Columns::single(col), Alignment::right());
    }
    for (index, position) in positions.iter().enumerate() {
        if let Some(open_position_gain_loss) = position.open_position_gain_loss {
            let color = if open_position_gain_loss >= 0.0 {
                Color::FG_GREEN
            } else {
                Color::FG_RED
            };
            table.modify(Cell::new(index + 1, 11), color.clone());
            table.modify(Cell::new(index + 1, 12), color);
        }
    }
    println!("{table}");
    if let Some(value) = result.total_monetary_value {
        println!("Monetary value: {}", format_eu(&format!("{value:.2}")));
        println!(
            "Total value: {}",
            format_optional_amount(result.total_value)
        );
    } else {
        println!("Monetary value: unavailable");
        println!("Total value: unavailable");
    }
}

fn position_display_row(position: &CurrentPosition) -> MonetaryPortfolioRow {
    let open_position_gain_loss = position.open_position_gain_loss.map(|value| {
        let sign = if value >= 0.0 { "+" } else { "" };
        format_eu(&format!("{sign}{value:.2}"))
    });
    let open_position_gain_loss_pct = position.open_position_gain_loss_pct.map(|value| {
        let sign = if value >= 0.0 { "+" } else { "" };
        format_eu(&format!("{sign}{value:.2}%"))
    });

    MonetaryPortfolioRow {
        ticker: if position.asset_type == AssetType::Stock {
            position.ticker.clone()
        } else {
            String::new()
        },
        name: position.name.clone(),
        asset_type: position.asset_type.to_string(),
        currency: position.currency.clone(),
        quantity: if position.total_qty.fract() == 0.0 {
            format_eu(&format!("{}", position.total_qty as i64))
        } else {
            format_eu(&format!("{:.2}", position.total_qty))
        },
        avg_cost: format_optional_amount(position.avg_cost),
        current_price: format_optional_amount(position.current_price),
        price_date: position
            .price_date
            .as_deref()
            .map_or_else(|| "unavailable".to_string(), display_date),
        total_invested: format_optional_amount(position.total_invested),
        current_value: format_optional_amount(position.current_value),
        dividends: position
            .dividends_received
            .map_or_else(|| "unavailable".to_string(), format_dividends),
        open_position_gain_loss: open_position_gain_loss
            .unwrap_or_else(|| "unavailable".to_string()),
        open_position_gain_loss_pct: open_position_gain_loss_pct
            .unwrap_or_else(|| "unavailable".to_string()),
    }
}

fn format_optional_amount(value: Option<f64>) -> String {
    value.map_or_else(
        || "unavailable".to_string(),
        |value| format_eu(&format!("{value:.2}")),
    )
}

fn format_dividends(value: f64) -> String {
    format_eu(&format!("{value:.2}"))
}

fn portfolio_totals_summary(result: &PortfolioResult) -> String {
    let gl_text = result
        .total_open_position_gain_loss
        .zip(result.total_open_position_gain_loss_pct)
        .map_or_else(
            || "unavailable".to_string(),
            |(gain_loss, pct)| {
                let sign = if gain_loss >= 0.0 { "+" } else { "" };
                format!(
                    "{} ({})",
                    format_eu(&format!("{sign}{gain_loss:.2}")),
                    format_eu(&format!("{sign}{pct:.2}%"))
                )
            },
        );
    let mut totals = format!(
        "Invested: {}  Value: {}",
        format_optional_amount(result.total_invested),
        format_optional_amount(result.total_current_value),
    );
    let _ = write!(
        totals,
        "  Lifetime Dividends: {}",
        result
            .total_dividends
            .map_or_else(|| "unavailable".to_string(), format_dividends)
    );
    let _ = write!(
        totals,
        "  Open-position Gain/Loss: {}",
        result
            .total_open_position_gain_loss
            .map(|value| color_value(value, &gl_text))
            .unwrap_or(gl_text)
    );
    totals
}

pub(super) fn format_market_data_limitation_warning(limitation: &MarketDataLimitation) -> String {
    let requested_end_date = display_date(&format_date(limitation.requested_end_date));

    if limitation.latest_available_date.is_none() {
        return match &limitation.subject {
            MarketDataSubject::Asset {
                ticker,
                name,
                asset_type,
            } => format!(
                "Market data limitation: {asset_type} {ticker} ({name}) has no available price through {requested_end_date}."
            ),
            MarketDataSubject::FxRate { currency } => format!(
                "Market data limitation: FX rate {currency} has no available rate through {requested_end_date}."
            ),
        };
    }

    let latest_available_date = limitation
        .latest_available_date
        .map(format_date)
        .map(|date| display_date(&date))
        .unwrap_or_default();

    match &limitation.subject {
        MarketDataSubject::Asset {
            ticker,
            name,
            asset_type,
        } => format!(
            "Market data limitation: {asset_type} {ticker} ({name}) has latest price from {latest_available_date}; requested through {requested_end_date}."
        ),
        MarketDataSubject::FxRate { currency } => format!(
            "Market data limitation: FX rate {currency} has latest rate from {latest_available_date}; requested through {requested_end_date}."
        ),
    }
}
