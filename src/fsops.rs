use std::fs;
use std::io::ErrorKind;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

/// Moves without overwriting. Cross-filesystem moves are delegated to `mv`.
pub fn move_dir(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        bail!("mv src not found: {}", src.display());
    }
    if dst.exists() {
        bail!("destination exists: {}", dst.display());
    }
    match fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            let status = Command::new("mv")
                .arg("--")
                .arg(src)
                .arg(dst)
                .status()
                .context("mv")?;
            if !status.success() {
                bail!("failed: mv {} {}", src.display(), dst.display());
            }
            Ok(())
        }
        Err(e) => Err(e).with_context(|| format!("failed to move {}", src.display())),
    }
}
