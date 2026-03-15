use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::portfolio_history;
use crate::models::PortfolioSnapshot;

pub async fn find_latest(db: &DatabaseConnection) -> anyhow::Result<Option<PortfolioSnapshot>> {
    let result = portfolio_history::Entity::find()
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(PortfolioSnapshot::from))
}

pub async fn find_earliest(db: &DatabaseConnection) -> anyhow::Result<Option<PortfolioSnapshot>> {
    let result = portfolio_history::Entity::find()
        .order_by_asc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(PortfolioSnapshot::from))
}

pub async fn find_at_or_before(
    db: &DatabaseConnection,
    date: &str,
) -> anyhow::Result<Option<PortfolioSnapshot>> {
    let result = portfolio_history::Entity::find()
        .filter(portfolio_history::Column::Date.lte(date))
        .order_by_desc(portfolio_history::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(PortfolioSnapshot::from))
}

pub async fn upsert(db: &DatabaseConnection, snapshot: &PortfolioSnapshot) -> anyhow::Result<()> {
    let existing = portfolio_history::Entity::find_by_id(&snapshot.date)
        .one(db)
        .await?;

    if let Some(record) = existing {
        let mut active: portfolio_history::ActiveModel = record.into();
        active.asset_value = Set(snapshot.asset_value);
        active.total_value = Set(snapshot.total_value);
        active.outstanding_shares = Set(snapshot.outstanding_shares);
        active.nav = Set(snapshot.nav);
        active.update(db).await?;
    } else {
        let record = portfolio_history::ActiveModel {
            date: Set(snapshot.date.clone()),
            asset_value: Set(snapshot.asset_value),
            total_value: Set(snapshot.total_value),
            outstanding_shares: Set(snapshot.outstanding_shares),
            nav: Set(snapshot.nav),
        };
        record.insert(db).await?;
    }

    Ok(())
}

pub async fn find_between(
    db: &DatabaseConnection,
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

pub async fn delete_from_date(db: &DatabaseConnection, date: &str) -> anyhow::Result<()> {
    portfolio_history::Entity::delete_many()
        .filter(portfolio_history::Column::Date.gte(date))
        .exec(db)
        .await?;
    Ok(())
}
