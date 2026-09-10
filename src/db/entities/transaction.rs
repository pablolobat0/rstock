use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "transactions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub asset_id: i32,
    pub tx_type: String,
    pub date: String,
    pub units: Option<f64>,
    pub unit_price_cents: Option<i64>,
    pub dividend_amount_cents: Option<i64>,
    pub dividend_deductions_cents: Option<i64>,
    pub split_ratio: Option<f64>,
    pub fees_cents: Option<i64>,
    pub notes: Option<String>,
    pub created_at: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::asset::Entity",
        from = "Column::AssetId",
        to = "super::asset::Column::Id"
    )]
    Asset,
}

impl Related<super::asset::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Asset.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
