use clap::ValueEnum;
use sea_orm::{ConnectionTrait, DatabaseConnection, TransactionTrait};

use crate::db::repos::{
    asset_repo, daily_price_repo, portfolio_asset_history_repo, portfolio_history_repo,
    transaction_repo,
};
use crate::models::{Asset, AssetClass, AssetClassification, AssetInfo};

pub async fn create_tracked_asset(
    db: &DatabaseConnection,
    info: &AssetInfo,
    classification: &AssetClassification,
    morningstar_code: Option<&str>,
) -> anyhow::Result<i32> {
    classification.validate_for_asset(&info.asset_type, morningstar_code)?;
    asset_repo::create(db, info, classification, morningstar_code).await
}

pub async fn update_tracked_asset(
    db: &DatabaseConnection,
    ticker: &str,
    classification: &AssetClassification,
    name: Option<&str>,
    morningstar_code: Option<&str>,
) -> anyhow::Result<()> {
    let txn = db.begin().await?;
    let existing = asset_repo::find_by_ticker(&txn, ticker)
        .await?
        .ok_or_else(|| anyhow::anyhow!("asset with ticker '{ticker}' not found"))?;
    let updated_classification = merge_classification(&existing, classification)?;
    let updated_morningstar_code = morningstar_code.or(existing.morningstar_code.as_deref());

    updated_classification.validate_for_asset(&existing.asset_type, updated_morningstar_code)?;
    let nav_scope_changed = existing.is_monetary()
        != (updated_classification.asset_class.as_ref() == Some(&AssetClass::Monetary));
    let provider_changed = matches!(
        existing.asset_type,
        crate::models::AssetType::Fund | crate::models::AssetType::Etf
    ) && morningstar_code.is_some()
        && existing.morningstar_code.as_deref() != morningstar_code;
    asset_repo::update(
        &txn,
        ticker,
        &updated_classification,
        name,
        updated_morningstar_code,
    )
    .await?;

    if provider_changed {
        daily_price_repo::delete_all_for_asset(&txn, existing.id).await?;
    }
    if nav_scope_changed || provider_changed {
        invalidate_nav_history(&txn, existing.id).await?;
    }

    txn.commit().await?;
    Ok(())
}

fn merge_classification(
    existing: &Asset,
    updates: &AssetClassification,
) -> anyhow::Result<AssetClassification> {
    Ok(AssetClassification {
        asset_class: updates.asset_class.clone().or(parse_optional_enum(
            existing.asset_class.as_deref(),
            "asset class",
        )?),
        equity_style: updates.equity_style.clone().or(parse_optional_enum(
            existing.equity_style.as_deref(),
            "equity style",
        )?),
        bond_credit: updates.bond_credit.clone().or(parse_optional_enum(
            existing.bond_credit.as_deref(),
            "bond credit",
        )?),
        bond_duration: updates.bond_duration.clone().or(parse_optional_enum(
            existing.bond_duration.as_deref(),
            "bond duration",
        )?),
        management: updates.management.clone().or(parse_optional_enum(
            existing.management.as_deref(),
            "management",
        )?),
    })
}

fn parse_optional_enum<E>(value: Option<&str>, field: &str) -> anyhow::Result<Option<E>>
where
    E: ValueEnum,
{
    value
        .map(|v| {
            E::from_str(v, true)
                .map_err(|_| anyhow::anyhow!("stored {field} value '{v}' is invalid"))
        })
        .transpose()
}

async fn invalidate_nav_history(db: &impl ConnectionTrait, asset_id: i32) -> anyhow::Result<()> {
    let earliest_tx_date = transaction_repo::find_by_asset_id(db, asset_id)
        .await?
        .into_iter()
        .map(|tx| tx.date)
        .min();

    if let Some(date) = earliest_tx_date {
        portfolio_history_repo::delete_from_date(db, &date).await?;
        portfolio_asset_history_repo::delete_from_date(db, &date).await?;
    }

    Ok(())
}
