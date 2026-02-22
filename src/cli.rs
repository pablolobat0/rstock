use clap::{Parser, Subcommand, ValueEnum};

#[derive(Parser)]
#[command(name = "rstock", about = "Personal investment portfolio manager")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Record a buy transaction, creates asset if it doesn't exist
    /// Show current portfolio overview
    Get,

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

#[derive(ValueEnum, Clone, Debug)]
pub enum AssetType {
    Stock,
    Fund,
    Etf,
}

impl std::fmt::Display for AssetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AssetType::Stock => write!(f, "stock"),
            AssetType::Fund => write!(f, "fund"),
            AssetType::Etf => write!(f, "etf"),
        }
    }
}
