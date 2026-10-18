use sea_orm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "benchmark_references")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    pub display_name: String,
    pub instrument_ticker: String,
    pub source: String,
    pub source_identifier: String,
    pub currency: String,
    pub adjustment_provenance: String,
    pub return_basis_qualification: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::benchmark_reference_price::Entity")]
    Price,
}

impl Related<super::benchmark_reference_price::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Price.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
