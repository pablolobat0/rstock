use std::collections::HashMap;

use chrono::Duration;

use crate::constants::{
    DATE_FORMAT, MACD_FAST, MACD_SIGNAL_PERIOD, MACD_SLOW, MONITOR_WARMUP_DAYS, RSI_PERIOD,
    SMA_LONG, SMA_SHORT,
};
use crate::models::monitor::{MomentumIndicators, MonitorReport, RelationshipMetrics, StockInfo};
use crate::models::AssetType;
use crate::services::price::PriceFetcher;

fn compute_rsi(prices: &[f64], period: usize) -> Option<f64> {
    if prices.len() < period + 1 {
        return None;
    }

    let changes: Vec<f64> = prices.windows(2).map(|w| w[1] - w[0]).collect();

    let (mut avg_gain, mut avg_loss) = {
        let mut gain_sum = 0.0;
        let mut loss_sum = 0.0;
        for &c in &changes[..period] {
            if c > 0.0 {
                gain_sum += c;
            } else {
                loss_sum += c.abs();
            }
        }
        (gain_sum / period as f64, loss_sum / period as f64)
    };

    for &c in &changes[period..] {
        let (gain, loss) = if c > 0.0 { (c, 0.0) } else { (0.0, c.abs()) };
        avg_gain = (avg_gain * (period as f64 - 1.0) + gain) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + loss) / period as f64;
    }

    if avg_loss < 1e-12 {
        return Some(100.0);
    }

    let rs = avg_gain / avg_loss;
    Some(100.0 - 100.0 / (1.0 + rs))
}

fn compute_sma(prices: &[f64], period: usize) -> Option<f64> {
    if prices.len() < period {
        return None;
    }
    let slice = &prices[prices.len() - period..];
    Some(slice.iter().sum::<f64>() / period as f64)
}

fn compute_ema_series(prices: &[f64], period: usize) -> Vec<f64> {
    if prices.len() < period {
        return Vec::new();
    }

    let multiplier = 2.0 / (period as f64 + 1.0);
    let mut ema = Vec::with_capacity(prices.len() - period + 1);

    let seed: f64 = prices[..period].iter().sum::<f64>() / period as f64;
    ema.push(seed);

    for &p in &prices[period..] {
        let prev = *ema.last().expect("ema has at least one element");
        ema.push((p - prev) * multiplier + prev);
    }

    ema
}

fn compute_macd(
    prices: &[f64],
    fast: usize,
    slow: usize,
    signal_period: usize,
) -> Option<(f64, f64, f64)> {
    let ema_fast = compute_ema_series(prices, fast);
    let ema_slow = compute_ema_series(prices, slow);

    if ema_fast.is_empty() || ema_slow.is_empty() {
        return None;
    }

    let offset = slow - fast;
    if ema_fast.len() <= offset {
        return None;
    }

    let macd_line: Vec<f64> = ema_fast[offset..]
        .iter()
        .zip(ema_slow.iter())
        .map(|(f, s)| f - s)
        .collect();

    if macd_line.len() < signal_period {
        return None;
    }

    let signal = compute_ema_series(&macd_line, signal_period);
    if signal.is_empty() {
        return None;
    }

    let last_macd = *macd_line.last()?;
    let last_signal = *signal.last()?;
    let histogram = last_macd - last_signal;

    Some((last_macd, last_signal, histogram))
}

