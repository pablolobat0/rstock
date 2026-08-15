use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait,
    QueryFilter, QueryOrder, Set,
};

use crate::db::entities::asset;
use crate::models::{enum_to_db, Asset, AssetClassification, AssetInfo};

pub async fn find_by_ticker(
    db: &impl ConnectionTrait,
    ticker: &str,
) -> anyhow::Result<Option<Asset>> {
    let result = asset::Entity::find()
        .filter(asset::Column::Ticker.eq(ticker))
        .one(db)
        .await?;
    Ok(result.map(Asset::from))
}

pub async fn find_by_morningstar_code(
    db: &impl ConnectionTrait,
    code: &str,
) -> anyhow::Result<Option<Asset>> {
    let result = asset::Entity::find()
        .filter(asset::Column::MorningstarCode.eq(code))
        .one(db)
        .await?;
    Ok(result.map(Asset::from))
}

pub async fn find_by_ids(
    db: &impl ConnectionTrait,
    ids: impl IntoIterator<Item = i32>,
) -> anyhow::Result<Vec<Asset>> {
    let ids: Vec<i32> = ids.into_iter().collect();
    let results = asset::Entity::find()
        .filter(asset::Column::Id.is_in(ids))
        .all(db)
        .await?;
    Ok(results.into_iter().map(Asset::from).collect())
}

pub async fn find_all(db: &impl ConnectionTrait) -> anyhow::Result<Vec<Asset>> {
    let results = asset::Entity::find()
        .order_by_asc(asset::Column::Ticker)
        .all(db)
        .await?;
    Ok(results.into_iter().map(Asset::from).collect())
}

pub async fn create(
    db: &impl ConnectionTrait,
    info: &AssetInfo,
    classification: &AssetClassification,
    morningstar_code: Option<&str>,
) -> anyhow::Result<i32> {
    if find_by_ticker(db, &info.ticker).await?.is_some() {
        anyhow::bail!(
            "asset with ticker '{}' already exists; use `portfolio asset edit` to update it",
            info.ticker
        );
    }

    let new_asset = active_model(info, classification, morningstar_code);
    let result = new_asset.insert(db).await?;
    Ok(result.id)
}

pub async fn create_on_conflict_do_nothing(
    db: &impl ConnectionTrait,
    info: &AssetInfo,
    classification: &AssetClassification,
    morningstar_code: Option<&str>,
) -> anyhow::Result<i32> {
    asset::Entity::insert(active_model(info, classification, morningstar_code))
        .on_conflict(
            OnConflict::column(asset::Column::Ticker)
                .do_nothing()
                .to_owned(),
        )
        .exec_without_returning(db)
        .await?;

    find_by_ticker(db, &info.ticker)
        .await?
        .map(|asset| asset.id)
        .ok_or_else(|| anyhow::anyhow!("asset '{}' was not persisted", info.ticker))
}

pub async fn update(
    db: &impl ConnectionTrait,
    ticker: &str,
    classification: &AssetClassification,
    name: Option<&str>,
    morningstar_code: Option<&str>,
) -> anyhow::Result<()> {
    let existing = asset::Entity::find()
        .filter(asset::Column::Ticker.eq(ticker))
        .one(db)
        .await?
        .ok_or_else(|| anyhow::anyhow!("asset with ticker '{ticker}' not found"))?;

    let mut active: asset::ActiveModel = existing.into();
    if let Some(n) = name {
        active.name = Set(n.to_owned());
    }
    if let Some(code) = morningstar_code {
        active.morningstar_code = Set(Some(code.to_owned()));
    }
    if let Some(v) = classification.asset_class.as_ref() {
        active.asset_class = Set(Some(enum_to_db(v)));
    }
    if let Some(v) = classification.equity_style.as_ref() {
        active.equity_style = Set(Some(enum_to_db(v)));
    }
    if let Some(v) = classification.bond_credit.as_ref() {
        active.bond_credit = Set(Some(enum_to_db(v)));
    }
    if let Some(v) = classification.bond_duration.as_ref() {
        active.bond_duration = Set(Some(enum_to_db(v)));
    }
    if let Some(v) = classification.management.as_ref() {
        active.management = Set(Some(enum_to_db(v)));
    }
    active.update(db).await?;
    Ok(())
}

fn active_model(
    info: &AssetInfo,
    classification: &AssetClassification,
    morningstar_code: Option<&str>,
) -> asset::ActiveModel {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    asset::ActiveModel {
        ticker: Set(info.ticker.clone()),
        name: Set(info.name.clone()),
        asset_type: Set(info.asset_type.to_string()),
        currency: Set(info.currency.clone()),
        created_at: Set(now),
        morningstar_code: Set(morningstar_code.map(str::to_owned)),
        asset_class: Set(classification.asset_class.as_ref().map(enum_to_db)),
        equity_style: Set(classification.equity_style.as_ref().map(enum_to_db)),
        bond_credit: Set(classification.bond_credit.as_ref().map(enum_to_db)),
        bond_duration: Set(classification.bond_duration.as_ref().map(enum_to_db)),
        management: Set(classification.management.as_ref().map(enum_to_db)),
        ..Default::default()
    }
}
