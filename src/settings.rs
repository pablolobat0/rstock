pub enum Settings {
    ScriptsDir,
    GetFundPriceScript,
    GetFundPriceHistoryScript,
}

impl Settings {
    pub fn as_str(&self) -> &'static str {
        match self {
            Settings::ScriptsDir => "scripts",
            Settings::GetFundPriceScript => "get_fund_price.py",
            Settings::GetFundPriceHistoryScript => "get_fund_price_history.py",
        }
    }
}

impl AsRef<str> for Settings {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
