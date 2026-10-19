use std::collections::BTreeMap;

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseBackend, EntityTrait, QueryFilter,
    QueryOrder, Set, Statement,
};

use crate::db::entities::{benchmark_blend, benchmark_blend_component, benchmark_reference};
use crate::models::{BlendComponent, CustomBlendDefinition};

/// Whether one saved Custom blend exists.
pub async fn exists(db: &impl ConnectionTrait, reference_id: &str) -> anyhow::Result<bool> {
    Ok(benchmark_blend::Entity::find_by_id(reference_id)
        .one(db)
        .await?
        .is_some())
}

/// Every saved Custom blend definition, in a stable order.
pub async fn find_all(db: &impl ConnectionTrait) -> anyhow::Result<Vec<CustomBlendDefinition>> {
    let blends = benchmark_blend::Entity::find()
        .order_by_asc(benchmark_blend::Column::ReferenceId)
        .all(db)
        .await?;
    let components = benchmark_blend_component::Entity::find()
        .order_by_asc(benchmark_blend_component::Column::BlendId)
        .order_by_asc(benchmark_blend_component::Column::Position)
        .all(db)
        .await?;
    let mut components_by_blend: BTreeMap<String, Vec<BlendComponent>> = BTreeMap::new();
    for component in components {
        components_by_blend
            .entry(component.blend_id.clone())
            .or_default()
            .push(component_from_row(component));
    }
    let mut definitions = Vec::with_capacity(blends.len());
    for blend in blends {
        let Some(catalog) = benchmark_reference::Entity::find_by_id(&blend.reference_id)
            .one(db)
            .await?
        else {
            continue;
        };
        definitions.push(CustomBlendDefinition {
            id: blend.reference_id.clone(),
            display_name: catalog.display_name.clone(),
            created_at: blend.created_at.clone(),
            currency: catalog.currency.clone(),
            reset_rule: blend.reset_rule,
            weight_convention: blend.weight_convention,
            cash_convention: blend.cash_convention,
            weight_tolerance: blend.weight_tolerance,
            requested_start_date: blend.requested_start_date,
            requested_end_date: blend.requested_end_date,
            admitted_start_date: blend.admitted_start_date,
            admitted_end_date: blend.admitted_end_date,
            admission_basis: blend.admission_basis,
            components: components_by_blend
                .remove(&blend.reference_id)
                .unwrap_or_default(),
        });
    }
    Ok(definitions)
}

