//! Sidecar / local binary launch helpers.

use std::path::{Path, PathBuf};
use std::process::Command;

use suite_core::{LaunchOverrides, ToolId, load_config};
use tauri::{AppHandle, Manager};

/// `ensure-sidecar-placeholders` used to copy `cmd.exe` when a tool had not
/// been built yet. Tauri then copied that file next to the launcher as
/// `omt-config-manager.exe`, so Launch opened a console instead of the GUI.
const MIN_TOOL_BYTES: u64 = 64 * 1024;

/// Parse a launcher / CLI tool id (`studio-monitor`, `test-patterns`, ...).
pub fn parse_tool_id(tool_id: &str) -> Result<ToolId, String> {
    match tool_id {
        "studio-monitor" => Ok(ToolId::StudioMonitor),
        "test-patterns" => Ok(ToolId::TestPatterns),
        "config-manager" => Ok(ToolId::ConfigManager),
        "discovery-server" => Ok(ToolId::DiscoveryServer),
        "screen-capture" => Ok(ToolId::ScreenCapture),
        _ => Err(format!("unknown tool: {tool_id}")),
    }
}

/// Resolve the on-disk path for a bundled tool binary.
pub fn resolve_tool_path(app: &AppHandle, tool: ToolId) -> Result<PathBuf, String> {
    let name = tool.binary_name();

    // Prefer Tauri resource/sidecar layout when packaged.
    if let Ok(resource) = app.path().resource_dir() {
        let candidates = [
            resource.join(name),
            resource.join(format!("{name}.exe")),
            resource.join("binaries").join(name),
            resource.join("binaries").join(format!("{name}.exe")),
        ];
        for path in candidates {
            if is_usable_tool_binary(&path) {
                return Ok(path);
            }
        }
    }

    // Development: look next to the launcher binary and in workspace target dirs.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for path in [
                dir.join(name),
                dir.join(format!("{name}.exe")),
                dir.join("binaries").join(name),
                dir.join("binaries").join(format!("{name}.exe")),
            ] {
                if is_usable_tool_binary(&path) {
                    return Ok(path);
                }
            }
        }
    }

    // Workspace target/{debug,release}/
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = manifest_dir
        .ancestors()
        .nth(3)
        .map(|p| p.to_path_buf())
        .unwrap_or(manifest_dir);
    for profile in ["debug", "release"] {
        let path = workspace_root.join("target").join(profile).join(name);
        let path_exe = workspace_root
            .join("target")
            .join(profile)
            .join(format!("{name}.exe"));
        if is_usable_tool_binary(&path_exe) {
            return Ok(path_exe);
        }
        if is_usable_tool_binary(&path) {
            return Ok(path);
        }
    }

    Err(format!("tool binary not found: {name}"))
}

/// Whether the tool binary is present on disk.
pub fn tool_available(app: &AppHandle, tool: ToolId) -> bool {
    resolve_tool_path(app, tool).is_ok()
}

fn resolve_launch_path(app: &AppHandle, tool: ToolId) -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(wrapped) = crate::os_shortcuts::macos_wrapped_executable(tool) {
            if is_usable_tool_binary(&wrapped) {
                return Ok(wrapped);
            }
        }
    }
    resolve_tool_path(app, tool)
}

/// Launch a tool with current suite language/theme overrides.
pub fn launch_tool(app: &AppHandle, tool: ToolId) -> Result<(), String> {
    if tool == ToolId::ScreenCapture {
        return Err("Screen Capture is not enabled in this suite build".into());
    }
    let path = resolve_launch_path(app, tool)?;
    let cfg = load_config().unwrap_or_default();
    let overrides = LaunchOverrides {
        language: cfg.language,
        theme: cfg.theme,
        suite_version: suite_core::SUITE_VERSION.to_string(),
    };
    let mut cmd = Command::new(&path);
    overrides.apply_to_command(&mut cmd);
    cmd.arg("--language").arg(overrides.language.as_str());
    cmd.arg("--theme").arg(overrides.theme.as_str());
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("failed to launch {}: {e}", path.display()))
}

/// Best-effort list of running tool process names (Windows + Unix).
pub fn running_tool_names() -> Vec<String> {
    let mut found = Vec::new();
    for tool in ToolId::all() {
        let name = tool.binary_name();
        if process_running(name) {
            found.push(name.to_string());
        }
    }
    found
}

#[cfg(windows)]
fn process_running(name: &str) -> bool {
    let exe = format!("{name}.exe");
    std::process::Command::new("tasklist")
        .args(["/FI", &format!("IMAGENAME eq {exe}"), "/NH"])
        .output()
        .map(|o| {
            let stdout = String::from_utf8_lossy(&o.stdout);
            stdout
                .to_ascii_lowercase()
                .contains(&exe.to_ascii_lowercase())
        })
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn process_running(name: &str) -> bool {
    std::process::Command::new("pgrep")
        .arg("-f")
        .arg(name)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn is_usable_tool_binary(path: &Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() || meta.len() < MIN_TOOL_BYTES {
        return false;
    }
    !is_cmd_exe_clone(path, meta.len())
}

#[cfg(windows)]
fn is_cmd_exe_clone(path: &Path, len: u64) -> bool {
    let _ = path;
    let windir = std::env::var_os("WINDIR").unwrap_or_else(|| r"C:\Windows".into());
    let cmd = PathBuf::from(windir).join("System32").join("cmd.exe");
    std::fs::metadata(cmd)
        .map(|cmd_meta| cmd_meta.len() == len)
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_cmd_exe_clone(_path: &Path, _len: u64) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn rejects_tiny_stub_and_missing_path() {
        assert!(!is_usable_tool_binary(Path::new(
            "this-file-does-not-exist.exe"
        )));
        let dir = std::env::temp_dir();
        let stub = dir.join("omt-tools-stub-binary.exe");
        {
            let mut f = std::fs::File::create(&stub).unwrap();
            f.write_all(&[b'M', b'Z']).unwrap();
        }
        assert!(!is_usable_tool_binary(&stub));
        let _ = std::fs::remove_file(&stub);
    }
}
