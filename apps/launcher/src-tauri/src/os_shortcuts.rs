//! Register sidecar tools with the host OS so they can be launched independently.
//!
//! - Windows: Start Menu shortcuts are created by NSIS/WiX (see installer templates).
//! - macOS: DMG has no post-install hook, so the packaged launcher writes `.app`
//!   wrappers under `~/Applications` (Launchpad / Spotlight).
//! - Linux: deb/rpm ship `.desktop` files; AppImage / portable builds write
//!   `~/.local/share/applications/*.desktop` at first launch.

use std::path::Path;
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::path::PathBuf;

use suite_core::ToolId;
use tauri::AppHandle;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use suite_core::suite_manifest;

#[cfg(any(target_os = "macos", target_os = "linux"))]
use crate::tools;

const MACOS_BUNDLE_PREFIX: &str = "lab.mikansei.omt-tools";

#[cfg(target_os = "linux")]
const DESKTOP_FILE_PREFIX: &str = "lab.mikansei.omt";

/// Best-effort registration. Failures are logged and never block the launcher.
pub fn ensure_registered(app: &AppHandle) {
    if !should_register() {
        return;
    }
    if let Err(err) = register(app) {
        eprintln!("omt-tools: failed to register OS app entries: {err}");
    }
}

/// `--launch studio-monitor` / `--launch=test-patterns` for AppImage desktop entries.
pub fn parse_launch_tool_id() -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == "--launch" {
            return args.next();
        }
        if let Some(id) = arg.strip_prefix("--launch=") {
            return Some(id.to_string());
        }
    }
    None
}

