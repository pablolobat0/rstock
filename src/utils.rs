use anyhow::{bail, Context};
use std::path::PathBuf;

/// Locate the `scripts/` directory used for Python helper scripts.
///
/// Resolution order:
/// 1. `RSTOCK_SCRIPTS_DIR` environment variable (if set and valid)
/// 2. Walk up from the current executable looking for a `scripts/` folder
pub fn resolve_scripts_dir() -> anyhow::Result<PathBuf> {
    if let Ok(dir) = std::env::var("RSTOCK_SCRIPTS_DIR") {
        let path = PathBuf::from(dir);
        if path.is_dir() {
            return Ok(path);
        }
        bail!(
            "RSTOCK_SCRIPTS_DIR is set but not a valid directory: {}",
            path.display()
        );
    }

    // Walk up from the executable looking for a scripts/ folder
    let exe = std::env::current_exe().context("cannot determine executable path")?;
    let mut dir = exe.parent();
    while let Some(d) = dir {
        let candidate = d.join("scripts");
        if candidate.is_dir() {
            return Ok(candidate);
        }
        dir = d.parent();
    }

    bail!("could not find scripts/ directory (set RSTOCK_SCRIPTS_DIR to override)")
}
