use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "daily_exchange_rates")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub from_currency: String,
    pub to_currency: String,
    pub date: String,
    pub rate: f64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
