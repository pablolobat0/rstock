use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "portfolio_history")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub date: String,
    pub asset_value: f64,
    pub total_value: f64,
    pub outstanding_shares: f64,
    pub nav: f64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
