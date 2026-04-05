use std::fs;

use tracing_subscriber::fmt::time::uptime;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

use crate::constants::app_data_dir;

pub fn init(verbosity: u8) -> anyhow::Result<()> {
    let filter = build_filter(verbosity);

    let data_dir = app_data_dir();
    fs::create_dir_all(&data_dir)?;

    let file_appender = tracing_appender::rolling::daily(&data_dir, "rstock.log");

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(false)
        .with_timer(uptime())
        .compact();

    let file_layer = fmt::layer()
        .with_writer(file_appender)
        .with_ansi(false)
        .with_target(true)
        .compact();

    tracing_subscriber::registry()
        .with(filter)
        .with(stderr_layer)
        .with(file_layer)
        .init();

    Ok(())
}

fn build_filter(verbosity: u8) -> EnvFilter {
    if verbosity > 0 {
        let level = match verbosity {
            1 => "info",
            2 => "debug",
            _ => "trace",
        };
        EnvFilter::new(level)
    } else if std::env::var("RUST_LOG").is_ok() {
        EnvFilter::from_default_env()
    } else {
        EnvFilter::new("warn")
    }
}
