pub use sea_orm_migration::prelude::*;

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

pub struct Migrator;

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
        ]
    }
}
