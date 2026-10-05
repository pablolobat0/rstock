mod logging;

// Keep the executable's logging module on the shared constants; the startup
// logging benchmark supplies its own constants module when including logging.
use clap::Parser;
use rstock::cli::output::OutputFormat;
use rstock::cli::{self, Cli};
use rstock::services::market_data::{DefaultMarketDataSources, MarketData};
use rstock::{constants, db};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    logging::init(cli.verbose)?;

    tracing::debug!(command = ?cli.command, "starting rstock");

    let output_format = OutputFormat::from_json(cli.json);

    let db = db::connect().await?;
    let market_data = MarketData::new(Box::new(DefaultMarketDataSources::new()?));

    cli::run_command(&db, &market_data, cli.command, output_format).await
}
