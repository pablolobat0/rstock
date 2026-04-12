use colored::Colorize;
use tabled::settings::object::Columns;
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Alignment, Style};
use tabled::Table;

use crate::models::HoldingsResult;

use super::types::{DirectHoldingRow, FundHoldingRow};

use super::helpers::format_eu;

pub fn print_holdings(result: &HoldingsResult) {
    if result.stocks.is_empty() && result.funds.is_empty() {
        println!("No positions found.");
        return;
    }

    // Section 1: Directly held stocks
    if !result.stocks.is_empty() {
        println!("{}", "Stocks".bold());
        println!();

        let rows: Vec<DirectHoldingRow> = result
            .stocks
            .iter()
            .map(|s| DirectHoldingRow {
                ticker: s.ticker.clone(),
                name: s.name.clone(),
                current_value: format_eu(&format!("{:.2}", s.current_value)),
                portfolio_weight: format_eu(&format!("{:.2}%", s.portfolio_weight)),
            })
            .collect();

        let mut table = Table::new(&rows);
        table.with(
            Style::modern()
                .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                .verticals([(1, VerticalLine::inherit(Style::modern()))])
                .remove_horizontal()
                .remove_vertical(),
        );
        // Right-align Value(2) and Weight(3)
        table.modify(Columns::single(2), Alignment::right());
        table.modify(Columns::single(3), Alignment::right());
        println!("{table}");
        println!();
    }

    // Section 2: Each fund/ETF with its underlying holdings
    for fund in &result.funds {
        let header = format!(
            "{} ({}) — {}% of portfolio, €{}",
            fund.name,
            fund.ticker,
            format_eu(&format!("{:.2}", fund.portfolio_weight)),
            format_eu(&format!("{:.2}", fund.current_value)),
        );
        println!("{}", header.bold());
        println!();

        if let Some(ref err) = fund.error {
            println!("  Could not fetch holdings: {err}");
        } else if fund.holdings.is_empty() {
            println!("  No holdings data available.");
        } else {
            let rows: Vec<FundHoldingRow> = fund
                .holdings
                .iter()
                .map(|h| FundHoldingRow {
                    name: h.name.clone(),
                    weighting: format_eu(&format!("{:.2}%", h.weighting)),
                    ticker: h.ticker.clone().unwrap_or_default(),
                    country: h.country.clone().unwrap_or_default(),
                    sector: h.sector.clone().unwrap_or_default(),
                })
                .collect();

            let mut table = Table::new(&rows);
            table.with(
                Style::modern()
                    .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                    .verticals([(1, VerticalLine::inherit(Style::modern()))])
                    .remove_horizontal()
                    .remove_vertical(),
            );
            // Right-align Weight(1)
            table.modify(Columns::single(1), Alignment::right());
            println!("{table}");
        }
        println!();
    }

    println!(
        "Total portfolio value: {}",
        format_eu(&format!("{:.2}", result.total_portfolio_value))
    );
}
