//! Register sidecar tools with the host OS so they can be launched independently.
//!
//! - Windows: Start Menu shortcuts are created by NSIS/WiX (see installer templates).
//! - macOS: DMG has no post-install hook, so the packaged launcher writes `.app`
//!   wrappers under `~/Applications` (Launchpad / Spotlight).
//! - Linux: deb/rpm ship `.desktop` files; AppImage / portable builds write
//!   `~/.local/share/applications/*.desktop` at first launch.

#[cfg(any(test, target_os = "macos", target_os = "linux"))]
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

    // Keep the GUI process inside this .app so Launch Services / the menu bar
    // read *this* Info.plist. A trampoline that `exec`s the sidecar in
    // `OMT Tools.app` loses the wrapper identity and shows a blank app name.
    let exe_name = tool.binary_name();
    let exe_dest = macos_dir.join(exe_name);
    let _ = fs::remove_file(macos_dir.join("launch"));
    sync_file(sidecar, &exe_dest)?;
    set_executable(&exe_dest)?;

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
    // CFBundleName is the menu-bar title (Apple: prefer ≤16 chars).
    // CFBundleDisplayName is the Dock / Launchpad label (qualified).
    let name = tool.os_entry_name();
    let display = tool.os_entry_name_qualified();
    let exe = tool.binary_name();
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
    <string>{exe}</string>
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

/// Copy `src` → `dest` when missing or stale. Avoids hardlinks so ad-hoc
/// signing the wrapper cannot rewrite the sidecar inside `OMT Tools.app`.
#[cfg(target_os = "macos")]
fn sync_file(src: &Path, dest: &Path) -> Result<(), String> {
    use std::fs;
    let need_copy = match (fs::metadata(src), fs::metadata(dest)) {
        (Ok(src_meta), Ok(dest_meta)) => {
            src_meta.len() != dest_meta.len()
                || src_meta
                    .modified()
                    .ok()
                    .zip(dest_meta.modified().ok())
                    .map(|(src_t, dest_t)| src_t > dest_t)
                    .unwrap_or(true)
        }
        (Ok(_), Err(_)) => true,
        (Err(err), _) => return Err(err.to_string()),
    };
    if need_copy {
        if dest.exists() {
            fs::remove_file(dest).map_err(|e| e.to_string())?;
        }
        fs::copy(src, dest).map_err(|e| format!("copy sidecar into .app: {e}"))?;
    }
    Ok(())
}

/// Packaged wrapper executable, if the launcher has already registered it.
#[cfg(target_os = "macos")]
pub fn macos_wrapped_executable(tool: ToolId) -> Option<PathBuf> {
    home_dir().map(|home| {
        home.join("Applications")
            .join(format!("{}.app", tool.os_entry_name_qualified()))
            .join("Contents")
            .join("MacOS")
            .join(tool.binary_name())
    })
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
        ToolId::ConfigManager => "View and edit the global OMT settings.xml",
        ToolId::DiscoveryServer => "Run an OMT discovery server for networks that block multicast",
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
        assert!(
            plist
                .contains("<key>CFBundleExecutable</key>\n    <string>omt-studio-monitor</string>")
        );
        assert!(
            plist.contains(
                "<key>CFBundleDisplayName</key>\n    <string>OMT Studio Monitor</string>"
            )
        );
        assert!(plist.contains("<key>CFBundleName</key>\n    <string>Studio Monitor</string>"));
        assert!(!plist.contains("<string>launch</string>"));
    }

    #[test]
    fn desktop_entry_uses_qualified_name() {
        let entry = linux_desktop_entry(ToolId::TestPatterns, "\"/opt/omt-test-patterns\"");
        assert!(entry.contains("Name=OMT Test Patterns"));
        assert!(!entry.contains("Name=\n"));
        assert!(entry.contains("Exec=\"/opt/omt-test-patterns\""));
        assert!(entry.contains("StartupWMClass=omt-test-patterns"));
    }

    #[test]
    fn desktop_path_is_quoted() {
        let quoted = quote_desktop_path(Path::new("/opt/OMT Tools.AppImage"));
        assert_eq!(quoted, "\"/opt/OMT Tools.AppImage\"");
    }
}
