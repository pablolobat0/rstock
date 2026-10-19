use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, QueryOrder, Set};

use crate::db::entities::benchmark_strategy_selection;
use crate::models::StrategySelection;

/// The explicitly selected Strategy benchmark, if one is selected.
///
/// At most one Strategy benchmark is selected, so the table holds at most one
/// row. The read never inspects the portfolio's holdings: only an explicit
/// selection is a selection.
pub async fn find_selected(
    db: &impl sea_orm::ConnectionTrait,
) -> anyhow::Result<Option<StrategySelection>> {
    Ok(benchmark_strategy_selection::Entity::find()
        .order_by_asc(benchmark_strategy_selection::Column::SelectedAt)
        .one(db)
        .await?
        .map(|row| StrategySelection {
            reference_id: row.reference_id,
            selected_at: row.selected_at,
            selection_basis: row.selection_basis,
        }))
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
        .map(|row| StrategySelection {
            reference_id: row.reference_id,
            selected_at: row.selected_at,
            selection_basis: row.selection_basis,
        }))
}

/// Persists one explicit Strategy selection.
pub async fn insert_selection(
    db: &impl sea_orm::ConnectionTrait,
    selection: &StrategySelection,
) -> anyhow::Result<()> {
    benchmark_strategy_selection::ActiveModel {
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
