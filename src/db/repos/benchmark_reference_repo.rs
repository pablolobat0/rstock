use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::{benchmark_reference, benchmark_reference_price};

#[derive(Clone, Debug, serde::Serialize)]
pub struct BenchmarkReference {
    pub id: String,
    pub display_name: String,
    pub instrument_ticker: String,
    pub source: String,
    pub source_identifier: String,
    pub currency: String,
    pub adjustment_provenance: String,
    pub adjustment_qualification: String,
    pub return_basis_qualification: String,
    pub role: String,
    pub universe: String,
    pub hedge_policy: String,
    pub source_provenance: String,
    pub history_boundary: String,
    pub verification_state: String,
    pub official_comparator: Option<String>,
    pub objective_comparator: Option<String>,
    /// `reference` for source-backed definitions, `custom_blend` for saved
    /// Custom blends, which are synthetic reference wealth.
    pub kind: String,
}

impl From<benchmark_reference::Model> for BenchmarkReference {
    fn from(model: benchmark_reference::Model) -> Self {
        Self {
            id: model.id,
            display_name: model.display_name,
            instrument_ticker: model.instrument_ticker,
            source: model.source,
            source_identifier: model.source_identifier,
            currency: model.currency,
            adjustment_provenance: model.adjustment_provenance,
            adjustment_qualification: model.adjustment_qualification,
            return_basis_qualification: model.return_basis_qualification,
            role: model.role,
            universe: model.universe,
            hedge_policy: model.hedge_policy,
            source_provenance: model.source_provenance,
            history_boundary: model.history_boundary,
            verification_state: model.verification_state,
            official_comparator: model.official_comparator,
            objective_comparator: model.objective_comparator,
            kind: model.kind,
        }
    }
}

pub async fn find_all(
    db: &impl sea_orm::ConnectionTrait,
) -> anyhow::Result<Vec<BenchmarkReference>> {
    Ok(benchmark_reference::Entity::find()
        .order_by_asc(benchmark_reference::Column::Id)
        .all(db)
        .await?
        .into_iter()
        .map(BenchmarkReference::from)
        .collect())
}

#[derive(Clone, Debug)]
pub struct ReferencePrice {
    pub date: String,
    pub price: f64,
    pub is_api_failure: bool,
    pub is_source_observation: bool,
}

pub async fn find_by_id(
    db: &impl sea_orm::ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<BenchmarkReference> {
    benchmark_reference::Entity::find_by_id(reference_id)
        .one(db)
        .await?
        .map(BenchmarkReference::from)
        .ok_or_else(|| anyhow::anyhow!("benchmark reference '{reference_id}' not found"))
}

/// Whether one benchmark definition identity exists.
pub async fn exists(
    db: &impl sea_orm::ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<bool> {
    Ok(benchmark_reference::Entity::find_by_id(reference_id)
        .one(db)
        .await?
        .is_some())
}

/// Inserts one new benchmark definition.
///
/// Definitions are immutable: an existing identity is never updated or
/// revised, and a repeated identity is rejected by the primary key.
pub async fn insert_definition(
    db: &impl sea_orm::ConnectionTrait,
    definition: &BenchmarkReference,
) -> anyhow::Result<()> {
    benchmark_reference::ActiveModel {
        id: Set(definition.id.clone()),
        display_name: Set(definition.display_name.clone()),
        instrument_ticker: Set(definition.instrument_ticker.clone()),
        source: Set(definition.source.clone()),
        source_identifier: Set(definition.source_identifier.clone()),
        currency: Set(definition.currency.clone()),
        adjustment_provenance: Set(definition.adjustment_provenance.clone()),
        adjustment_qualification: Set(definition.adjustment_qualification.clone()),
        return_basis_qualification: Set(definition.return_basis_qualification.clone()),
        role: Set(definition.role.clone()),
        universe: Set(definition.universe.clone()),
        hedge_policy: Set(definition.hedge_policy.clone()),
        source_provenance: Set(definition.source_provenance.clone()),
        history_boundary: Set(definition.history_boundary.clone()),
        verification_state: Set(definition.verification_state.clone()),
        official_comparator: Set(definition.official_comparator.clone()),
        objective_comparator: Set(definition.objective_comparator.clone()),
        kind: Set(definition.kind.clone()),
    }
    .insert(db)
    .await?;
    Ok(())
}

pub async fn find_prices_with_seed(
    db: &impl sea_orm::ConnectionTrait,
    reference_id: &str,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<ReferencePrice>> {
    let mut records = benchmark_reference_price::Entity::find()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(reference_id))
        .filter(benchmark_reference_price::Column::Date.gte(start_date))
        .filter(benchmark_reference_price::Column::Date.lte(end_date))
        .order_by_asc(benchmark_reference_price::Column::Date)
        .all(db)
        .await?;
    let predecessor = benchmark_reference_price::Entity::find()
        .filter(benchmark_reference_price::Column::ReferenceId.eq(reference_id))
        .filter(benchmark_reference_price::Column::Date.lt(start_date))
        .filter(benchmark_reference_price::Column::IsApiFailure.eq(false))
        .order_by_desc(benchmark_reference_price::Column::Date)
        .one(db)
        .await?;
    if let Some(predecessor) = predecessor {
        records.insert(0, predecessor);
    }
    Ok(records
        .into_iter()
        .map(|record| ReferencePrice {
            date: record.date,
            price: record.closing_price,
            is_api_failure: record.is_api_failure,
            is_source_observation: record.is_source_observation,
        })
        .collect())
}

pub async fn upsert_prices_immutable(
    db: &impl sea_orm::ConnectionTrait,
    reference_id: &str,
    prices: &[ReferencePrice],
) -> anyhow::Result<()> {
    for chunk in prices.chunks(100) {
        benchmark_reference_price::Entity::insert_many(chunk.iter().map(|price| {
            benchmark_reference_price::ActiveModel {
                reference_id: Set(reference_id.to_owned()),
                date: Set(price.date.clone()),
                closing_price: Set(price.price),
                is_api_failure: Set(price.is_api_failure),
                is_source_observation: Set(price.is_source_observation),
                ..Default::default()
            }
        }))
        .on_conflict(
            OnConflict::columns([
                benchmark_reference_price::Column::ReferenceId,
                benchmark_reference_price::Column::Date,
            ])
            .update_columns([
                benchmark_reference_price::Column::ClosingPrice,
                benchmark_reference_price::Column::IsApiFailure,
                benchmark_reference_price::Column::IsSourceObservation,
            ])
            .action_and_where(
                sea_orm::sea_query::Expr::col(benchmark_reference_price::Column::IsApiFailure)
                    .eq(true),
            )
            .to_owned(),
        )
        .exec_without_returning(db)
        .await?;
    }
    Ok(())
}
