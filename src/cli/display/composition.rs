use colored::Colorize;
use tabled::settings::object::Columns;
use tabled::settings::style::HorizontalLine;
use tabled::settings::{Alignment, Style};
use tabled::{Table, Tabled};

use crate::models::{AllocationEntry, CompositionResult};

use super::helpers::format_eu;
use super::types::TopHoldingRow;

#[derive(Tabled)]
struct BreakdownRow {
    #[tabled(rename = "Category")]
    label: String,
    #[tabled(rename = "Weight")]
    weight: String,
}

fn print_breakdown(title: &str, entries: &[AllocationEntry]) {
    if entries.is_empty() {
        return;
    }

    println!("{}", title.bold());
    println!();

    let rows: Vec<BreakdownRow> = entries
        .iter()
        .map(|e| BreakdownRow {
            label: e.label.clone(),
            weight: format_eu(&format!("{:.2}%", e.weight)),
        })
        .collect();

    let mut table = Table::new(&rows);
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .remove_horizontal()
            .remove_vertical(),
    );
    table.modify(Columns::single(1), Alignment::right());
    println!("{table}");
    println!();
}

pub fn print_composition(result: &CompositionResult) {
    println!();
    println!("{}", "Portfolio Composition Analysis".bold().underline());
    println!();

    print_breakdown("Asset Class", &result.asset_class_breakdown);
    print_breakdown("Equity Style", &result.equity_style_breakdown);
    print_breakdown("Management", &result.management_breakdown);
    print_breakdown("Sector Allocation", &result.sector_breakdown);
    print_breakdown("Country Allocation", &result.country_breakdown);
    print_breakdown("Market Cap", &result.market_cap_breakdown);

    if !result.top_holdings.is_empty() {
        println!("{}", "Top Holdings".bold());
        println!();

        let rows: Vec<TopHoldingRow> = result
            .top_holdings
            .iter()
            .map(|h| TopHoldingRow {
                name: h.name.clone(),
                ticker: h.ticker.clone().unwrap_or_default(),
                weight: format_eu(&format!("{:.2}%", h.weight)),
                country: h.country.clone().unwrap_or_default(),
                sector: h.sector.clone().unwrap_or_default(),
            })
            .collect();

        let mut table = Table::new(&rows);
        table.with(
            Style::modern()
                .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                .remove_horizontal()
                .remove_vertical(),
        );
        table.modify(Columns::single(2), Alignment::right());
        println!("{table}");
        println!();
    }

    if !result.warnings.is_empty() {
        println!("{}", "Warnings:".yellow());
        println!();
        for w in &result.warnings {
            println!("  - {w}");
        }
    }
    println!();
}
