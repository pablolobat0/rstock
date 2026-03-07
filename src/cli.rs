use clap::{Parser, Subcommand, ValueEnum};

use crate::models::AssetType;

#[derive(Parser)]
#[command(name = "rstock", about = "Personal investment portfolio manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ChartPeriod {
    Ytd,
    #[value(name = "1y")]
    OneYear,
    #[value(name = "3y")]
    ThreeYears,
    #[value(name = "5y")]
    FiveYears,
    All,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show current portfolio overview
    Get {
        /// Time period for the NAV chart
        #[arg(long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },

    /// Record a buy transaction, creates asset if it doesn't exist
    Buy {
        /// Ticker symbol
        #[arg(long)]
        ticker: String,

        /// Full name of the asset
        #[arg(long)]
        name: String,

        /// Asset type
        #[arg(long = "type", value_enum)]
        asset_type: AssetType,

        /// ISIN code
        #[arg(long)]
        isin: Option<String>,

        /// Purchase date (YYYY-MM-DD)
        #[arg(long)]
        date: String,

        /// Number of shares/units
        #[arg(long)]
        quantity: f64,

        /// Price per unit (e.g. 150.25)
        #[arg(long)]
        price: f64,

        /// Commission/fees
        #[arg(long, default_value = "0")]
        fees: f64,

        /// Currency
        #[arg(long, default_value = "EUR")]
        currency: String,

        /// Optional notes
        #[arg(long)]
        notes: Option<String>,
    },
}
