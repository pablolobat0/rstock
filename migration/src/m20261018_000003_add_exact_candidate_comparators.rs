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
            .col(ColumnDef::new(BenchmarkCandidates::OfficialDocumentDate).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveComparator).string().null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveProvenance).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::ObjectiveDocumentDate).string().not_null())
            .col(ColumnDef::new(BenchmarkCandidates::HistoryBoundary).string().not_null())
            .primary_key(Index::create().col(BenchmarkCandidates::Vehicle).col(BenchmarkCandidates::Identifier))
            .to_owned()).await?;
        let candidates = [
            ("fund","IE000ZYRH0Q7","iShares Developed World S EUR Acc",Some("MSCI World Index Net (EUR); exact official tracking target"),"FUND-EXAMPLES.md §2; W2 factsheet September 2026; W3 Key Facts dated 2026-10-15","2026-09 factsheet; 2026-10-15 key facts",None,"Not specified in the exact-class evidence summarized here","Unknown","Class history starts 2025-08-21; no pre-class history authorized"),
            ("fund","IE00B03HD191","Vanguard Global Stock EUR Acc",None,"Exact-class official variant not established in ticket evidence","Unknown",None,"No separate official objective comparator established","Unknown","Exact class history boundary unknown"),
            ("fund","IE000QAZP7L2","iShares Emerging Markets S EUR Acc",Some("MSCI Emerging Markets, Net Returns (EUR); exact official tracking target"),"FUND-EXAMPLES.md §3; E2 factsheet September 2026; E3 Key Facts","2026-09 factsheet; 2026-10-15 key facts",None,"Not specified in the exact-class evidence summarized here","Unknown","Class history starts 2025-08-21; no pre-class history authorized"),
            ("fund","IE0031786696","Vanguard Emerging Markets EUR Acc",None,"Exact-class official variant not established in ticket evidence","Unknown",None,"No separate official objective comparator established","Unknown","Exact class history boundary unknown"),
            ("fund","LU1598719752","Cobas International P Acc EUR",Some("MSCI Europe Total Net Return; exact series code/currency convention unreconciled"),"FUND-EXAMPLES.md §4; L1 KID 2023-01-01; L2 prospectus 2021-06 (visa 2021-07-01); L3 factsheet 2026-08","2026-08 factsheet; legal docs 2021/2023",None,"No manager objective comparator reported separately in cited evidence","Unknown","Exact class return history boundary not established by these sources"),
            ("fund","ES0119199018","Cobas Internacional FI D",Some("Bloomberg Europe Developed Markets Large & Mid Cap Net Return Index EUR (EDMNE Index); net-of-tax dividend reinvestment; comparison only"),"FUND-EXAMPLES.md §5; C1 prospectus updated 2026-06-22; C2 KID 2025-12-31","2026-06-22 prospectus; 2025-12-31 KID",None,"No separate manager objective comparator reported","Unknown","Manager class performance dates from 2021; index-change effective date/history unknown"),
            ("fund","LU3038481936","Hamco Global Value R EUR",None,"Official comparator not established in the approved class evidence summarized here","Unknown",None,"No separate official objective comparator established","Unknown","Exact class boundary unknown; no predecessor history implied"),
            ("fund","LU1694789535","DNCA Invest Alpha Bonds B EUR",None,"Official comparator not established here","Unknown",Some("€STR + 1.20%"),"PRD source register: DNCA exact B-class report downloaded September 2026; spread accrual details unverified","Unknown","Pre-€STR construction unknown"),
            ("fund","LU1623762843","Carmignac Portfolio Credit A EUR Acc",None,"Official comparator identity/variant not established here","Unknown",None,"No objective comparator recorded in this ticket evidence","Unknown","Exact class and composite history boundaries unknown"),
            ("fund","IE00BFZMJT78","Neuberger Short Duration Euro Bond EUR I Acc",None,"Official comparator exact-class convention not established here","Unknown",None,"No objective comparator recorded in this ticket evidence","Unknown","Exact class history boundary unknown"),
            ("fund","IE00BD2B9710","Twelve Cat Bond P EUR Acc",None,"Official comparator class/variant not verified here","Unknown",None,"No objective comparator recorded in this ticket evidence","Unknown","Class inception and observation-frequency boundary unknown"),
            ("fund","ES0140072028","Ábaco Renta Fija Mixta Global C",None,"Official comparator variants not reconciled here","Unknown",None,"No separate manager objective recorded here","Unknown","Exact class and component history boundaries unknown"),
        ];
        for (vehicle, identifier, name, official, official_source, official_date, objective, objective_source, objective_date, history) in candidates {
            let official_sql = official.map(|value| format!("'{value}'")).unwrap_or_else(|| "NULL".to_owned());
            let objective_sql = objective.map(|value| format!("'{value}'")).unwrap_or_else(|| "NULL".to_owned());
            manager.get_connection().execute_unprepared(&format!("INSERT INTO benchmark_candidates (vehicle,identifier,display_name,official_comparator,official_provenance,official_document_date,objective_comparator,objective_provenance,objective_document_date,history_boundary) VALUES ('{vehicle}','{identifier}','{name}',{official_sql},'{official_source}','{official_date}',{objective_sql},'{objective_source}','{objective_date}','{history}')")).await?;
        }
        Ok(())
    }
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.drop_table(Table::drop().table(BenchmarkCandidates::Table).to_owned()).await
    }
}

#[derive(DeriveIden)] enum BenchmarkCandidates { Table, Vehicle, Identifier, DisplayName, OfficialComparator, OfficialProvenance, OfficialDocumentDate, ObjectiveComparator, ObjectiveProvenance, ObjectiveDocumentDate, HistoryBoundary }
