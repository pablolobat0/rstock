mod cli;
mod db;
mod models;
mod services;

use clap::Parser;
use cli::{Cli, Commands};
use models::{AssetInfo, BuyOrder, PortfolioRow};
use services::price::RealPriceFetcher;
use tabled::Table;

fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{}", qty as i64)
    } else {
        format!("{:.4}", qty)
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let db = db::connect().await?;
    let fetcher = RealPriceFetcher;

    match cli.command {
        Commands::Get => {
            let summary = services::portfolio::get_portfolio_summary(&db, &fetcher).await?;

            let result = services::portfolio::get_portfolio(&db).await?;

            if result.rows.is_empty() {
                println!("No positions found.");
            } else {
                let total_current_value = result.total_current_value;
                let display_rows: Vec<PortfolioRow> = result
                    .rows
                    .iter()
                    .map(|r| {
                        let sign = if r.gain_loss >= 0.0 { "+" } else { "" };
                        let weight = if total_current_value > 0.0 {
                            format!("{:.1}%", (r.current_value / total_current_value) * 100.0)
                        } else {
                            "0.0%".to_string()
                        };
                        PortfolioRow {
                            ticker: r.ticker.clone(),
                            name: r.name.clone(),
                            asset_type: r.asset_type.clone(),
                            currency: r.currency.clone(),
                            quantity: format_qty(r.total_qty),
                            avg_cost: format!("{:.2}", r.avg_cost),
                            current_price: format!("{:.2}", r.current_price),
                            total_invested: format!("{:.2}", r.total_invested),
                            current_value: format!("{:.2}", r.current_value),
                            gain_loss: format!("{}{:.2}", sign, r.gain_loss),
                            gain_loss_pct: format!("{}{:.2}%", sign, r.gain_loss_pct),
                            weight,
                        }
                    })
                    .collect();
                println!("{}", Table::new(&display_rows));

                let sign = if result.total_gain_loss >= 0.0 {
                    "+"
                } else {
                    ""
                };
                println!();
                println!(
                    "Invested: {:.2}  Value: {:.2}  G/L: {}{:.2} ({}{:.2}%)",
                    result.total_invested,
                    result.total_current_value,
                    sign,
                    result.total_gain_loss,
                    sign,
                    result.total_gain_loss_pct,
                );
            }

            if let Some(summary) = summary {
                println!();
                println!("Portfolio Value: {:.2}", summary.total_value);
                println!("NAV:            {:.2}", summary.nav);

                if let (Some(change), Some(change_pct)) =
                    (summary.daily_change, summary.daily_change_pct)
                {
                    let sign = if change >= 0.0 { "+" } else { "" };
                    println!(
                        "Daily:          {}{:.2} ({}{:.2}%)",
                        sign, change, sign, change_pct
                    );
                }

                if let Some(ref inception) = summary.inception_date {
                    println!("Inception:      {}", inception);
                }

                let fmt_ret = |r: Option<f64>| match r {
                    Some(v) => {
                        let sign = if v >= 0.0 { "+" } else { "" };
                        format!("{}{:.2}%", sign, v)
                    }
                    None => "N/A".to_string(),
                };

                println!(
                    "YTD: {}  1Y: {}  3Y(CAGR): {}  5Y(CAGR): {}",
                    fmt_ret(summary.ytd_return),
                    fmt_ret(summary.one_year_return),
                    fmt_ret(summary.three_year_return),
                    fmt_ret(summary.five_year_return),
                );
            }
        }
        Commands::Buy {
            ticker,
            name,
            asset_type,
            isin,
            date,
            quantity,
            price,
            fees,
            currency,
            notes,
        } => {
            let asset = AssetInfo {
                ticker,
                name,
                asset_type: asset_type.to_string(),
                isin,
                currency,
            };
            let order = BuyOrder {
                date,
                quantity,
                price,
                fees,
                notes,
            };
            services::transactions::buy(&db, asset, order).await?;
        }
    }

    Ok(())
}
