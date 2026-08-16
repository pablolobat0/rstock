mod constants {
    use std::path::PathBuf;

    pub fn app_data_dir() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_owned());
        PathBuf::from(home).join(".rstock")
    }
}
#[path = "../logging.rs"]
mod logging;

fn main() -> anyhow::Result<()> {
    logging::init(0)
}
