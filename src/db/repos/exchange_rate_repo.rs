use sea_orm::{
    sea_query::OnConflict, ActiveModelTrait, ColumnTrait, ConnectionTrait, EntityTrait,
    FromQueryResult, QueryFilter, QueryOrder, Set, Statement,
};

use crate::db::entities::daily_exchange_rate;

const BULK_WRITE_SIZE: usize = 100;

pub struct ExchangeRateWrite {
    pub from_currency: String,
    pub to_currency: String,
    pub date: String,
    pub rate: f64,
}

#[derive(FromQueryResult)]
struct DatedRate {
    date: String,
    value: f64,
}

pub async fn find_rate(
    db: &impl ConnectionTrait,
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
    db: &impl ConnectionTrait,
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
    db: &impl ConnectionTrait,
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
    db: &impl ConnectionTrait,
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

pub async fn find_coverage_with_seed(
    db: &impl ConnectionTrait,
    from_currency: &str,
    to_currency: &str,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<Vec<(String, f64)>> {
    let rows = DatedRate::find_by_statement(Statement::from_sql_and_values(
        db.get_database_backend(),
        r"
            SELECT date, rate AS value
            FROM daily_exchange_rates
            WHERE from_currency = ? AND to_currency = ? AND date >= ? AND date <= ?
            UNION ALL
            SELECT date, rate AS value
            FROM daily_exchange_rates
            WHERE from_currency = ? AND to_currency = ? AND date = (
                SELECT MAX(date)
                FROM daily_exchange_rates
                WHERE from_currency = ? AND to_currency = ? AND date < ?
            )
            ORDER BY date
        ",
        [
            from_currency.into(),
            to_currency.into(),
            start_date.into(),
            end_date.into(),
            from_currency.into(),
            to_currency.into(),
            from_currency.into(),
            to_currency.into(),
            start_date.into(),
        ],
    ))
    .all(db)
    .await?;
    Ok(rows.into_iter().map(|row| (row.date, row.value)).collect())
}

pub async fn find_latest_date(
    db: &impl ConnectionTrait,
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
    db: &impl ConnectionTrait,
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
    db: &impl ConnectionTrait,
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
    db: &impl ConnectionTrait,
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

pub async fn upsert_native(
    db: &impl ConnectionTrait,
    from_currency: &str,
    to_currency: &str,
    date: &str,
    rate: f64,
) -> anyhow::Result<()> {
    daily_exchange_rate::Entity::insert(active_model(from_currency, to_currency, date, rate))
        .on_conflict(native_conflict())
        .exec_without_returning(db)
        .await?;
    Ok(())
}

pub async fn upsert_many_native(
    db: &impl ConnectionTrait,
    rates: &[ExchangeRateWrite],
) -> anyhow::Result<()> {
    for chunk in rates.chunks(BULK_WRITE_SIZE) {
        daily_exchange_rate::Entity::insert_many(chunk.iter().map(|rate| {
            active_model(
                &rate.from_currency,
                &rate.to_currency,
                &rate.date,
                rate.rate,
            )
        }))
        .on_conflict(native_conflict())
        .exec_without_returning(db)
        .await?;
    }
    Ok(())
}

pub async fn insert_many_immutable(
    db: &impl ConnectionTrait,
    rates: &[ExchangeRateWrite],
) -> anyhow::Result<()> {
    for chunk in rates.chunks(BULK_WRITE_SIZE) {
        daily_exchange_rate::Entity::insert_many(rate_models(chunk))
            .on_conflict(immutable_conflict())
            .exec_without_returning(db)
            .await?;
    }
    Ok(())
}

fn active_model(
    from_currency: &str,
    to_currency: &str,
    date: &str,
    rate: f64,
) -> daily_exchange_rate::ActiveModel {
    daily_exchange_rate::ActiveModel {
        from_currency: Set(from_currency.to_owned()),
        to_currency: Set(to_currency.to_owned()),
        date: Set(date.to_owned()),
        rate: Set(rate),
        ..Default::default()
    }
}

fn rate_models(
    rates: &[ExchangeRateWrite],
) -> impl Iterator<Item = daily_exchange_rate::ActiveModel> + '_ {
    rates.iter().map(|rate| {
        active_model(
            &rate.from_currency,
            &rate.to_currency,
            &rate.date,
            rate.rate,
        )
    })
}

fn native_conflict() -> OnConflict {
    OnConflict::columns([
        daily_exchange_rate::Column::FromCurrency,
        daily_exchange_rate::Column::ToCurrency,
        daily_exchange_rate::Column::Date,
    ])
    .update_column(daily_exchange_rate::Column::Rate)
    .to_owned()
}

fn immutable_conflict() -> OnConflict {
    OnConflict::columns([
        daily_exchange_rate::Column::FromCurrency,
        daily_exchange_rate::Column::ToCurrency,
        daily_exchange_rate::Column::Date,
    ])
    .do_nothing()
    .to_owned()
}
