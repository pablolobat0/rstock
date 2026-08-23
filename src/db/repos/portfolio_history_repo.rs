use sea_orm::{
    sea_query::OnConflict, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder,
    QuerySelect, Set,
};

use crate::db::entities::portfolio_history;
use crate::models::PortfolioSnapshot;

const BULK_WRITE_SIZE: usize = 100;

pub async fn find_latest(db: &impl ConnectionTrait) -> anyhow::Result<Option<PortfolioSnapshot>> {
    let result = portfolio_history::Entity::find()
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(PortfolioSnapshot::from))
}

pub async fn find_earliest(db: &impl ConnectionTrait) -> anyhow::Result<Option<PortfolioSnapshot>> {
    let result = portfolio_history::Entity::find()
        .order_by_asc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(PortfolioSnapshot::from))
}

pub async fn find_at_or_before(
    db: &impl ConnectionTrait,
    date: &str,
) -> anyhow::Result<Option<PortfolioSnapshot>> {
    let result = portfolio_history::Entity::find()
        .filter(portfolio_history::Column::Date.lte(date))
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(PortfolioSnapshot::from))
}

pub async fn upsert_many(
    db: &impl ConnectionTrait,
    snapshots: &[PortfolioSnapshot],
) -> anyhow::Result<()> {
    for chunk in snapshots.chunks(BULK_WRITE_SIZE) {
        portfolio_history::Entity::insert_many(chunk.iter().map(active_model))
            .on_conflict(native_conflict())
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

pub async fn find_between(
    db: &impl ConnectionTrait,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<PortfolioSnapshot>> {
    let results = portfolio_history::Entity::find()
        .filter(portfolio_history::Column::Date.gte(start_date))
        .filter(portfolio_history::Column::Date.lte(end_date))
        .order_by_asc(portfolio_history::Column::Date)
        .all(db)
        .await?;
    Ok(results.into_iter().map(PortfolioSnapshot::from).collect())
}

pub async fn find_dates_between(
    db: &impl ConnectionTrait,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<String>> {
    portfolio_history::Entity::find()
        .select_only()
        .column(portfolio_history::Column::Date)
        .filter(portfolio_history::Column::Date.gte(start_date))
        .filter(portfolio_history::Column::Date.lte(end_date))
        .order_by_asc(portfolio_history::Column::Date)
        .into_tuple::<String>()
        .all(db)
        .await
        .map_err(Into::into)
}

pub async fn delete_from_date(db: &impl ConnectionTrait, date: &str) -> anyhow::Result<()> {
    portfolio_history::Entity::delete_many()
        .filter(portfolio_history::Column::Date.gte(date))
        .exec(db)
        .await?;
    Ok(())
}

fn active_model(snapshot: &PortfolioSnapshot) -> portfolio_history::ActiveModel {
    portfolio_history::ActiveModel {
        date: Set(snapshot.date.clone()),
        asset_value: Set(snapshot.asset_value),
        total_value: Set(snapshot.total_value),
        outstanding_shares: Set(snapshot.outstanding_shares),
        nav: Set(snapshot.nav),
    }
}

fn native_conflict() -> OnConflict {
    OnConflict::column(portfolio_history::Column::Date)
        .update_columns([
            portfolio_history::Column::AssetValue,
            portfolio_history::Column::TotalValue,
            portfolio_history::Column::OutstandingShares,
            portfolio_history::Column::Nav,
        ])
        .to_owned()
}