fn should_register() -> bool {
    if cfg!(debug_assertions) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let exe = exe.to_string_lossy();
    !exe.contains("/target/") && !exe.contains("\\target\\")
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn enabled_tools() -> Vec<ToolId> {
    suite_manifest()
        .tools
        .into_iter()
        .filter(|tool| tool.enabled)
        .map(|tool| tool.id)
        .collect()
}

fn register(app: &AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        register_macos(app)?;
    }
    #[cfg(target_os = "linux")]
    {
        register_linux(app)?;
    }
    #[cfg(windows)]
    {
        let _ = app;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn register_macos(app: &AppHandle) -> Result<(), String> {
    use std::fs;

    let apps_dir = user_applications_dir()?;
    fs::create_dir_all(&apps_dir).map_err(|e| format!("create ~/Applications: {e}"))?;
    let icon = macos_icon_path();
    for tool in enabled_tools() {
        let sidecar = tools::resolve_tool_path(app, tool)?;
        write_macos_wrapper(&apps_dir, tool, &sidecar, icon.as_deref())?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn user_applications_dir() -> Result<PathBuf, String> {
    home_dir()
        .map(|home| home.join("Applications"))
        .ok_or_else(|| "HOME is not set".into())
}

#[cfg(target_os = "macos")]
fn macos_icon_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let resources = exe.parent()?.parent()?.join("Resources");
    for name in ["icon.icns", "AppIcon.icns"] {
        let path = resources.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn write_macos_wrapper(
    apps_dir: &Path,
    tool: ToolId,
    sidecar: &Path,
    icon: Option<&Path>,
) -> Result<(), String> {
    use std::fs;

    let app_dir = apps_dir.join(format!("{}.app", tool.os_entry_name_qualified()));
    let contents = app_dir.join("Contents");
    let macos_dir = contents.join("MacOS");
    let resources = contents.join("Resources");
    fs::create_dir_all(&macos_dir).map_err(|e| e.to_string())?;
    fs::create_dir_all(&resources).map_err(|e| e.to_string())?;

    fs::write(contents.join("PkgInfo"), b"APPL????").map_err(|e| e.to_string())?;
    fs::write(
        contents.join("Info.plist"),
        macos_info_plist(tool, env!("CARGO_PKG_VERSION")),
    )
    .map_err(|e| e.to_string())?;

    let launch = macos_dir.join("launch");
    fs::write(&launch, macos_launch_script(sidecar)).map_err(|e| e.to_string())?;
    set_executable(&launch)?;

    if let Some(icon) = icon {
        let dest = resources.join("AppIcon.icns");
        let _ = fs::copy(icon, dest);
    }

    adhoc_codesign(&app_dir);
    lsregister(&app_dir);
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
fn macos_info_plist(tool: ToolId, version: &str) -> String {
    let name = tool.os_entry_name_qualified();
    let display = tool.os_entry_name();
    let id = tool.os_entry_id();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>{display}</string>
    <key>CFBundleExecutable</key>
    <string>launch</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>{MACOS_BUNDLE_PREFIX}.{id}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>{name}</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn macos_launch_script(sidecar: &Path) -> String {
    let quoted = shell_single_quote(&sidecar.to_string_lossy());
    format!(
        "#!/bin/sh\nTARGET={quoted}\nif [ ! -x \"$TARGET\" ]; then\n  osascript -e 'display dialog \"OMT Tools is not installed.\" buttons {{\"OK\"}} default button 1 with icon stop'\n  exit 1\nfi\nexec \"$TARGET\" \"$@\"\n"
    )
}

#[cfg_attr(not(test), allow(dead_code))]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

#[cfg(target_os = "macos")]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(path).map_err(|e| e.to_string())?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(path, perms).map_err(|e| e.to_string())
}

#[cfg(target_os = "macos")]
fn adhoc_codesign(app_dir: &Path) {
    let _ = std::process::Command::new("codesign")
        .args(["--force", "--sign", "-"])
        .arg(app_dir)
        .status();
}

#[cfg(target_os = "macos")]
fn lsregister(app_dir: &Path) {
    let _ = std::process::Command::new("/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister")
        .args(["-f"])
        .arg(app_dir)
        .status();
}

#[cfg(target_os = "linux")]
fn register_linux(app: &AppHandle) -> Result<(), String> {
    use std::fs;

    if Path::new("/usr/share/applications/lab.mikansei.omt-studio-monitor.desktop").exists() {
        return Ok(());
    }
    let apps_dir = linux_applications_dir()?;
    fs::create_dir_all(&apps_dir).map_err(|e| format!("create applications dir: {e}"))?;
    let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
    for tool in enabled_tools() {
        let exec = if let Some(appimage) = &appimage {
            format!(
                "{} --launch {}",
                quote_desktop_path(appimage),
                tool.os_entry_id()
            )
        } else {
            quote_desktop_path(&tools::resolve_tool_path(app, tool)?)
        };
        let desktop = linux_desktop_entry(tool, &exec);
        let path = apps_dir.join(format!(
            "{DESKTOP_FILE_PREFIX}-{}.desktop",
            tool.os_entry_id()
        ));
        fs::write(&path, desktop).map_err(|e| e.to_string())?;
        let _ = std::process::Command::new("chmod")
            .args(["+x"])
            .arg(&path)
            .status();
    }
    let _ = std::process::Command::new("update-desktop-database")
        .arg(&apps_dir)
        .status();
    Ok(())
}

#[cfg(target_os = "linux")]
fn linux_applications_dir() -> Result<PathBuf, String> {
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return Ok(PathBuf::from(xdg).join("applications"));
    }
    home_dir()
        .map(|home| home.join(".local/share/applications"))
        .ok_or_else(|| "HOME is not set".into())
}

#[cfg_attr(not(test), allow(dead_code))]
fn linux_desktop_entry(tool: ToolId, exec: &str) -> String {
    let name = tool.os_entry_name_qualified();
    let comment = match tool {
        ToolId::StudioMonitor => "Browse and view OMT sources on the LAN",
        ToolId::TestPatterns => "Pick a pattern and send video + tone over OMT",
        ToolId::ScreenCapture => "Capture the desktop and send over OMT",
    };
    format!(
        "\
[Desktop Entry]\n\
Type=Application\n\
Version=1.0\n\
Name={name}\n\
Comment={comment}\n\
Exec={exec}\n\
Icon=OMT Tools\n\
Terminal=false\n\
StartupNotify=true\n\
StartupWMClass={wm}\n\
Categories=AudioVideo;Video;AudioVideoEditing;\n\
Keywords=OMT;NDI;video;\n",
        wm = tool.binary_name()
    )
}

#[cfg(any(test, target_os = "linux"))]
fn quote_desktop_path(path: &Path) -> String {
    let value = path.to_string_lossy().replace('"', "\\\"");
    format!("\"{value}\"")
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plist_contains_bundle_id_and_executable() {
        let plist = macos_info_plist(ToolId::StudioMonitor, "0.1.0");
        assert!(plist.contains("lab.mikansei.omt-tools.studio-monitor"));
        assert!(plist.contains("<string>launch</string>"));
        assert!(plist.contains("OMT Studio Monitor"));
    }

    #[test]
    fn launch_script_quotes_sidecar_path() {
        let script = macos_launch_script(Path::new(
            "/Applications/OMT Tools.app/Contents/MacOS/omt-studio-monitor",
        ));
        assert!(script.contains("'/Applications/OMT Tools.app/Contents/MacOS/omt-studio-monitor'"));
        assert!(script.contains("exec \"$TARGET\""));
    }

    #[test]
    fn desktop_entry_uses_qualified_name() {
        let entry = linux_desktop_entry(ToolId::TestPatterns, "\"/opt/omt-test-patterns\"");
        assert!(entry.contains("Name=OMT Test Patterns"));
        assert!(entry.contains("Exec=\"/opt/omt-test-patterns\""));
        assert!(entry.contains("StartupWMClass=omt-test-patterns"));
    }

    #[test]
    fn desktop_path_is_quoted() {
        let quoted = quote_desktop_path(Path::new("/opt/OMT Tools.AppImage"));
        assert_eq!(quoted, "\"/opt/OMT Tools.AppImage\"");
    }
}
