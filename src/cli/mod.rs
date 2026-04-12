pub mod commands;
pub mod display;

use chrono::NaiveDate;
use clap::{Args, Parser, Subcommand, ValueEnum};

use crate::constants::DISPLAY_DATE_FORMAT;
use crate::models::{AssetClass, AssetType, BondCredit, BondDuration, EquityStyle, Management};

fn parse_date(s: &str) -> Result<NaiveDate, String> {
    let date = NaiveDate::parse_from_str(s, DISPLAY_DATE_FORMAT)
        .map_err(|_| format!("invalid date '{s}', expected DD-MM-YYYY format"))?;
    let today = chrono::Local::now().date_naive();
    if date > today {
        return Err(format!("date cannot be in the future: {s}"));
    }
    Ok(date)
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
    /// Increase logging verbosity (-v info, -vv debug, -vvv trace)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    pub verbose: u8,

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

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show portfolio overview with NAV chart and key metrics (shortcut for `portfolio get`)
    Get {
        /// Time period for the NAV chart
        #[arg(short, long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },

    /// Record a buy transaction for an existing asset (shortcut for `transaction buy`)
    Buy {
        /// Ticker symbol (asset must already exist; create with `portfolio asset add`)
        #[arg(short, long)]
        ticker: String,

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
    },

    /// Portfolio views: overview, asset list, holdings breakdown
    Portfolio(PortfolioArgs),

    /// Record, edit, or delete transactions (buy, sell, dividend, split, edit, delete)
    Transaction(TransactionArgs),

    /// Import or export transactions as CSV
    Data(DataArgs),

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
}

#[derive(Debug, Args)]
pub struct PortfolioArgs {
    #[command(subcommand)]
    pub command: PortfolioCommands,
}

#[derive(Debug, Subcommand)]
pub enum PortfolioCommands {
    /// Show portfolio overview with NAV chart and key metrics
    Get {
        /// Time period for the NAV chart
        #[arg(short, long, value_enum, default_value = "1y")]
        period: ChartPeriod,
    },

    /// List all assets in the portfolio
    List {},

    /// Show portfolio holdings breakdown (stocks directly, funds/ETFs with underlying positions)
    Holdings {},

    /// Manage assets: add or edit asset metadata and classification
    Asset(AssetArgs),
}

#[derive(Debug, Args)]
pub struct AssetArgs {
    #[command(subcommand)]
    pub command: AssetCommands,
}

#[derive(Debug, Subcommand)]
pub enum AssetCommands {
    /// Create an asset with its classification
    Add {
        /// Ticker symbol (stocks) or ISIN (funds/ETFs)
        #[arg(short, long)]
        ticker: String,

        /// Full name of the asset
        #[arg(short, long)]
        name: String,

        /// Vehicle type
        #[arg(short = 'T', long = "type", value_enum)]
        asset_type: AssetType,

        /// Trading currency code
        #[arg(short, long, default_value = "EUR")]
        currency: String,

        /// Asset class (top-level classification)
        #[arg(long = "asset-class", value_enum)]
        asset_class: AssetClass,

        /// Equity style (for equity assets)
        #[arg(long = "equity-style", value_enum)]
        equity_style: Option<EquityStyle>,

        /// Bond credit quality (for fixed-income assets)
        #[arg(long = "bond-credit", value_enum)]
        bond_credit: Option<BondCredit>,

        /// Bond duration bucket (for fixed-income assets)
        #[arg(long = "bond-duration", value_enum)]
        bond_duration: Option<BondDuration>,

        /// Management style (active vs passive)
        #[arg(long, value_enum)]
        management: Option<Management>,

        /// Morningstar code (for funds/ETFs needing the price scraper)
        #[arg(long = "morningstar-code")]
        morningstar_code: Option<String>,
    },

    /// Update an existing asset's metadata or classification
    Edit {
        /// Ticker symbol (asset must already exist)
        #[arg(short, long)]
        ticker: String,

        /// New name
        #[arg(short, long)]
        name: Option<String>,

        /// Asset class
        #[arg(long = "asset-class", value_enum)]
        asset_class: Option<AssetClass>,

        /// Equity style
        #[arg(long = "equity-style", value_enum)]
        equity_style: Option<EquityStyle>,

        /// Bond credit quality
        #[arg(long = "bond-credit", value_enum)]
        bond_credit: Option<BondCredit>,

        /// Bond duration bucket
        #[arg(long = "bond-duration", value_enum)]
        bond_duration: Option<BondDuration>,

        /// Management style
        #[arg(long, value_enum)]
        management: Option<Management>,

        /// Morningstar code
        #[arg(long = "morningstar-code")]
        morningstar_code: Option<String>,
    },
}

#[derive(Debug, Args)]
pub struct DataArgs {
    #[command(subcommand)]
    pub command: DataCommands,
}

#[derive(Debug, Subcommand)]
pub enum DataCommands {
    /// Export all transactions to a CSV file
    Export {
        /// Output file path (e.g. transactions.csv)
        #[arg(long, short)]
        output: String,
    },

    /// Import transactions from a CSV file
    Import {
        /// Input CSV file path (e.g. transactions.csv)
        #[arg(long, short)]
        input: String,
    },
}

#[derive(Debug, Args)]
pub struct MonitorArgs {
    #[command(subcommand)]
    pub command: MonitorCommands,
}

#[derive(Debug, Subcommand)]
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

#[derive(Debug, Args)]
pub struct TransactionArgs {
    #[command(subcommand)]
    pub command: TransactionCommands,
}

#[derive(Debug, Subcommand)]
pub enum TransactionCommands {
    /// Record a buy transaction for an existing asset
    Buy {
        /// Ticker symbol (asset must already exist; create with `portfolio asset add`)
        #[arg(short, long)]
        ticker: String,

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
    },

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
        #[arg(short, long, value_parser = parse_positive_f64)]
        price: f64,

        /// Broker commission and fees
        #[arg(short, long, default_value = "0")]
        fees: f64,
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

    /// Edit an existing transaction (date, quantity, price, fees)
    Edit {
        /// Transaction ID
        id: i32,

        /// New date (DD-MM-YYYY)
        #[arg(short, long, value_parser = parse_date)]
        date: Option<NaiveDate>,

        /// New quantity
        #[arg(short, long, value_parser = parse_positive_f64)]
        quantity: Option<f64>,

        /// New price per unit
        #[arg(short, long, value_parser = parse_positive_f64)]
        price: Option<f64>,

        /// New fees
        #[arg(short, long)]
        fees: Option<f64>,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Delete an existing transaction
    Delete {
        /// Transaction ID
        id: i32,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },
}
