use std::fmt::Write;

use colored::Colorize;
use tabled::settings::object::Cell;
use tabled::settings::style::{HorizontalLine, VerticalLine};
use tabled::settings::{Color, Style};
use tabled::Table;
use textplots::{Chart, Plot, Shape};

use tabled::builder::Builder;

use crate::constants::{RSI_OVERBOUGHT, RSI_OVERSOLD};
use crate::db::repos::watchlist_repo::WatchlistItem;
use crate::models::monitor::{MomentumIndicators, MonitorReport};
use crate::models::{
    Asset, AssetRow, CorrelationMatrix, DirectHoldingRow, FundHoldingRow, HoldingsResult,
    PeriodMetrics, PortfolioResult, PortfolioRow, PortfolioSnapshot, PortfolioSummary,
};

fn format_qty(qty: f64) -> String {
    if qty.fract() == 0.0 {
        format!("{}", qty as i64)
    } else {
        format!("{qty:.4}")
    }
}

fn color_value(value: f64, formatted: &str) -> String {
    if value >= 0.0 {
        formatted.green().to_string()
    } else {
        formatted.red().to_string()
    }
}

fn format_pct(v: Option<f64>) -> String {
    match v {
        Some(val) => format!("{val:.2}%"),
        None => "N/A".to_string(),
    }
}

fn format_plain(v: Option<f64>) -> String {
    match v {
        Some(val) => format!("{val:.2}"),
        None => "N/A".to_string(),
    }
}

fn format_return_plain(r: Option<f64>) -> String {
    match r {
        Some(v) => {
            let sign = if v >= 0.0 { "+" } else { "" };
            format!("{sign}{v:.2}%")
        }
        None => "N/A".to_string(),
    }
}

