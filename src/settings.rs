use crate::constants::app_data_dir;
use anyhow::Context;
use std::path::PathBuf;

#[derive(Clone)]
pub struct Settings {
    pub token_page_url: String,
    pub chartservice_url: String,
    pub holdings_url: String,
    pub quote_url: String,
    pub sal_api_key: String,
    pub user_agent: String,
    pub token_cache_path: PathBuf,
}

impl Settings {
    pub fn from_env() -> anyhow::Result<Self> {
        load_dotenv_files();

        Ok(Self {
            token_page_url: required_env("RSTOCK_SOURCE_TOKEN_PAGE_URL")?,
            chartservice_url: required_env("RSTOCK_SOURCE_CHARTSERVICE_URL")?,
            holdings_url: required_env("RSTOCK_SOURCE_HOLDINGS_URL")?,
            quote_url: required_env("RSTOCK_SOURCE_QUOTE_URL")?,
            sal_api_key: required_env("RSTOCK_SOURCE_SAL_API_KEY")?,
            user_agent: required_env("RSTOCK_SOURCE_USER_AGENT")?,
            token_cache_path: PathBuf::from(required_env("RSTOCK_SOURCE_TOKEN_CACHE_PATH")?),
        })
    }
}

fn load_dotenv_files() {
    dotenvy::dotenv().ok();
    dotenvy::from_path(app_data_dir().join(".env")).ok();
}

fn required_env(name: &str) -> anyhow::Result<String> {
    std::env::var(name).with_context(|| format!("missing required environment variable {name}"))
}
