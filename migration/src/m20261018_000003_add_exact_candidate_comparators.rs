use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)] pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.create_table(Table::create().table(BenchmarkCandidates::Table)
            .col(ColumnDef::new(BenchmarkCandidates::Vehicle).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::Identifier).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::DisplayName).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::OfficialComparator).string().null())
            .col(ColumnDef::new(BenchmarkCandidates::OfficialProvenance).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveComparator).string().null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveProvenance).string().not_null())
            .primary_key(Index::create().col(BenchmarkCandidates::Vehicle).col(BenchmarkCandidates::Identifier))
            .to_owned()).await?;
        let candidates = [
            ("fund","IE000ZYRH0Q7","iShares Developed World S EUR Acc",None,None),
            ("fund","IE00B03HD191","Vanguard Global Stock EUR Acc",None,None),
            ("fund","IE000QAZP7L2","iShares Emerging Markets S EUR Acc",None,None),
            ("fund","IE0031786696","Vanguard Emerging Markets EUR Acc",None,None),
            ("fund","LU1598719752","Cobas International P Acc EUR",Some("Official comparator provenance documented in source research; value retained separately"),None),
            ("fund","ES0119199018","Cobas Internacional FI D",Some("Official comparator provenance documented separately for this vehicle"),None),
            ("fund","LU3038481936","Hamco Global Value R EUR",None,None),
            ("fund","LU1694789535","DNCA Invest Alpha Bonds B EUR",None,Some("€STR + 1.20%; exact spread accrual convention unverified")),
            ("fund","LU1623762843","Carmignac Portfolio Credit A EUR Acc",Some("Official comparator variant/reset details unverified"),None),
            ("fund","IE00BFZMJT78","Neuberger Short Duration Euro Bond EUR I Acc",Some("Official comparator exact-class conventions unverified"),None),
            ("fund","IE00BD2B9710","Twelve Cat Bond P EUR Acc",Some("Official comparator identity/cadence documented; class qualification unknown"),None),
            ("fund","ES0140072028","Ábaco Renta Fija Mixta Global C",Some("Official comparator retained as distinct provenance; exact variants unverified"),None),
        ];
        for (vehicle, identifier, name, official, objective) in candidates {
            let official_sql = official.map(|value| format!("'{value}'")).unwrap_or_else(|| "NULL".to_owned());
            let objective_sql = objective.map(|value| format!("'{value}'")).unwrap_or_else(|| "NULL".to_owned());
            manager.get_connection().execute_unprepared(&format!("INSERT INTO benchmark_candidates (vehicle,identifier,display_name,official_comparator,official_provenance,objective_comparator,objective_provenance) VALUES ('{vehicle}','{identifier}','{name}',{official_sql},'candidate-specific; absent/unknown remains explicit',{objective_sql},'candidate-specific; absence is not inferred from selected reference')")).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(BenchmarkCandidates::Table).to_owned()).await
    }
}

#[derive(DeriveIden)] enum BenchmarkCandidates { Table, Vehicle, Identifier, DisplayName, OfficialComparator, OfficialProvenance, ObjectiveComparator, ObjectiveProvenance }
