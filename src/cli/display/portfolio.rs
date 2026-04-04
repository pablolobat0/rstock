use std::fmt::Write;

use tabled::builder::Builder;
use tabled::settings::object::{Cell, Columns};
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Color, Style};
use tabled::Table;

use crate::constants::display_date;
use crate::models::{AssetType, PeriodMetrics, PortfolioResult, PortfolioSummary};

use super::types::PortfolioRow;

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

    // Beta (row 5)
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
    }

    println!("{table}");
}

#[allow(clippy::too_many_lines)]
pub fn print_portfolio(result: &PortfolioResult, summary: Option<&PortfolioSummary>) {
    if result.rows.is_empty() {
        println!("No positions found.");
    } else {
        let total_current_value = result.total_current_value;
        let mut display_rows: Vec<PortfolioRow> = result
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

                let divs_text = if r.dividends_received > 0.0 {
                    format_eu(&format!("{:.2}", r.dividends_received))
                } else {
                    String::new()
                };

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

        display_rows.sort_by(|a, b| {
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
        for (i, r) in result.rows.iter().enumerate() {
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
            "  G/L: {}",
            color_value(result.total_gain_loss, &gl_text)
        );
        println!("{totals}");
    }

    if let Some(summary) = summary {
        println!();
        println!("As of:          {}", display_date(&summary.snapshot_date));
        println!(
            "Portfolio Value: {}",
            format_eu(&format!("{:.2}", summary.total_value))
        );
        println!(
            "NAV:            {}",
            format_eu(&format!("{:.2}", summary.nav))
        );

        if let (Some(change), Some(change_pct)) = (summary.daily_change, summary.daily_change_pct) {
            let sign = if change >= 0.0 { "+" } else { "" };
            let text = format!(
                "{} ({})",
                format_eu(&format!("{sign}{change:.2}")),
                format_eu(&format!("{sign}{change_pct:.2}%")),
            );
            println!("Daily:          {}", color_value(change, &text));
        }

        if let Some(ref inception) = summary.inception_date {
            println!("Inception:      {}", display_date(inception));
        }

        let periods = [
            ("YTD", summary.ytd_return, &summary.ytd_metrics),
            ("1Y", summary.one_year_return, &summary.one_year_metrics),
            (
                "3Y(CAGR)",
                summary.three_year_return,
                &summary.three_year_metrics,
            ),
            (
                "5Y(CAGR)",
                summary.five_year_return,
                &summary.five_year_metrics,
            ),
        ];

        print_metrics_table(&periods);
    }
}
