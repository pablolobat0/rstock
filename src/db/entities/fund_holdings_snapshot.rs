use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "fund_holdings_snapshots")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub ms_code: String,
    pub snapshot_date: String,
    pub fingerprint: String,
    pub holdings_json: String,
    pub total_holdings: Option<i32>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
