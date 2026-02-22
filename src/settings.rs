pub enum Settings {
    ScriptsDir,
    GetFundPriceScript,
}

impl Settings {
    pub fn as_str(&self) -> &'static str {
        match self {
            Settings::ScriptsDir => "scripts",
            Settings::GetFundPriceScript => "get_fund_price.py",
        }
    }
}

impl AsRef<str> for Settings {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
