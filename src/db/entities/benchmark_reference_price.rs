use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_reference_prices")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,
    pub reference_id: String,
    pub date: String,
    pub closing_price: f64,
    pub is_api_failure: bool,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(
        belongs_to = "super::benchmark_reference::Entity",
        from = "Column::ReferenceId",
        to = "super::benchmark_reference::Column::Id"
    )]
    Reference,
}

impl Related<super::benchmark_reference::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Reference.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
