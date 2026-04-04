use sea_orm::DatabaseConnection;

use crate::db::repos::watchlist_repo;
use crate::models::WatchlistItem;

pub async fn add(db: &DatabaseConnection, ticker: &str, sector_etf: &str) -> anyhow::Result<()> {
    if watchlist_repo::find_by_ticker(db, ticker).await?.is_some() {
        anyhow::bail!("{ticker} is already in the watchlist");
    }
    watchlist_repo::insert(db, ticker, sector_etf).await?;
    println!("Added {ticker} with sector ETF {sector_etf} to watchlist.");
    Ok(())
}

pub async fn remove(db: &DatabaseConnection, ticker: &str) -> anyhow::Result<()> {
    if watchlist_repo::delete_by_ticker(db, ticker).await? {
        println!("Removed {ticker} from watchlist.");
    } else {
        anyhow::bail!("{ticker} is not in the watchlist");
    }
    Ok(())
}

pub async fn list(db: &DatabaseConnection) -> anyhow::Result<Vec<WatchlistItem>> {
    watchlist_repo::find_all(db).await
}

pub async fn get(db: &DatabaseConnection, ticker: &str) -> anyhow::Result<WatchlistItem> {
    watchlist_repo::find_by_ticker(db, ticker)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{ticker} is not in the watchlist. Add it first with: rstock monitor add --ticker {ticker} --sector-etf <ETF>"
            )
        })
}
