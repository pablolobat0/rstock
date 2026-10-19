use std::collections::BTreeMap;

use anyhow::Context;
use sea_orm::{
    ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::{
    benchmark_blend, benchmark_blend_component, benchmark_reference, benchmark_strategy_selection,
};
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
        admitted_start_date: blend.admitted_start_date,
        admitted_end_date: blend.admitted_end_date,
        admission_basis: blend.admission_basis,
        components,
    })
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
            component_hedge_policy: Set(component.hedge_policy.clone()),
            component_universe: Set(component.universe.clone()),
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
/// history are never touched.
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

/// The reference currently selected as the portfolio Strategy, if any.
pub async fn find_strategy_selection(db: &impl ConnectionTrait) -> anyhow::Result<Option<String>> {
    Ok(benchmark_strategy_selection::Entity::find()
        .one(db)
        .await?
        .map(|selection| selection.reference_id))
}

/// Records the reference selected as the portfolio Strategy.
pub async fn insert_strategy_selection(
    db: &impl ConnectionTrait,
    reference_id: &str,
    selected_at: &str,
) -> anyhow::Result<()> {
    benchmark_strategy_selection::ActiveModel {
        id: Set("portfolio-strategy".to_owned()),
        reference_id: Set(reference_id.to_owned()),
        selected_at: Set(selected_at.to_owned()),
    }
    .insert(db)
    .await?;
    Ok(())
}

/// Removes the selected portfolio Strategy, if one is selected.
pub async fn delete_strategy_selection(db: &impl ConnectionTrait) -> anyhow::Result<bool> {
    let result = benchmark_strategy_selection::Entity::delete_many()
        .exec(db)
        .await?;
    Ok(result.rows_affected > 0)
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
        hedge_policy: component.component_hedge_policy,
        universe: component.component_universe,
        source_provenance: component.component_source_provenance,
        verification_state: component.component_verification_state,
    }
}
