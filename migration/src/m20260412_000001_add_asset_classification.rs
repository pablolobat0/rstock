use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Assets::Table)
                    .add_column(ColumnDef::new(Assets::AssetClass).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Assets::Table)
                    .add_column(ColumnDef::new(Assets::EquityStyle).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Assets::Table)
                    .add_column(ColumnDef::new(Assets::BondCredit).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Assets::Table)
                    .add_column(ColumnDef::new(Assets::BondDuration).string().null())
                    .to_owned(),
            )
            .await?;

        manager
            .alter_table(
                Table::alter()
                    .table(Assets::Table)
                    .add_column(ColumnDef::new(Assets::Management).string().null())
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for col in [
            Assets::Management,
            Assets::BondDuration,
            Assets::BondCredit,
            Assets::EquityStyle,
            Assets::AssetClass,
        ] {
            manager
                .alter_table(
                    Table::alter()
                        .table(Assets::Table)
                        .drop_column(col)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }
}

#[derive(DeriveIden)]
enum Assets {
    Table,
    AssetClass,
    EquityStyle,
    BondCredit,
    BondDuration,
    Management,
}
