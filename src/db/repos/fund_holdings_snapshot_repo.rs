use sea_orm::{ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set};

use crate::db::entities::fund_holdings_snapshot;

pub async fn find_latest(
    db: &DatabaseConnection,
    ms_code: &str,
) -> anyhow::Result<Option<fund_holdings_snapshot::Model>> {
    let result = fund_holdings_snapshot::Entity::find()
        .filter(fund_holdings_snapshot::Column::MsCode.eq(ms_code))
        .order_by_desc(fund_holdings_snapshot::Column::SnapshotDate)
        .order_by_desc(fund_holdings_snapshot::Column::CreatedAt)
        .one(db)
        .await?;
    Ok(result)
}

pub async fn find_by_snapshot_date(
    db: &DatabaseConnection,
    ms_code: &str,
    snapshot_date: &str,
) -> anyhow::Result<Option<fund_holdings_snapshot::Model>> {
    let result = fund_holdings_snapshot::Entity::find()
        .filter(fund_holdings_snapshot::Column::MsCode.eq(ms_code))
        .filter(fund_holdings_snapshot::Column::SnapshotDate.eq(snapshot_date))
        .order_by_desc(fund_holdings_snapshot::Column::CreatedAt)
        .one(db)
        .await?;
    Ok(result)
}

pub async fn insert(
    db: &DatabaseConnection,
    ms_code: &str,
    snapshot_date: &str,
    fingerprint: &str,
    holdings_json: &str,
    total_holdings: Option<i32>,
) -> anyhow::Result<()> {
    let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
    let record = fund_holdings_snapshot::ActiveModel {
        ms_code: Set(ms_code.to_owned()),
        snapshot_date: Set(snapshot_date.to_owned()),
        fingerprint: Set(fingerprint.to_owned()),
        holdings_json: Set(holdings_json.to_owned()),
        total_holdings: Set(total_holdings),
        created_at: Set(now),
        ..Default::default()
    };
    fund_holdings_snapshot::Entity::insert(record)
        .exec(db)
        .await?;
    Ok(())
}
