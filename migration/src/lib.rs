pub use sea_orm_migration::prelude::*;

use sea_orm_migration::prelude::sea_orm::{TransactionError, TransactionTrait};

mod m20220101_000001_create_table;
mod m20250224_000001_add_price_history;
mod m20250225_000001_portfolio_asset_history;
mod m20250308_000001_add_exchange_rates;
mod m20260330_000001_create_watchlist;
mod m20260401_000001_drop_isin;
mod m20260411_000001_add_morningstar_code;
mod m20260412_000001_add_asset_classification;
mod m20260413_000001_create_fund_holdings_snapshots;
mod m20260414_000001_drop_watchlist;
mod m20260521_000001_migrate_exchange_rates_to_currency_columns;
mod m20260815_000001_add_transaction_indexes;
mod m20260905_000001_contract_transaction_schema;

pub struct Migrator;

// SQLite's default SeaORM migration runner does not open a transaction. Keep
// the default implementation separate so the public runner can add one without
// recursively calling its own override.
struct RawMigrator;

fn migration_error(error: TransactionError<DbErr>) -> DbErr {
    match error {
        TransactionError::Connection(error) | TransactionError::Transaction(error) => error,
    }
}

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20250224_000001_add_price_history::Migration),
            Box::new(m20250225_000001_portfolio_asset_history::Migration),
            Box::new(m20250308_000001_add_exchange_rates::Migration),
            Box::new(m20260330_000001_create_watchlist::Migration),
            Box::new(m20260401_000001_drop_isin::Migration),
            Box::new(m20260411_000001_add_morningstar_code::Migration),
            Box::new(m20260412_000001_add_asset_classification::Migration),
            Box::new(m20260413_000001_create_fund_holdings_snapshots::Migration),
            Box::new(m20260414_000001_drop_watchlist::Migration),
            Box::new(m20260521_000001_migrate_exchange_rates_to_currency_columns::Migration),
            Box::new(m20260815_000001_add_transaction_indexes::Migration),
            Box::new(m20260905_000001_contract_transaction_schema::Migration),
        ]
    }

    async fn up<'c, C>(db: C, steps: Option<u32>) -> Result<(), DbErr>
    where
        C: IntoSchemaManagerConnection<'c>,
    {
        match db.into_schema_manager_connection() {
            SchemaManagerConnection::Connection(db) => db
                .transaction(|transaction| {
                    Box::pin(async move { RawMigrator::up(transaction, steps).await })
                })
                .await
                .map_err(migration_error),
            SchemaManagerConnection::Transaction(transaction) => {
                RawMigrator::up(transaction, steps).await
            }
        }
    }

    async fn down<'c, C>(db: C, steps: Option<u32>) -> Result<(), DbErr>
    where
        C: IntoSchemaManagerConnection<'c>,
    {
        match db.into_schema_manager_connection() {
            SchemaManagerConnection::Connection(db) => db
                .transaction(|transaction| {
                    Box::pin(async move { RawMigrator::down(transaction, steps).await })
                })
                .await
                .map_err(migration_error),
            SchemaManagerConnection::Transaction(transaction) => {
                RawMigrator::down(transaction, steps).await
            }
        }
    }
}

#[async_trait::async_trait]
impl MigratorTrait for RawMigrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        Migrator::migrations()
    }
}
