use sea_orm_migration::{prelude::*, schema::*};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(DailyExchangeRates::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(DailyExchangeRates::Table)
                    .col(pk_auto(DailyExchangeRates::Id))
                    .col(string(DailyExchangeRates::FromCurrency))
                    .col(string(DailyExchangeRates::ToCurrency))
                    .col(string(DailyExchangeRates::Date))
                    .col(double(DailyExchangeRates::Rate))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_daily_exchange_rates_currencies_date")
                    .table(DailyExchangeRates::Table)
                    .col(DailyExchangeRates::FromCurrency)
                    .col(DailyExchangeRates::ToCurrency)
                    .col(DailyExchangeRates::Date)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(
                Table::drop()
                    .table(DailyExchangeRates::Table)
                    .if_exists()
                    .to_owned(),
            )
            .await?;

        manager
            .create_table(
                Table::create()
                    .table(DailyExchangeRates::Table)
                    .col(pk_auto(DailyExchangeRates::Id))
                    .col(string(DailyExchangeRates::Pair))
                    .col(string(DailyExchangeRates::Date))
                    .col(double(DailyExchangeRates::Rate))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx_daily_exchange_rates_pair_date")
                    .table(DailyExchangeRates::Table)
                    .col(DailyExchangeRates::Pair)
                    .col(DailyExchangeRates::Date)
                    .unique()
                    .to_owned(),
            )
            .await?;

        Ok(())
    }
}

#[derive(DeriveIden)]
enum DailyExchangeRates {
    Table,
    Id,
    Pair,
    FromCurrency,
    ToCurrency,
    Date,
    Rate,
}
