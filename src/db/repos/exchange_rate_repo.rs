use std::collections::HashMap;

use sea_orm::{
    sea_query::OnConflict, ColumnTrait, ConnectionTrait, EntityTrait, FromQueryResult, QueryFilter,
    QueryOrder, Set, Statement,
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

pub async fn find_rates_between_currencies(
    db: &impl ConnectionTrait,
    from_currencies: &[String],
    to_currency: &str,
    start_date: &str,
    end_date: &str,
) -> anyhow::Result<HashMap<String, Vec<(String, f64)>>> {
    if from_currencies.is_empty() {
        return Ok(HashMap::new());
    }

    let results = daily_exchange_rate::Entity::find()
        .filter(daily_exchange_rate::Column::FromCurrency.is_in(from_currencies.to_vec()))
        .filter(daily_exchange_rate::Column::ToCurrency.eq(to_currency))
        .filter(daily_exchange_rate::Column::Date.gte(start_date))
        .filter(daily_exchange_rate::Column::Date.lte(end_date))
        .order_by_asc(daily_exchange_rate::Column::FromCurrency)
        .order_by_asc(daily_exchange_rate::Column::Date)
        .all(db)
        .await?;

    let mut rates = HashMap::new();
    for rate in results {
        rates
            .entry(rate.from_currency)
            .or_insert_with(Vec::new)
            .push((rate.date, rate.rate));
    }
    Ok(rates)
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

fn immutable_conflict() -> OnConflict {
    OnConflict::columns([
        daily_exchange_rate::Column::FromCurrency,
        daily_exchange_rate::Column::ToCurrency,
        daily_exchange_rate::Column::Date,
    ])
    .do_nothing()
    .to_owned()
}
