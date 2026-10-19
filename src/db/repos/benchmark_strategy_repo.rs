use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};

use crate::db::entities::benchmark_strategy_selection;
use crate::models::StrategySelection;

/// The single selection slot of the Strategy benchmark.
///
/// The table holds one row at most, so the database itself enforces that one
/// Strategy benchmark is selected.
pub const SELECTION_SLOT: &str = "portfolio";

/// The explicitly selected Strategy benchmark, if one is selected.
///
/// Only an explicit selection is a selection: the read never inspects the
/// portfolio's holdings.
pub async fn find_selected(
    db: &impl sea_orm::ConnectionTrait,
) -> anyhow::Result<Option<StrategySelection>> {
    Ok(
        benchmark_strategy_selection::Entity::find_by_id(SELECTION_SLOT)
            .one(db)
            .await?
            .map(StrategySelection::from),
    )
}

/// The selection that points at one benchmark definition, if any.
pub async fn find_by_reference(
    db: &impl sea_orm::ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<Option<StrategySelection>> {
    Ok(benchmark_strategy_selection::Entity::find()
        .filter(benchmark_strategy_selection::Column::ReferenceId.eq(reference_id))
        .one(db)
        .await?
        .map(StrategySelection::from))
}

/// Persists one explicit Strategy selection.
pub async fn insert_selection(
    db: &impl sea_orm::ConnectionTrait,
    selection: &StrategySelection,
) -> anyhow::Result<()> {
    benchmark_strategy_selection::ActiveModel {
        selection_slot: Set(SELECTION_SLOT.to_owned()),
        reference_id: Set(selection.reference_id.clone()),
        selected_at: Set(selection.selected_at.clone()),
        selection_basis: Set(selection.selection_basis.clone()),
    }
    .insert(db)
    .await?;
    Ok(())
}

/// Removes the explicit Strategy selection, if one exists.
pub async fn delete_selection(db: &impl sea_orm::ConnectionTrait) -> anyhow::Result<bool> {
    let result = benchmark_strategy_selection::Entity::delete_many()
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
}
