use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, ModelTrait, QueryFilter,
    QueryOrder, Set,
};

use crate::db::entities::watchlist;
use crate::models::WatchlistItem;

impl From<watchlist::Model> for WatchlistItem {
    fn from(m: watchlist::Model) -> Self {
        Self {
            ticker: m.ticker,
            sector_etf_ticker: m.sector_etf_ticker,
        }
    }
}

pub async fn find_by_ticker(
    db: &DatabaseConnection,
    ticker: &str,
) -> anyhow::Result<Option<WatchlistItem>> {
    let result = watchlist::Entity::find()
        .filter(watchlist::Column::Ticker.eq(ticker))
        .one(db)
        .await?;
    Ok(result.map(WatchlistItem::from))
}

pub async fn find_all(db: &DatabaseConnection) -> anyhow::Result<Vec<WatchlistItem>> {
    let results = watchlist::Entity::find()
        .order_by_asc(watchlist::Column::Ticker)
        .all(db)
        .await?;
    Ok(results.into_iter().map(WatchlistItem::from).collect())
}

pub async fn insert(
    db: &DatabaseConnection,
    ticker: &str,
    sector_etf_ticker: &str,
) -> anyhow::Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let new = watchlist::ActiveModel {
        ticker: Set(ticker.to_owned()),
        sector_etf_ticker: Set(sector_etf_ticker.to_owned()),
        created_at: Set(now),
        ..Default::default()
    };
    new.insert(db).await?;
    Ok(())
}

pub async fn delete_by_ticker(db: &DatabaseConnection, ticker: &str) -> anyhow::Result<bool> {
    let item = watchlist::Entity::find()
        .filter(watchlist::Column::Ticker.eq(ticker))
        .one(db)
        .await?;
    match item {
        Some(model) => {
            model.delete(db).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}
