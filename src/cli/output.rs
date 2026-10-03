use std::io::{self, Write};

use anyhow::Context;
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Human,
    Json,
}

impl OutputFormat {
    pub fn from_json(json: bool) -> Self {
        if json {
            Self::Json
        } else {
            Self::Human
        }
    }

    pub fn is_json(self) -> bool {
        self == Self::Json
    }
}

#[derive(Serialize)]
struct Envelope<'a, T: ?Sized> {
    command: &'a str,
    data: &'a T,
}

pub fn emit_json<T: Serialize + ?Sized>(command: &str, data: &T) -> anyhow::Result<()> {
    let stdout = io::stdout();
    let mut writer = stdout.lock();
    write_json(&mut writer, command, data)
}

pub fn write_json<W: Write, T: Serialize + ?Sized>(
    writer: &mut W,
    command: &str,
    data: &T,
) -> anyhow::Result<()> {
    let mut document = serde_json::to_vec(&Envelope { command, data })
        .context("failed to serialize JSON output")?;
    document.push(b'\n');
    writer
        .write_all(&document)
        .context("failed to write JSON output")
}

#[cfg(test)]
#[path = "../../tests/unit/output_tests.rs"]
mod tests;
