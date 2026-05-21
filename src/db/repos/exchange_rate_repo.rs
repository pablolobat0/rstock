use sea_orm::{
    ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, QueryOrder, Set,
};

use crate::db::entities::daily_exchange_rate;

pub async fn find_rate(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    let result = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.eq(date))
        .one(db)
        .await?;
    Ok(result.map(|r| r.rate))
}

pub async fn find_rate_at_or_before(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    let result = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.lte(date))
        .order_by_desc(daily_exchange_rate::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.rate))
}

pub async fn find_rate_and_date_at_or_before(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    date: &str,
) -> anyhow::Result<Option<(f64, String)>> {
    let result = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.lte(date))
        .order_by_desc(daily_exchange_rate::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| (r.rate, r.date)))
}

pub async fn find_rates_between(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let results = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.gte(start_date))
        .filter(daily_exchange_rate::Column::Date.lte(end_date))
        .order_by_asc(daily_exchange_rate::Column::Date)
        .all(db)
        .await?;
    Ok(results.into_iter().map(|r| (r.date, r.rate)).collect())
}

pub async fn find_latest_date(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
) -> anyhow::Result<Option<String>> {
    let result = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .order_by_desc(daily_exchange_rate::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.date))
}

pub async fn find_rate_before(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    date: &str,
) -> anyhow::Result<Option<f64>> {
    let result = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.lt(date))
        .order_by_desc(daily_exchange_rate::Column::Date)
        .one(db)
        .await?;
    Ok(result.map(|r| r.rate))
}

pub async fn exists(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    date: &str,
) -> anyhow::Result<bool> {
    let result = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.eq(date))
        .one(db)
        .await?;
    Ok(result.is_some())
}

pub async fn upsert(
    db: &DatabaseConnection,
    from_currency: &str,
    to_currency: &str,
    date: &str,
    rate: f64,
) -> anyhow::Result<()> {
    let existing = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.eq(from_currency))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.eq(date))
        .one(db)
        .await?;

    if let Some(record) = existing {
        let mut active: daily_exchange_rate::ActiveModel = record.into();
        active.rate = Set(rate);
        active.update(db).await?;
    } else {
        let record = daily_exchange_rate::ActiveModel {
            from_currency: Set(from_currency.to_owned()),
            to_currency: Set(to_currency.to_owned()),
            date: Set(date.to_owned()),
            rate: Set(rate),
            ..Default::default()
        };
        record.insert(db).await?;
    }

    Ok(())
}
