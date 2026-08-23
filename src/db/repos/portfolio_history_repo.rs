use anyhow::Context;
use sea_orm::{
    sea_query::OnConflict, ColumnTrait, ConnectionTrait, DatabaseConnection, EntityTrait,
    QueryFilter, QueryOrder, QuerySelect, Set, Statement, TransactionTrait,
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

pub async fn find_database_revision(db: &DatabaseConnection) -> anyhow::Result<(String, i64)> {
    let transaction = db.begin().await?;
    let (database_identity, fallback_identity) = match transaction
        .query_one(Statement::from_string(
            transaction.get_database_backend(),
            "SELECT id, revision FROM rstock_database_identity WHERE database_key = 'database'",
        ))
        .await
    {
        Ok(Some(row)) => {
            let identity = row.try_get("", "id")?;
            let revision = row.try_get("", "revision")?;
            ((identity, revision), false)
        }
        Ok(None) => anyhow::bail!("database identity table is empty"),
        Err(error) if error.to_string().contains("no such table") => {
            transaction
                .execute_unprepared(
                    "CREATE TABLE IF NOT EXISTS rstock_database_identity (database_key TEXT PRIMARY KEY NOT NULL, id TEXT NOT NULL, revision INTEGER NOT NULL)",
                )
                .await?;
            transaction
                .execute_unprepared(
                    "INSERT INTO rstock_database_identity (database_key, id, revision) SELECT 'database', lower(hex(randomblob(16))), 0 WHERE NOT EXISTS (SELECT 1 FROM rstock_database_identity WHERE database_key = 'database')",
                )
                .await?;
            for sql in revision_trigger_statements() {
                transaction.execute_unprepared(sql).await?;
            }
            let row = transaction
                .query_one(Statement::from_string(
                    transaction.get_database_backend(),
                    "SELECT id, revision FROM rstock_database_identity WHERE database_key = 'database'",
                ))
                .await?
                .context("missing database identity result")?;
            let identity = row.try_get("", "id")?;
            let revision = row.try_get("", "revision")?;
            ((identity, revision), true)
        }
        Err(error) => return Err(error.into()),
    };
    if fallback_identity {
        transaction.commit().await?;
    } else {
        transaction.rollback().await?;
    }
    Ok(database_identity)
}

fn revision_trigger_statements() -> [&'static str; 12] {
    [
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_transactions_insert AFTER INSERT ON transactions BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_transactions_update AFTER UPDATE ON transactions BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_transactions_delete AFTER DELETE ON transactions BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_assets_insert AFTER INSERT ON assets BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_assets_update AFTER UPDATE ON assets BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_assets_delete AFTER DELETE ON assets BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_portfolio_insert AFTER INSERT ON portfolio_history BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_portfolio_update AFTER UPDATE ON portfolio_history BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_portfolio_delete AFTER DELETE ON portfolio_history BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_asset_history_insert AFTER INSERT ON portfolio_asset_history BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_asset_history_update AFTER UPDATE ON portfolio_asset_history BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
        "CREATE TRIGGER IF NOT EXISTS rstock_revision_asset_history_delete AFTER DELETE ON portfolio_asset_history BEGIN UPDATE rstock_database_identity SET revision = revision + 1; END",
    ]
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