/// One saved Custom blend definition with its components.
pub async fn find_by_id(
    db: &impl ConnectionTrait,
    reference_id: &str,
) -> anyhow::Result<CustomBlendDefinition> {
    let blend = benchmark_blend::Entity::find_by_id(reference_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Custom blend '{reference_id}' not found"))?;
    let catalog = benchmark_reference::Entity::find_by_id(reference_id)
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Custom blend '{reference_id}' not found"))?;
    let components = benchmark_blend_component::Entity::find()
        .filter(benchmark_blend_component::Column::BlendId.eq(reference_id))
        .order_by_asc(benchmark_blend_component::Column::Position)
        .all(db)
        .await?
        .into_iter()
        .map(component_from_row)
        .collect();
    Ok(CustomBlendDefinition {
        id: blend.reference_id,
        display_name: catalog.display_name,
        created_at: blend.created_at,
        currency: catalog.currency,
        reset_rule: blend.reset_rule,
        weight_convention: blend.weight_convention,
        cash_convention: blend.cash_convention,
        weight_tolerance: blend.weight_tolerance,
        requested_start_date: blend.requested_start_date,
        requested_end_date: blend.requested_end_date,
        admitted_start_date: blend.admitted_start_date,
        admitted_end_date: blend.admitted_end_date,
        admission_basis: blend.admission_basis,
        components,
    })
}

/// The earliest date with a positive invested closing snapshot of a
/// non-Monetary holding.
///
/// The query inspects positive invested exposure rather than mere row
/// presence: a zero or exited holding can still carry a historical row. It
/// reads the already-persisted Complete NAV history and never replays the
/// Transaction ledger or rebuilds NAV.
pub async fn find_first_invested_closing_date(
    db: &impl ConnectionTrait,
) -> anyhow::Result<Option<String>> {
    let row = db
        .query_one(Statement::from_string(
            DatabaseBackend::Sqlite,
            "SELECT MIN(pah.date) AS date \
             FROM portfolio_asset_history pah \
             JOIN assets a ON a.id = pah.asset_id \
             WHERE pah.market_value > 0 \
               AND (a.asset_class IS NULL OR LOWER(a.asset_class) <> 'monetary')",
        ))
        .await?;
    Ok(row.and_then(|row| row.try_get::<Option<String>>("", "date").ok().flatten()))
}

/// Persists the blend-specific definition facts of one saved blend.
pub async fn insert_blend_attributes(
    db: &impl ConnectionTrait,
    definition: &CustomBlendDefinition,
) -> anyhow::Result<()> {
    benchmark_blend::ActiveModel {
        reference_id: Set(definition.id.clone()),
        created_at: Set(definition.created_at.clone()),
        reset_rule: Set(definition.reset_rule.clone()),
        weight_convention: Set(definition.weight_convention.clone()),
        cash_convention: Set(definition.cash_convention.clone()),
        weight_tolerance: Set(definition.weight_tolerance),
        requested_start_date: Set(definition.requested_start_date.clone()),
        requested_end_date: Set(definition.requested_end_date.clone()),
        admitted_start_date: Set(definition.admitted_start_date.clone()),
        admitted_end_date: Set(definition.admitted_end_date.clone()),
        admission_basis: Set(definition.admission_basis.clone()),
    }
    .insert(db)
    .await?;
    Ok(())
}

/// Persists the component rows of one saved blend. Components are only ever
/// inserted: a saved blend is never revised.
pub async fn insert_components(
    db: &impl ConnectionTrait,
    blend_id: &str,
    components: &[BlendComponent],
) -> anyhow::Result<()> {
    for (position, component) in components.iter().enumerate() {
        let position = i32::try_from(position)
            .context("a Custom blend cannot hold more components than fit its position")?;
        benchmark_blend_component::ActiveModel {
            blend_id: Set(blend_id.to_owned()),
            component_key: Set(component.component_key.clone()),
            component_reference_id: Set(component.component_reference_id.clone()),
            component_display_name: Set(component.display_name.clone()),
            position: Set(position),
            target_weight: Set(component.target_weight),
            is_cash: Set(component.is_cash),
            component_currency: Set(component.currency.clone()),
            component_return_basis: Set(component.return_basis.clone()),
            component_adjustment: Set(component.adjustment.clone()),
            component_hedge_policy: Set(component.hedge_policy.clone()),
            component_universe: Set(component.universe.clone()),
            component_source: Set(component.source.clone()),
            component_source_identifier: Set(component.source_identifier.clone()),
            component_cadence: Set(component.cadence.clone()),
            component_source_provenance: Set(component.source_provenance.clone()),
            component_verification_state: Set(component.verification_state.clone()),
        }
        .insert(db)
        .await?;
    }
    Ok(())
}

/// Removes one saved blend definition and its component rows.
///
/// Only the blend's own definition, component, and cached-price-free rows are
/// removed. Component definitions and their price, FX, ledger, and NAV
/// history are never touched. The caller's transaction owns the whole
/// deletion, so a failed deletion leaves the saved definition intact.
pub async fn delete_blend(db: &impl ConnectionTrait, blend_id: &str) -> anyhow::Result<bool> {
    let components = benchmark_blend_component::Entity::delete_many()
        .filter(benchmark_blend_component::Column::BlendId.eq(blend_id))
        .exec(db)
        .await?;
    let blend = benchmark_blend::Entity::delete_by_id(blend_id)
        .exec(db)
        .await?;
    let definition = benchmark_reference::Entity::delete_by_id(blend_id)
        .exec(db)
        .await?;
    Ok(definition.rows_affected > 0 || blend.rows_affected > 0 || components.rows_affected > 0)
}

/// The component snapshot of one persisted component row.
fn component_from_row(component: benchmark_blend_component::Model) -> BlendComponent {
    BlendComponent {
        component_key: component.component_key,
        component_reference_id: component.component_reference_id,
        display_name: component.component_display_name,
        target_weight: component.target_weight,
        target_weight_percent: component.target_weight * 100.0,
        is_cash: component.is_cash,
        currency: component.component_currency,
        return_basis: component.component_return_basis,
        adjustment: component.component_adjustment,
        hedge_policy: component.component_hedge_policy,
        universe: component.component_universe,
        source: component.component_source,
        source_identifier: component.component_source_identifier,
        cadence: component.component_cadence,
        source_provenance: component.component_source_provenance,
        verification_state: component.component_verification_state,
    }
}
