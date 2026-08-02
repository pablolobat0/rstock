use std::fmt::Write;

use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Color, Style};
use tabled::Table;

use crate::constants::{display_date, format_date};
use crate::models::{
    AssetType, MarketDataLimitation, MarketDataSubject, MonetaryPosition, PeriodMetrics,
    PortfolioResult,
};

use super::types::{MonetaryPortfolioRow, PortfolioRow};

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
        let total_current_value = result.total_current_value;
        let display_rows: Vec<PortfolioRow> = result
            .rows
            .iter()
            .map(|r| {
                let sign = if r.gain_loss >= 0.0 { "+" } else { "" };
                let weight = if total_current_value > 0.0 {
                    format_eu(&format!(
                        "{:.1}%",
                        (r.current_value / total_current_value) * 100.0
                    ))
                } else {
                    "0,0%".to_string()
                };

                let gl_text = format_eu(&format!("{}{:.2}", sign, r.gain_loss));
                let gl_pct_text = format_eu(&format!("{}{:.2}%", sign, r.gain_loss_pct));

                let divs_text = format_dividends(r.dividends_received);

                PortfolioRow {
                    ticker: if r.asset_type == AssetType::Stock {
                        r.ticker.clone()
                    } else {
                        String::new()
                    },
                    name: r.name.clone(),
                    asset_type: r.asset_type.to_string(),
                    currency: r.currency.clone(),
                    quantity: if r.total_qty.fract() == 0.0 {
                        format_eu(&format!("{}", r.total_qty as i64))
                    } else {
                        format_eu(&format!("{:.2}", r.total_qty))
                    },
                    avg_cost: format_eu(&format!("{:.2}", r.avg_cost)),
                    current_price: format_eu(&format!("{:.2}", r.current_price)),
                    price_date: display_date(&r.price_date),
                    total_invested: format_eu(&format!("{:.2}", r.total_invested)),
                    current_value: format_eu(&format!("{:.2}", r.current_value)),
                    dividends: divs_text,
                    gain_loss: gl_text,
                    gain_loss_pct: gl_pct_text,
                    weight,
                }
            })
            .collect();

        let mut sorted: Vec<_> = display_rows.into_iter().zip(result.rows.iter()).collect();
        sorted.sort_by(|(a, _), (b, _)| {
            let wa: f64 = a
                .weight
                .trim_end_matches('%')
                .replace('.', "")
                .replace(',', ".")
                .parse()
                .unwrap_or(0.0);
            let wb: f64 = b
                .weight
                .trim_end_matches('%')
                .replace('.', "")
                .replace(',', ".")
                .parse()
                .unwrap_or(0.0);
            wb.partial_cmp(&wa).unwrap_or(std::cmp::Ordering::Equal)
        });
        let (display_rows, sorted_rows): (Vec<_>, Vec<_>) = sorted.into_iter().unzip();

        let mut table = Table::new(&display_rows);
        table.with(
            Style::modern()
                .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                .verticals([(1, VerticalLine::inherit(Style::modern()))])
                .remove_horizontal()
                .remove_vertical(),
        );
        // Right-align numeric columns: Quantity(4) through Weight(13)
        for col in 4..=13 {
            table.modify(Columns::single(col), Alignment::right());
        }
        for (i, r) in sorted_rows.iter().enumerate() {
            let color = if r.gain_loss >= 0.0 {
                Color::FG_GREEN
            } else {
                Color::FG_RED
            };
            // G/L = column 11, G/L % = column 12 (after Divs column)
            table.modify(Cell::new(i + 1, 11), color.clone());
            table.modify(Cell::new(i + 1, 12), color);
        }
        println!("{table}");

        let sign = if result.total_gain_loss >= 0.0 {
            "+"
        } else {
            ""
        };
        let gl_text = format!(
            "{} ({})",
            format_eu(&format!("{}{:.2}", sign, result.total_gain_loss)),
            format_eu(&format!("{}{:.2}%", sign, result.total_gain_loss_pct)),
        );
        println!();
        let mut totals = format!(
            "Invested: {}  Value: {}",
            format_eu(&format!("{:.2}", result.total_invested)),
            format_eu(&format!("{:.2}", result.total_current_value)),
        );
        if result.total_dividends > 0.0 {
            let _ = write!(
                totals,
                "  Divs: {}",
                format_eu(&format!("{:.2}", result.total_dividends))
            );
        }
        let _ = write!(
            totals,
            "  Open-position Gain/Loss: {}",
            color_value(result.total_gain_loss, &gl_text)
        );
        println!("{totals}");
    }

    if !result.monetary_positions.is_empty() {
        print_monetary_positions(result);
    }

    if let Some(ref snapshot_date) = result.snapshot_date {
        println!();
        println!("As of:          {}", display_date(snapshot_date));
        println!(
            "Portfolio Value: {}",
            format_eu(&format!("{:.2}", result.total_current_value))
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

    if !result.market_data_limitations.is_empty() {
        println!();
        println!("Market data limitations:");
        for limitation in &result.market_data_limitations {
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

    let mut positions: Vec<&MonetaryPosition> = result.monetary_positions.iter().collect();
    positions.sort_by(|left, right| {
        right
            .current_value
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&left.current_value.unwrap_or(f64::NEG_INFINITY))
    });
    let display_rows: Vec<MonetaryPortfolioRow> = positions
        .iter()
        .map(|position| monetary_display_row(position))
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
        if let Some(gain_loss) = position.gain_loss {
            let color = if gain_loss >= 0.0 {
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
            format_eu(&format!("{:.2}", result.total_current_value + value))
        );
    } else {
        println!("Monetary value: unavailable");
        println!("Total value: unavailable");
    }
}

fn monetary_display_row(position: &MonetaryPosition) -> MonetaryPortfolioRow {
    let gain_loss = position.gain_loss.map(|value| {
        let sign = if value >= 0.0 { "+" } else { "" };
        format_eu(&format!("{sign}{value:.2}"))
    });
    let gain_loss_pct = position.gain_loss_pct.map(|value| {
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
            .map(display_date)
            .unwrap_or_default(),
        total_invested: format_optional_amount(position.total_invested),
        current_value: format_optional_amount(position.current_value),
        dividends: position
            .dividends_received
            .map(format_dividends)
            .unwrap_or_default(),
        gain_loss: gain_loss.unwrap_or_default(),
        gain_loss_pct: gain_loss_pct.unwrap_or_default(),
    }
}

fn format_optional_amount(value: Option<f64>) -> String {
    value
        .map(|value| format_eu(&format!("{value:.2}")))
        .unwrap_or_default()
}

fn format_dividends(value: f64) -> String {
    format_eu(&format!("{value:.2}"))
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

#[cfg(test)]
mod tests {
    use tabled::Table;

    use super::{format_dividends, PortfolioRow};

    #[test]
    fn human_output_keeps_non_positive_lifetime_dividends_visible() {
        assert_eq!(format_dividends(0.0), "0,00");
        assert_eq!(format_dividends(-1.5), "-1,50");
    }

    #[test]
    fn human_output_names_open_position_gain_loss_and_lifetime_dividends() {
        let table = Table::new(vec![PortfolioRow {
            ticker: "XFAKE1".to_string(),
            name: "Fake holding".to_string(),
            asset_type: "stock".to_string(),
            currency: "EUR".to_string(),
            quantity: "1".to_string(),
            avg_cost: "10,00".to_string(),
            current_price: "10,00".to_string(),
            price_date: "10-06-2025".to_string(),
            total_invested: "10,00".to_string(),
            current_value: "10,00".to_string(),
            dividends: "0,00".to_string(),
            gain_loss: "0,00".to_string(),
            gain_loss_pct: "0,00%".to_string(),
            weight: "100,0%".to_string(),
        }])
        .to_string();

        assert!(table.contains("Lifetime Dividends"));
        assert!(table.contains("Open-position Gain/Loss"));
    }
}
