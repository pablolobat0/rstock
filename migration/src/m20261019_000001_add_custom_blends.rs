use sea_orm_migration::prelude::*;
use sea_orm_migration::sea_orm::{DatabaseBackend, Statement};

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // A saved Custom blend is a benchmark definition with a distinct
        // identity: synthetic reference wealth is never presented as official
        // index data, an adjusted fund return, or a raw price.
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .add_column(
                        ColumnDef::new(BenchmarkReferences::Kind)
                            .string()
                            .not_null()
                            .default("reference"),
                    )
                    .to_owned(),
            )
            .await?;

        // Blend-specific definition facts. The definition row itself stays in
        // the reference catalog so a blend keeps one stable identity that
        // assignments and deletion guards can reference.
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkBlends::Table)
                    .col(
                        ColumnDef::new(BenchmarkBlends::ReferenceId)
                            .string()
                            .not_null()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(BenchmarkBlends::CreatedAt).string().not_null())
                    .col(ColumnDef::new(BenchmarkBlends::ResetRule).string().not_null())
                    .col(
                        ColumnDef::new(BenchmarkBlends::WeightConvention)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::CashConvention)
                            .string()
                            .not_null(),
                    )
                    .col(ColumnDef::new(BenchmarkBlends::WeightTolerance).double().not_null())
                    .col(
                        ColumnDef::new(BenchmarkBlends::RequestedStartDate)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::RequestedEndDate)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::AdmittedStartDate)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::AdmittedEndDate)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlends::AdmissionBasis)
                            .string()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_blends_reference")
                            .from(BenchmarkBlends::Table, BenchmarkBlends::ReferenceId)
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .to_owned(),
            )
            .await?;

        // Component convention snapshots keep a blend's meaning explicit and
        // fixed: the exact source and lookup identity, the adjustment basis,
        // the observation cadence, and the currency, return basis, and hedge
        // policy. Cash is synthetic: it carries no reference identity and
        // earns a 0% return.
        manager
            .create_table(
                Table::create()
                    .table(BenchmarkBlendComponents::Table)
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::BlendId)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentKey)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentReferenceId)
                            .string()
                            .null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentDisplayName)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::Position)
                            .integer()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::TargetWeight)
                            .double()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::IsCash)
                            .boolean()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentCurrency)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentReturnBasis)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentAdjustment)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentHedgePolicy)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentUniverse)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentSource)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentSourceIdentifier)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentCadence)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentSourceProvenance)
                            .string()
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(BenchmarkBlendComponents::ComponentVerificationState)
                            .string()
                            .not_null(),
                    )
                    .primary_key(
                        Index::create()
                            .col(BenchmarkBlendComponents::BlendId)
                            .col(BenchmarkBlendComponents::ComponentKey),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_blend_components_blend")
                            .from(BenchmarkBlendComponents::Table, BenchmarkBlendComponents::BlendId)
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk_benchmark_blend_components_reference")
                            .from(
                                BenchmarkBlendComponents::Table,
                                BenchmarkBlendComponents::ComponentReferenceId,
                            )
                            .to(BenchmarkReferences::Table, BenchmarkReferences::Id)
                            .on_delete(ForeignKeyAction::Restrict),
                    )
                    .to_owned(),
            )
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Rolling this migration back would drop the blend definition facts and
        // the definition kind, which would silently turn a saved Custom blend
        // into an ordinary catalog reference. Refuse the destructive rollback
        // while saved blends exist instead of losing their meaning.
        let saved_blends = manager
            .get_connection()
            .query_one(Statement::from_string(
                DatabaseBackend::Sqlite,
                "SELECT COUNT(*) AS count FROM benchmark_blends",
            ))
            .await?
            .map(|row| row.try_get::<i64>("", "count").unwrap_or_default())
            .unwrap_or_default();
        if saved_blends > 0 {
            return Err(DbErr::Custom(
                "the Custom blend migration cannot be rolled back while saved Custom blends exist: \
                 delete the saved blends first so no definition is silently converted into an ordinary \
                 catalog reference"
                    .to_owned(),
            ));
        }
        manager
            .drop_table(Table::drop().table(BenchmarkBlendComponents::Table).to_owned())
            .await?;
        manager
            .drop_table(Table::drop().table(BenchmarkBlends::Table).to_owned())
            .await?;
        manager
            .alter_table(
                Table::alter()
                    .table(BenchmarkReferences::Table)
                    .drop_column(BenchmarkReferences::Kind)
                    .to_owned(),
            )
            .await?;
        Ok(())
    }
}

#[derive(DeriveIden)]
enum BenchmarkReferences {
    Table,
    Id,
    Kind,
}

#[derive(DeriveIden)]
enum BenchmarkBlends {
    Table,
    ReferenceId,
    CreatedAt,
    ResetRule,
    WeightConvention,
    CashConvention,
    WeightTolerance,
    RequestedStartDate,
    RequestedEndDate,
    AdmittedStartDate,
    AdmittedEndDate,
    AdmissionBasis,
}

#[derive(DeriveIden)]
enum BenchmarkBlendComponents {
    Table,
    BlendId,
    ComponentKey,
    ComponentReferenceId,
    ComponentDisplayName,
    Position,
    TargetWeight,
    IsCash,
    ComponentCurrency,
    ComponentReturnBasis,
    ComponentAdjustment,
    ComponentHedgePolicy,
    ComponentUniverse,
    ComponentSource,
    ComponentSourceIdentifier,
    ComponentCadence,
    ComponentSourceProvenance,
    ComponentVerificationState,
}
