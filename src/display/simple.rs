use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::Style;
use tabled::Table;
use textplots::{Chart, Plot, Shape};

use crate::constants::display_date;
use crate::db::repos::watchlist_repo::WatchlistItem;
use crate::models::{Asset, AssetRow, AssetType, PortfolioSnapshot};

pub fn print_asset_list(assets: &[Asset]) {
    if assets.is_empty() {
        println!("No assets found.");
        return;
    }

    let rows: Vec<AssetRow> = assets
        .iter()
        .map(|a| AssetRow {
            ticker: if a.asset_type == AssetType::Stock {
                a.ticker.clone()
            } else {
                String::new()
            },
            name: a.name.clone(),
            asset_type: a.asset_type.to_string(),
            currency: a.currency.clone(),
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
    println!("{table}");
    println!("\nTotal: {} assets", assets.len());
}

pub fn print_nav_chart(snapshots: &[PortfolioSnapshot], period_label: &str) {
    if snapshots.len() < 2 {
        println!("\nNot enough data to display NAV chart.");
        return;
    }

    let first_date = &snapshots[0].date;
    let last_date = &snapshots[snapshots.len() - 1].date;

    // Convert to (f32, f32) points: x = day index, y = nav
    let points: Vec<(f32, f32)> = snapshots
        .iter()
        .enumerate()
        .map(|(i, s)| (i as f32, s.nav as f32))
        .collect();

    let xmax = (snapshots.len() - 1) as f32;

    println!("\nNAV — {period_label}");
    Chart::new(180, 60, 0.0, xmax)
        .lineplot(&Shape::Lines(&points))
        .display();
    println!(
        "  {}  →  {}",
        display_date(first_date),
        display_date(last_date)
    );
}

pub fn print_watchlist(items: &[WatchlistItem]) {
    if items.is_empty() {
        println!("Watchlist is empty.");
        return;
    }

    for item in items {
        println!("  {} → Sector ETF: {}", item.ticker, item.sector_etf_ticker);
    }
    println!("\nTotal: {} stocks", items.len());
}
