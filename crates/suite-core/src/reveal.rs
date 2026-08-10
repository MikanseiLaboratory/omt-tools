//! Reveal a path in the platform file manager (Explorer / Finder / folder).

use std::path::{Path, PathBuf};
use std::process::Command;

/// Errors while revealing a path in the file manager.
#[derive(Debug, thiserror::Error)]
pub enum RevealError {
    /// Path does not exist.
    #[error("path does not exist: {0}")]
    Missing(String),
    /// Underlying process / IO failure.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

fn absolute_path(path: &Path) -> PathBuf {
    let abs = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    // Windows `canonicalize` yields `\\?\C:\...`, which explorer.exe rejects.
    #[cfg(target_os = "windows")]
    {
        let s = abs.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    abs
}

/// Open the platform file manager with `path` selected when possible.
pub fn reveal_in_file_manager(path: &Path) -> Result<(), RevealError> {
    if !path.exists() {
        return Err(RevealError::Missing(path.display().to_string()));
    }
    let path = absolute_path(path);

    #[cfg(target_os = "windows")]
    {
        // `/select,<path>` must be a single argument for explorer.exe.
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()?;
        Ok(())
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open").args(["-R"]).arg(&path).spawn()?;
        Ok(())
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        reveal_linux(&path)
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
    {
        let _ = path;
        Err(RevealError::Io(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "reveal is not supported on this platform",
        )))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn reveal_linux(path: &Path) -> Result<(), RevealError> {
    let uri = format!("file://{}", path.display());
    // Prefer the FreeDesktop FileManager1 D-Bus API (selects the item).
    let dbus = Command::new("dbus-send")
        .args([
            "--session",
            "--dest=org.freedesktop.FileManager1",
            "--type=method_call",
            "/org/freedesktop/FileManager1",
            "org.freedesktop.FileManager1.ShowItems",
            &format!("array:string:{uri}"),
            "string:",
        ])
        .status();
    if matches!(dbus, Ok(status) if status.success()) {
        return Ok(());
    }

    let parent = path.parent().unwrap_or(path);
    Command::new("xdg-open").arg(parent).spawn()?;
    Ok(())
}
