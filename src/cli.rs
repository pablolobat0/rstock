use chrono::NaiveDate;
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
        date: NaiveDate,

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

    /// Record a dividend payment for an existing asset
    Dividend {
        /// Ticker symbol (asset must already exist)
        #[arg(long)]
        ticker: String,

        /// Ex-dividend date (YYYY-MM-DD)
        #[arg(long)]
        date: NaiveDate,

        /// Total dividend amount received
        #[arg(long)]
        amount: f64,

        /// Withholding tax or fees
        #[arg(long, default_value = "0")]
        fees: f64,

        /// Optional notes
        #[arg(long)]
        notes: Option<String>,
    },

    /// List all assets in the portfolio
    List {},

    /// Export transactions to a CSV file
    Export {
        /// Output file path (e.g. transactions.csv)
        #[arg(long, short)]
        output: String,
    },

    /// Show portfolio holdings breakdown (stocks directly, funds/ETFs with underlying positions)
    Holdings {},

    /// Record a stock split or reverse split for an existing asset
    Split {
        /// Ticker symbol (asset must already exist)
        #[arg(long)]
        ticker: String,

        /// Split date (YYYY-MM-DD)
        #[arg(long)]
        date: NaiveDate,

        /// Split ratio: new shares per old share (e.g. 2 for 2:1 split, 0.25 for 1:4 reverse split)
        #[arg(long)]
        ratio: f64,

        /// Optional notes
        #[arg(long)]
        notes: Option<String>,
    },

    /// Record a sell transaction for an existing asset
    Sell {
        /// Ticker symbol (asset must already exist)
        #[arg(long)]
        ticker: String,

        /// Sale date (YYYY-MM-DD)
        #[arg(long)]
        date: NaiveDate,

        /// Number of shares/units to sell
        #[arg(long)]
        quantity: f64,

        /// Sale price per unit (e.g. 150.25)
        #[arg(long)]
        price: f64,

        /// Commission/fees
        #[arg(long, default_value = "0")]
        fees: f64,

        /// Optional notes
        #[arg(long)]
        notes: Option<String>,
    },
}
