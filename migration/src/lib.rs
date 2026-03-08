pub use sea_orm_migration::prelude::*;

mod m20220101_000001_create_table;
mod m20250224_000001_add_price_history;
mod m20250225_000001_portfolio_asset_history;
mod m20250308_000001_add_exchange_rates;

pub struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(m20220101_000001_create_table::Migration),
            Box::new(m20250224_000001_add_price_history::Migration),
            Box::new(m20250225_000001_portfolio_asset_history::Migration),
            Box::new(m20250308_000001_add_exchange_rates::Migration),
        ]
    }
}
