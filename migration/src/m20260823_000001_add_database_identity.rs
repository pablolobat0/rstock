use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(RstockDatabaseIdentity::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(RstockDatabaseIdentity::DatabaseKey)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(RstockDatabaseIdentity::Id)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(RstockDatabaseIdentity::Revision)
                            .big_integer()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await?;
        manager
            .exec_stmt(
                Query::insert()
                    .into_table(RstockDatabaseIdentity::Table)
                    .columns([
                        RstockDatabaseIdentity::DatabaseKey,
                        RstockDatabaseIdentity::Id,
                        RstockDatabaseIdentity::Revision,
                    ])
                    .values_panic([
                        "database".into(),
                        Expr::cust("lower(hex(randomblob(16)))").into(),
                        0.into(),
                    ])
                    .on_conflict(
                        OnConflict::column(RstockDatabaseIdentity::DatabaseKey)
                            .do_nothing()
                            .to_owned(),
                    )
                    .to_owned(),
            )
            .await?;
        for sql in revision_triggers() {
            manager.get_connection().execute_unprepared(sql).await?;
        }
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        for name in revision_trigger_names() {
            manager
                .get_connection()
                .execute_unprepared(&format!("DROP TRIGGER IF EXISTS {name}"))
                .await?;
        }
        manager
            .drop_table(
                Table::drop()
                    .table(RstockDatabaseIdentity::Table)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum RstockDatabaseIdentity {
    Table,
    DatabaseKey,
    Id,
    Revision,
}

fn revision_triggers() -> [&'static str; 12] {
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

fn revision_trigger_names() -> [&'static str; 12] {
    [
        "rstock_revision_transactions_insert",
        "rstock_revision_transactions_update",
        "rstock_revision_transactions_delete",
        "rstock_revision_assets_insert",
        "rstock_revision_assets_update",
        "rstock_revision_assets_delete",
        "rstock_revision_portfolio_insert",
        "rstock_revision_portfolio_update",
        "rstock_revision_portfolio_delete",
        "rstock_revision_asset_history_insert",
        "rstock_revision_asset_history_update",
        "rstock_revision_asset_history_delete",
    ]
}
