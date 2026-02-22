use migration::MigratorTrait;
use sea_orm::{Database, DatabaseConnection, DbErr};
use std::fs;
use std::path::PathBuf;

pub mod entities;

fn db_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".rstock").join("rstock.db")
}

pub async fn connect() -> Result<DatabaseConnection, DbErr> {
    let path = db_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| DbErr::Custom(e.to_string()))?;
    }

    let url = format!("sqlite:{}?mode=rwc", path.display());
    let db = Database::connect(&url).await?;
    migration::Migrator::up(&db, None).await?;
    Ok(db)
}