fn color_for_value(v: f64) -> Color {
    if v >= 0.0 {
        Color::FG_GREEN
    } else {
        Color::FG_RED
    }
}

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

                let gl_text = format!("{}{:.2}", sign, r.gain_loss);
                let gl_pct_text = format!("{}{:.2}%", sign, r.gain_loss_pct);

                let divs_text = if r.dividends_received > 0.0 {
                    format!("{:.2}", r.dividends_received)
                } else {
                    String::new()
                };

                PortfolioRow {
                    ticker: r.ticker.clone(),
                    name: r.name.clone(),
                    asset_type: r.asset_type.to_string(),
                    currency: r.currency.clone(),
                    quantity: format_qty(r.total_qty),
                    avg_cost: format!("{:.2}", r.avg_cost),
                    current_price: format!("{:.2}", r.current_price),
                    price_date: r.price_date.clone(),
                    total_invested: format!("{:.2}", r.total_invested),
                    current_value: format!("{:.2}", r.current_value),
                    dividends: divs_text,
                    gain_loss: gl_text,
                    gain_loss_pct: gl_pct_text,
                    weight,
                }
            })
            .collect();

        let mut table = Table::new(&display_rows);
        table.with(
            Style::modern()
                .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
                .verticals([(1, VerticalLine::inherit(Style::modern()))])
                .remove_horizontal()
                .remove_vertical(),
        );
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
            "{}{:.2} ({}{:.2}%)",
            sign, result.total_gain_loss, sign, result.total_gain_loss_pct
        );
        println!();
        let mut totals = format!(
            "Invested: {:.2}  Value: {:.2}",
            result.total_invested, result.total_current_value,
        );
        if result.total_dividends > 0.0 {
            let _ = write!(totals, "  Divs: {:.2}", result.total_dividends);
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
        println!("As of:          {}", summary.snapshot_date);
        println!("Portfolio Value: {:.2}", summary.total_value);
        println!("NAV:            {:.2}", summary.nav);

        if let (Some(change), Some(change_pct)) = (summary.daily_change, summary.daily_change_pct) {
            let sign = if change >= 0.0 { "+" } else { "" };
            let text = format!("{sign}{change:.2} ({sign}{change_pct:.2}%)");
            println!("Daily:          {}", color_value(change, &text));
        }

        if let Some(ref inception) = summary.inception_date {
            println!("Inception:      {inception}");
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

pub fn print_asset_list(assets: &[Asset]) {
    if assets.is_empty() {
        println!("No assets found.");
        return;
    }

    let rows: Vec<AssetRow> = assets
        .iter()
        .map(|a| AssetRow {
            ticker: a.ticker.clone(),
            name: a.name.clone(),
            asset_type: a.asset_type.to_string(),
            currency: a.currency.clone(),
            isin: a.isin.clone().unwrap_or_default(),
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

pub fn print_correlation_matrix(matrix: &CorrelationMatrix, period_label: &str) {
    if matrix.labels.is_empty() {
        println!("No assets found for correlation analysis.");
        return;
    }

    println!("\nCorrelation Matrix — {period_label}\n");

    let n = matrix.labels.len();

    // Build table with Builder for dynamic columns
    let mut builder = Builder::default();

    // Header row: empty cell + all tickers
    let mut header = vec![String::new()];
    header.extend(matrix.labels.iter().cloned());
    builder.push_record(header);

    // Data rows
    for i in 0..n {
        let mut row = vec![matrix.labels[i].clone()];
        for j in 0..n {
            let cell = match matrix.matrix[i][j] {
                Some(v) => format!("{v:.2}"),
                None => "N/A".to_string(),
            };
            row.push(cell);
        }
        builder.push_record(row);
    }

    let mut table = builder.build();
    table.with(
        Style::modern()
            .horizontals([(1, HorizontalLine::inherit(Style::modern()).horizontal('═'))])
            .verticals([(1, VerticalLine::inherit(Style::modern()))])
            .remove_horizontal()
            .remove_vertical(),
    );

    // Apply color coding per cell
    for i in 0..n {
        for j in 0..n {
            let color = match matrix.matrix[i][j] {
                None => Color::FG_BRIGHT_BLACK,
                Some(_) if i == j => Color::FG_WHITE,
                Some(v) if v.abs() > 0.7 => Color::FG_GREEN,
                Some(v) if v.abs() >= 0.3 => Color::FG_YELLOW,
                Some(_) => Color::FG_RED,
            };
            // +1 for header row, +1 for label column
            table.modify(Cell::new(i + 1, j + 1), color);
        }
    }

    println!("{table}");

    if !matrix.warnings.is_empty() {
        println!(
            "\nNote: insufficient data for {period_label}: {}",
            matrix.warnings.join(", ")
        );
    }
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
    println!("  {first_date}  →  {last_date}");
}

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
                current_value: format!("{:.2}", s.current_value),
                portfolio_weight: format!("{:.1}%", s.portfolio_weight),
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
        println!();
    }

    // Section 2: Each fund/ETF with its underlying holdings
    for fund in &result.funds {
        let header = format!(
            "{} ({}) — {:.1}% of portfolio, €{:.2}",
            fund.name, fund.ticker, fund.portfolio_weight, fund.current_value
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
                .map(|h| {
                    let effective = fund.portfolio_weight * h.weighting / 100.0;
                    FundHoldingRow {
                        ticker: h.ticker.clone(),
                        name: h.name.clone(),
                        fund_weight: format!("{:.2}%", h.weighting),
                        effective_weight: format!("{effective:.2}%"),
                    }
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
        }
        println!();
    }

    println!("Total portfolio value: {:.2}", result.total_portfolio_value);
}

fn format_volume(v: u64) -> String {
    if v >= 1_000_000_000 {
        format!("{:.1}B", v as f64 / 1_000_000_000.0)
    } else if v >= 1_000_000 {
        format!("{:.1}M", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.1}K", v as f64 / 1_000.0)
    } else {
        format!("{v}")
    }
}

fn format_market_cap(v: f64) -> String {
    if v >= 1e12 {
        format!("{:.2}T", v / 1e12)
    } else if v >= 1e9 {
        format!("{:.2}B", v / 1e9)
    } else if v >= 1e6 {
        format!("{:.2}M", v / 1e6)
    } else {
        format!("{v:.2}")
    }
}

fn rsi_label(rsi: f64) -> String {
    if rsi > RSI_OVERBOUGHT {
        "Overbought".red().to_string()
    } else if rsi < RSI_OVERSOLD {
        "Oversold".green().to_string()
    } else {
        "Neutral".to_string()
    }
}

fn print_momentum_section(label: &str, m: &MomentumIndicators) {
    println!("\n{label}");

    if let Some(rsi) = m.rsi_14 {
        println!("  RSI(14):    {rsi:>8.1}         [{}]", rsi_label(rsi));
    } else {
        println!("  RSI(14):         N/A");
    }

    if let Some(sma) = m.sma_50 {
        let signal = m.sma_50_signal.as_deref().unwrap_or("N/A");
        let arrow = if signal == "Above" { " ↑" } else { " ↓" };
        println!("  SMA(50):    {sma:>8.2}       Price {signal}{arrow}");
    } else {
        println!("  SMA(50):         N/A");
    }

    if let Some(sma) = m.sma_200 {
        let signal = m.sma_200_signal.as_deref().unwrap_or("N/A");
        let arrow = if signal == "Above" { " ↑" } else { " ↓" };
        println!("  SMA(200):   {sma:>8.2}       Price {signal}{arrow}");
    } else {
        println!("  SMA(200):        N/A");
    }

    if let Some(ref cross) = m.golden_death_cross {
        println!("  SMA Signal: {cross}");
    }

    if let (Some(ml), Some(ms), Some(mh)) = (m.macd_line, m.macd_signal, m.macd_histogram) {
        let signal_text = m.macd_signal_text.as_deref().unwrap_or("N/A");
        let sign = if mh >= 0.0 { "+" } else { "" };
        println!("  MACD:       {ml:>8.2}  Signal: {ms:.2}  Hist: {sign}{mh:.2}  [{signal_text}]");
    } else {
        println!("  MACD:            N/A");
    }
}

pub fn print_monitor_report(report: &MonitorReport) {
    let info = &report.stock_info;

    let name = info.name.as_deref().unwrap_or("Unknown");
    println!("\n══ {} — {} ══", info.ticker, name);

    let sector_str = info.sector.as_deref().unwrap_or("N/A");
    let industry_str = info.industry.as_deref().unwrap_or("N/A");
    println!("Sector: {sector_str}  |  Industry: {industry_str}");

    println!();
    let price_str = info
        .current_price
        .map_or("N/A".to_string(), |p| format!("{p:.2}"));
    let prev_str = info
        .previous_close
        .map_or("N/A".to_string(), |p| format!("{p:.2}"));

    let change_str = match (info.current_price, info.previous_close) {
        (Some(cur), Some(prev)) if prev > 0.0 => {
            let diff = cur - prev;
            let pct = diff / prev * 100.0;
            let sign = if diff >= 0.0 { "+" } else { "" };
            let text = format!("{sign}{diff:.2} ({sign}{pct:.2}%)");
            color_value(diff, &text)
        }
        _ => "N/A".to_string(),
    };

    let currency = info.currency.as_deref().unwrap_or("");
    println!("  Price: {price_str} {currency}  Prev Close: {prev_str}  Change: {change_str}");

    if let Some((lo, hi)) = info.day_range {
        print!("  Day Range: {lo:.2} – {hi:.2}");
    }
    if let Some((lo, hi)) = info.fifty_two_week_range {
        print!("    52W Range: {lo:.2} – {hi:.2}");
    }
    println!();

    let vol_str = info.volume.map_or("N/A".to_string(), format_volume);
    let avg_vol_str = info.avg_volume.map_or("N/A".to_string(), format_volume);
    print!("  Volume: {vol_str}  Avg Volume: {avg_vol_str}");

    if let Some(mc) = info.market_cap {
        print!("  Market Cap: {}", format_market_cap(mc));
    }
    println!();

    let pe_str = info.pe_ttm.map_or("N/A".to_string(), |v| format!("{v:.2}"));
    let eps_str = info
        .eps_ttm
        .map_or("N/A".to_string(), |v| format!("{v:.2}"));
    let div_str = info
        .dividend_yield
        .map_or("N/A".to_string(), |v| format!("{:.2}%", v * 100.0));
    println!("  P/E: {pe_str}  EPS: {eps_str}  Div Yield: {div_str}");

    let stock_label = format!("── Momentum: {} ──────────────────────", info.ticker);
    print_momentum_section(&stock_label, &report.stock_momentum);

    let sector_label = format!(
        "── Momentum: {} (Sector ETF) ─────────",
        report.sector_etf_ticker
    );
    print_momentum_section(&sector_label, &report.sector_momentum);

    println!(
        "\n── {} vs {} Relationship ────────────",
        info.ticker, report.sector_etf_ticker
    );
    let rel = &report.relationship;
    if let Some(rs) = rel.relative_strength_current {
        let change_str = rel.relative_strength_change.map_or(String::new(), |c| {
            let sign = if c >= 0.0 { "+" } else { "" };
            format!("  ({sign}{c:.1}% over period)")
        });
        println!("  Relative Strength: {rs:.2}{change_str}");
    }
    if let Some(beta) = rel.beta_vs_sector {
        println!("  Beta vs Sector:    {beta:.2}");
    }
    if let Some(corr) = rel.correlation {
        println!("  Correlation:       {corr:.2}");
    }

    print_normalized_chart(
        &report.stock_prices,
        &report.sector_prices,
        &info.ticker,
        &report.sector_etf_ticker,
        &report.period_label,
    );
}

fn print_normalized_chart(
    stock_prices: &[(String, f64)],
    sector_prices: &[(String, f64)],
    stock_ticker: &str,
    sector_ticker: &str,
    period_label: &str,
) {
    use std::collections::HashMap;

    if stock_prices.len() < 2 || sector_prices.len() < 2 {
        println!("\nNot enough data to display chart.");
        return;
    }

    let sector_map: HashMap<&str, f64> = sector_prices
        .iter()
        .map(|(d, p)| (d.as_str(), *p))
        .collect();

    let mut aligned: Vec<(&str, f64, f64)> = Vec::new();
    for (date, sp) in stock_prices {
        if let Some(&ep) = sector_map.get(date.as_str()) {
            aligned.push((date.as_str(), *sp, ep));
        }
    }

    if aligned.len() < 2 {
        println!("\nNot enough aligned data to display chart.");
        return;
    }

    let base_stock = aligned[0].1;
    let base_sector = aligned[0].2;

    if base_stock <= 0.0 || base_sector <= 0.0 {
        println!("\nInvalid base prices for normalization.");
        return;
    }

    let stock_points: Vec<(f32, f32)> = aligned
        .iter()
        .enumerate()
        .map(|(i, (_, sp, _))| (i as f32, (sp / base_stock * 100.0) as f32))
        .collect();

    let sector_points: Vec<(f32, f32)> = aligned
        .iter()
        .enumerate()
        .map(|(i, (_, _, ep))| (i as f32, (ep / base_sector * 100.0) as f32))
        .collect();

    let xmax = (aligned.len() - 1) as f32;
    let first_date = aligned[0].0;
    let last_date = aligned.last().expect("at least 2 aligned").0;

    println!("\n── Performance (normalized to 100) — {period_label} ──");
    Chart::new(180, 60, 0.0, xmax)
        .lineplot(&Shape::Lines(&stock_points))
        .lineplot(&Shape::Lines(&sector_points))
        .display();
    println!("  {stock_ticker}: ——  {sector_ticker}: ··");
    println!("  {first_date}  →  {last_date}");
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