pub fn compute_momentum(prices: &[f64]) -> MomentumIndicators {
    let current_price = prices.last().copied();
    let rsi_14 = compute_rsi(prices, RSI_PERIOD);
    let sma_50 = compute_sma(prices, SMA_SHORT);
    let sma_200 = compute_sma(prices, SMA_LONG);

    let sma_50_signal = match (current_price, sma_50) {
        (Some(p), Some(s)) if p > s => Some("Above".to_owned()),
        (Some(_), Some(_)) => Some("Below".to_owned()),
        _ => None,
    };

    let sma_200_signal = match (current_price, sma_200) {
        (Some(p), Some(s)) if p > s => Some("Above".to_owned()),
        (Some(_), Some(_)) => Some("Below".to_owned()),
        _ => None,
    };

    let golden_death_cross = if prices.len() > SMA_LONG {
        let prev_prices = &prices[..prices.len() - 1];
        let prev_sma_50 = compute_sma(prev_prices, SMA_SHORT);
        let prev_sma_200 = compute_sma(prev_prices, SMA_LONG);
        match (sma_50, sma_200, prev_sma_50, prev_sma_200) {
            (Some(s50), Some(s200), Some(ps50), Some(ps200)) => {
                if s50 > s200 && ps50 <= ps200 {
                    Some("Golden Cross".to_owned())
                } else if s50 < s200 && ps50 >= ps200 {
                    Some("Death Cross".to_owned())
                } else if s50 > s200 {
                    Some("SMA50 > SMA200".to_owned())
                } else {
                    Some("SMA50 < SMA200".to_owned())
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let (macd_line, macd_signal, macd_histogram, macd_signal_text) =
        match compute_macd(prices, MACD_FAST, MACD_SLOW, MACD_SIGNAL_PERIOD) {
            Some((ml, ms, mh)) => {
                let text = if ml > ms {
                    "Bullish".to_owned()
                } else {
                    "Bearish".to_owned()
                };
                (Some(ml), Some(ms), Some(mh), Some(text))
            }
            None => (None, None, None, None),
        };

    MomentumIndicators {
        rsi_14,
        sma_50,
        sma_200,
        sma_50_signal,
        sma_200_signal,
        golden_death_cross,
        macd_line,
        macd_signal,
        macd_histogram,
        macd_signal_text,
    }
}

pub fn compute_relationship(
    stock_prices: &[(String, f64)],
    sector_prices: &[(String, f64)],
) -> RelationshipMetrics {
    let sector_map: HashMap<&str, f64> = sector_prices
        .iter()
        .map(|(d, p)| (d.as_str(), *p))
        .collect();

    let mut aligned_stock = Vec::new();
    let mut aligned_sector = Vec::new();
    for (date, sp) in stock_prices {
        if let Some(&ep) = sector_map.get(date.as_str()) {
            aligned_stock.push(*sp);
            aligned_sector.push(ep);
        }
    }

    if aligned_stock.len() < 2 {
        return RelationshipMetrics {
            relative_strength_current: None,
            relative_strength_change: None,
            beta_vs_sector: None,
            correlation: None,
        };
    }

    let first_stock = aligned_stock[0];
    let first_sector = aligned_sector[0];
    let last_stock = *aligned_stock.last().expect("at least 2 elements");
    let last_sector = *aligned_sector.last().expect("at least 2 elements");

    let rs_start = if first_sector > 0.0 {
        first_stock / first_sector
    } else {
        0.0
    };
    let rs_end = if last_sector > 0.0 {
        last_stock / last_sector
    } else {
        0.0
    };
    let relative_strength_current = Some(rs_end);
    let relative_strength_change = if rs_start > 0.0 {
        Some((rs_end / rs_start - 1.0) * 100.0)
    } else {
        None
    };

    let stock_returns: Vec<f64> = aligned_stock
        .windows(2)
        .map(|w| (w[1] / w[0]).ln())
        .collect();
    let sector_returns: Vec<f64> = aligned_sector
        .windows(2)
        .map(|w| (w[1] / w[0]).ln())
        .collect();

    if stock_returns.len() < 20 {
        return RelationshipMetrics {
            relative_strength_current,
            relative_strength_change,
            beta_vs_sector: None,
            correlation: None,
        };
    }

    let n = stock_returns.len() as f64;
    let mean_stock = stock_returns.iter().sum::<f64>() / n;
    let mean_sector = sector_returns.iter().sum::<f64>() / n;

    let cov: f64 = stock_returns
        .iter()
        .zip(sector_returns.iter())
        .map(|(s, e)| (s - mean_stock) * (e - mean_sector))
        .sum::<f64>()
        / (n - 1.0);

    let var_sector: f64 = sector_returns
        .iter()
        .map(|e| (e - mean_sector).powi(2))
        .sum::<f64>()
        / (n - 1.0);

    let var_stock: f64 = stock_returns
        .iter()
        .map(|s| (s - mean_stock).powi(2))
        .sum::<f64>()
        / (n - 1.0);

    let beta_vs_sector = if var_sector > 0.0 {
        Some(cov / var_sector)
    } else {
        None
    };

    let std_stock = var_stock.sqrt();
    let std_sector = var_sector.sqrt();
    let correlation = if std_stock > 0.0 && std_sector > 0.0 {
        Some(cov / (std_stock * std_sector))
    } else {
        None
    };

    RelationshipMetrics {
        relative_strength_current,
        relative_strength_change,
        beta_vs_sector,
        correlation,
    }
}

pub async fn generate_monitor_report(
    ticker: &str,
    sector_etf_ticker: &str,
    period_days: i64,
    period_label: &str,
    price_fetcher: &dyn PriceFetcher,
) -> anyhow::Result<MonitorReport> {
    let today = chrono::Local::now().date_naive();
    let display_start = today - Duration::days(period_days);
    let fetch_start = today - Duration::days(period_days + MONITOR_WARMUP_DAYS);

    let fetch_start_str = fetch_start.format(DATE_FORMAT).to_string();
    let display_start_str = display_start.format(DATE_FORMAT).to_string();
    let end_str = today.format(DATE_FORMAT).to_string();

    let (stock_info_result, stock_prices_result, sector_prices_result) = tokio::join!(
        price_fetcher.get_stock_info(ticker),
        price_fetcher.get_historical_prices(ticker, &fetch_start_str, &end_str, &AssetType::Stock),
        price_fetcher.get_historical_prices(
            sector_etf_ticker,
            &fetch_start_str,
            &end_str,
            &AssetType::Stock,
        ),
    );

    let stock_info = stock_info_result?;
    let all_stock_prices = stock_prices_result?;
    let all_sector_prices = sector_prices_result?;

    let stock_closes: Vec<f64> = all_stock_prices.iter().map(|(_, p)| *p).collect();
    let sector_closes: Vec<f64> = all_sector_prices.iter().map(|(_, p)| *p).collect();

    let stock_momentum = compute_momentum(&stock_closes);
    let sector_momentum = compute_momentum(&sector_closes);

    let stock_prices: Vec<(String, f64)> = all_stock_prices
        .into_iter()
        .filter(|(d, _)| d.as_str() >= display_start_str.as_str())
        .collect();
    let sector_prices: Vec<(String, f64)> = all_sector_prices
        .into_iter()
        .filter(|(d, _)| d.as_str() >= display_start_str.as_str())
        .collect();

    let relationship = compute_relationship(&stock_prices, &sector_prices);

    let stock_info = if stock_info.fifty_two_week_range.is_none() {
        let one_year_ago = (today - Duration::days(365))
            .format(DATE_FORMAT)
            .to_string();
        let year_prices: Vec<f64> = stock_prices
            .iter()
            .filter(|(d, _)| d.as_str() >= one_year_ago.as_str())
            .map(|(_, p)| *p)
            .collect();
        if year_prices.len() >= 2 {
            let lo = year_prices.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = year_prices
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            StockInfo {
                fifty_two_week_range: Some((lo, hi)),
                ..stock_info
            }
        } else {
            stock_info
        }
    } else {
        stock_info
    };

    Ok(MonitorReport {
        stock_info,
        stock_momentum,
        sector_etf_ticker: sector_etf_ticker.to_owned(),
        sector_momentum,
        relationship,
        stock_prices,
        sector_prices,
        period_label: period_label.to_owned(),
    })
}
