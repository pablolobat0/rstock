use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::constants::DISPLAY_DATE_FORMAT;
use crate::models::AssetType;

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(s, DISPLAY_DATE_FORMAT)
        .map_err(|_| format!("invalid date '{s}', expected DD-MM-YYYY format"))
}

fn parse_positive_f64(s: &str) -> Result<f64, String> {
    let val: f64 = s
        .parse()
        .map_err(|_| format!("'{s}' is not a valid number"))?;

    if val > 0.0 {
        Ok(val)
    } else {
        Err("value must be greater than 0".to_string())
    }
}

#[derive(Parser)]
#[command(
    name = "rstock",
    version,
    about = "Personal investment portfolio manager",
    long_about = "Personal investment portfolio manager.\n\n\
        Track purchases, sales, dividends, and splits. View portfolio \
        performance, analyze correlations, and monitor individual stocks.",
    after_help = "Use 'rstock <command> --help' for more information about a specific command."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum CorrelationPeriod {
    #[value(name = "30d")]
    ThirtyDays,
    #[value(name = "6m")]
    SixMonths,
    #[value(name = "1y")]
    OneYear,
    #[value(name = "3y")]
    ThreeYears,
    #[value(name = "5y")]
    FiveYears,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum AnalysisTarget {
    Portfolio,
}

#[derive(ValueEnum, Clone, Debug)]
pub enum ChartPeriod {
    #[value(name = "1m")]
    OneMonth,
    #[value(name = "3m")]
    ThreeMonths,
    #[value(name = "6m")]
    SixMonths,
    Ytd,
    #[value(name = "1y")]
    OneYear,
    #[value(name = "3y")]
    ThreeYears,
    #[value(name = "5y")]
    FiveYears,
    All,
}

impl ChartPeriod {
    pub fn label(&self) -> &'static str {
        match self {
            Self::OneMonth => "1M",
            Self::ThreeMonths => "3M",
            Self::SixMonths => "6M",
            Self::Ytd => "YTD",
            Self::OneYear => "1Y",
            Self::ThreeYears => "3Y",
            Self::FiveYears => "5Y",
            Self::All => "All",
        }
    }
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show portfolio overview with NAV chart and key metrics
    Get {
        /// Time period for the NAV chart
        #[arg(short, long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },

    /// Record a buy transaction, creates asset if it doesn't exist
    Buy {
        /// Ticker symbol (stocks) or ISIN (funds/ETFs)
        #[arg(short, long)]
        ticker: String,

        /// Full name of the asset
        #[arg(short, long)]
        name: String,

        /// Asset type
        #[arg(short = 'T', long = "type", value_enum)]
        asset_type: AssetType,

        /// Purchase date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Number of shares/units
        #[arg(short, long, value_parser = parse_positive_f64)]
        quantity: f64,

        /// Price per unit (e.g. 150.25)
        #[arg(short, long, value_parser = parse_positive_f64)]
        price: f64,

        /// Broker commission and fees
        #[arg(short, long, default_value = "0")]
        fees: f64,

        /// Transaction currency code
        #[arg(short, long, default_value = "EUR")]
        currency: String,
    },

    /// Record a dividend payment for an existing asset
    Dividend {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// Ex-dividend date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Total dividend amount received
        #[arg(short, long)]
        amount: f64,

        /// Withholding tax or fees
        #[arg(short, long, default_value = "0")]
        fees: f64,
    },

    /// List all assets in the portfolio
    List {},

    /// Export all transactions to a CSV file
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
        #[arg(short, long)]
        ticker: String,

        /// Split date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Split ratio: new shares per old share (e.g. 2 for 2:1 split, 0.25 for 1:4 reverse split)
        #[arg(short, long)]
        ratio: f64,
    },

    /// Analyze portfolio correlations across holdings
    Analyze {
        /// What to analyze
        #[arg(value_enum)]
        target: AnalysisTarget,

        /// Time period for correlation calculation
        #[arg(short, long, value_enum, default_value = "1y")]
        period: CorrelationPeriod,
    },

    /// Monitor a stock: fundamentals, momentum, and sector comparison
    Monitor(MonitorArgs),

    /// Record a sell transaction for an existing asset
    Sell {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// Sale date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: NaiveDate,

        /// Number of shares/units to sell
        #[arg(short, long, value_parser = parse_positive_f64)]
        quantity: f64,

        /// Sale price per unit (e.g. 150.25)
        #[arg(short, long,value_parser = parse_positive_f64)]
        price: f64,

        /// Broker commission and fees
        #[arg(short, long, default_value = "0")]
        fees: f64,
    },
}

#[derive(Args)]
pub struct MonitorArgs {
    #[command(subcommand)]
    pub command: MonitorCommands,
}

#[derive(Subcommand)]
pub enum MonitorCommands {
    /// Add a stock to the watchlist with its sector ETF
    Add {
        /// Ticker symbol
        #[arg(short, long)]
        ticker: String,

        /// Sector ETF ticker to compare against
        #[arg(short, long)]
        sector_etf: String,
    },

    /// Remove a stock from the watchlist
    Remove {
        /// Ticker symbol
        #[arg(short, long)]
        ticker: String,
    },

    /// List all monitored stocks
    List {},

    /// View analysis for a monitored stock
    View {
        /// Ticker symbol (must be in watchlist)
        ticker: String,

        /// Time period for analysis
        #[arg(short, long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },
}
