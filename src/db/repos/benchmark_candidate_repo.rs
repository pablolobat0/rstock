use sea_orm::{EntityTrait, QueryOrder};

use crate::db::entities::benchmark_candidate;

#[derive(Clone, Debug)]
pub struct FundCandidateComparator {
    pub vehicle: String,
    pub identifier: String,
    pub display_name: String,
    pub official_comparator: Option<String>,
    pub official_provenance: String,
    pub official_document_date: String,
    pub objective_comparator: Option<String>,
    pub objective_provenance: String,
    pub objective_document_date: String,
    pub history_boundary: String,
}

fn from_model(model: benchmark_candidate::Model) -> FundCandidateComparator {
    FundCandidateComparator {
        vehicle: model.vehicle,
        identifier: model.identifier,
        display_name: model.display_name,
        official_comparator: model.official_comparator,
        official_provenance: model.official_provenance,
        official_document_date: model.official_document_date,
        objective_comparator: model.objective_comparator,
        objective_provenance: model.objective_provenance,
        objective_document_date: model.objective_document_date,
        history_boundary: model.history_boundary,
    }
}

pub async fn find_all(
    db: &impl sea_orm::ConnectionTrait,
) -> anyhow::Result<Vec<FundCandidateComparator>> {
    Ok(benchmark_candidate::Entity::find()
        .order_by_asc(benchmark_candidate::Column::Identifier)
        .all(db)
        .await?
        .into_iter()
        .map(from_model)
        .collect())
}
