use chrono::NaiveDate;
use sea_orm::DatabaseConnection;

use crate::constants::format_date;
use crate::models::{AssetInfo, AssetType, BuyOrder, DividendOrder, SellOrder, SplitOrder};
use crate::services;

#[allow(clippy::too_many_arguments)]
pub async fn buy(
    db: &DatabaseConnection,
    ticker: String,
    name: String,
    asset_type: AssetType,
    date: NaiveDate,
    quantity: f64,
    price: f64,
    fees: f64,
    currency: String,
) -> anyhow::Result<()> {
    let asset = AssetInfo {
        ticker,
        name,
        asset_type,
        currency,
    };
    let order = BuyOrder {
        date: format_date(date),
        quantity,
        price,
        fees,
    };
    services::transactions::buy(db, asset, order).await
}

pub async fn sell(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    quantity: f64,
    price: f64,
    fees: f64,
) -> anyhow::Result<()> {
    let order = SellOrder {
        date: format_date(date),
        quantity,
        price,
        fees,
    };
    services::transactions::sell(db, ticker, order).await
}

pub async fn dividend(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    amount: f64,
    fees: f64,
) -> anyhow::Result<()> {
    let order = DividendOrder {
        date: format_date(date),
        amount,
        fees,
    };
    services::transactions::dividend(db, ticker, order).await
}

pub async fn split(
    db: &DatabaseConnection,
    ticker: String,
    date: NaiveDate,
    ratio: f64,
) -> anyhow::Result<()> {
    let order = SplitOrder {
        date: format_date(date),
        ratio,
    };
    services::transactions::split(db, ticker, order).await
}
