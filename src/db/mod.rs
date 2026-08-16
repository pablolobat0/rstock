use std::fs;
use std::path::PathBuf;

use migration::MigratorTrait;
use sea_orm::{Database, DatabaseConnection, DbErr, TransactionTrait};

use crate::constants::app_data_dir;

pub mod entities;
pub mod repos;

fn db_path() -> PathBuf {
    app_data_dir().join("rstock.db")
}

pub async fn connect() -> Result<DatabaseConnection, DbErr> {
    let path = db_path();
    tracing::debug!(path = %path.display(), "connecting to database");

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| DbErr::Custom(e.to_string()))?;
    }

    let url = format!("sqlite:{}?mode=rwc", path.display());
    let db = Database::connect(&url).await?;
    tracing::info!("running database migrations");
    migrate(&db).await?;
    tracing::debug!("database ready");
    Ok(db)
}

pub async fn migrate(db: &DatabaseConnection) -> Result<(), DbErr> {
    let transaction = db.begin().await?;
    migration::Migrator::up(&transaction, None).await?;
    transaction.commit().await
}
