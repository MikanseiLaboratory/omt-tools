//! OMT Tools Tauri launcher backend.

mod os_shortcuts;
mod tools;

use serde::{Deserialize, Serialize};
use suite_core::{
    Language, SimdCapabilities, SuiteManifest, ThemePreference, ToolId, load_config, save_config,
    suite_manifest, t,
};
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ToolCard {
    id: String,
    title: String,
    description: String,
    binary: String,
    enabled: bool,
    available: bool,
    version: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct LauncherState {
    language: String,
    theme: String,
    suite_version: String,
    /// Compact SIMD capability summary for the settings panel.
    simd: String,
    labels: Labels,
    tools: Vec<ToolCard>,
    manifest: SuiteManifest,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Labels {
    title: String,
    subtitle: String,
    settings: String,
    docs: String,
    save: String,
    language: String,
    theme: String,
    version: String,
    launch: String,
    launching: String,
    back: String,
    theme_light: String,
    theme_dark: String,
    theme_system: String,
    unavailable: String,
    simd: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SaveSettingsArgs {
    language: String,
    theme: String,
}

fn labels(lang: Language) -> Labels {
    Labels {
        title: t(lang, "app.title").to_string(),
        subtitle: t(lang, "launcher.subtitle").to_string(),
        settings: t(lang, "settings").to_string(),
        docs: t(lang, "docs").to_string(),
        save: t(lang, "save").to_string(),
        language: t(lang, "language").to_string(),
        theme: t(lang, "theme").to_string(),
        version: t(lang, "version").to_string(),
        launch: t(lang, "launch").to_string(),
        launching: t(lang, "launching").to_string(),
        back: t(lang, "back").to_string(),
        theme_light: t(lang, "theme.light").to_string(),
        theme_dark: t(lang, "theme.dark").to_string(),
        theme_system: t(lang, "theme.system").to_string(),
        unavailable: t(lang, "tool.unavailable").to_string(),
        simd: t(lang, "simd").to_string(),
    }
}

fn build_state(app: &tauri::AppHandle) -> Result<LauncherState, String> {
    let cfg = load_config().unwrap_or_default();
    let manifest = suite_manifest();
    let mut tools = Vec::new();
    for info in &manifest.tools {
        let available = tools::tool_available(app, info.id);
        tools.push(ToolCard {
            id: match info.id {
                ToolId::StudioMonitor => "studio-monitor".into(),
                ToolId::TestPatterns => "test-patterns".into(),
                ToolId::ScreenCapture => "screen-capture".into(),
            },
            title: t(cfg.language, info.id.title_key()).to_string(),
            description: t(cfg.language, info.id.description_key()).to_string(),
            binary: info.binary.clone(),
            enabled: info.enabled,
            available: available && info.enabled,
            version: info.version.clone(),
        });
    }
    Ok(LauncherState {
        language: cfg.language.as_str().to_string(),
        theme: cfg.theme.as_str().to_string(),
        suite_version: manifest.suite_version.clone(),
        simd: SimdCapabilities::detect().summary(),
        labels: labels(cfg.language),
        tools,
        manifest,
    })
}

#[tauri::command]
fn get_launcher_state(app: tauri::AppHandle) -> Result<LauncherState, String> {
    build_state(&app)
}

#[tauri::command]
fn save_settings(app: tauri::AppHandle, args: SaveSettingsArgs) -> Result<LauncherState, String> {
    let language = args
        .language
        .parse::<Language>()
        .map_err(|_| "invalid language".to_string())?;
    let theme = args
        .theme
        .parse::<ThemePreference>()
        .map_err(|_| "invalid theme".to_string())?;
    let mut cfg = load_config().unwrap_or_default();
    cfg.language = language;
    cfg.theme = theme;
    save_config(&cfg).map_err(|e| e.to_string())?;
    build_state(&app)
}

#[tauri::command]
fn launch_tool(app: tauri::AppHandle, tool_id: String) -> Result<(), String> {
    let id = tools::parse_tool_id(&tool_id)?;
    tools::launch_tool(&app, id)
}

#[tauri::command]
fn list_running_tools() -> Result<Vec<String>, String> {
    Ok(tools::running_tool_names())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // No native application menu — keep chrome minimal like NDI Tools.
        .menu(tauri::menu::Menu::new)
        .setup(|app| {
            if let Some(tool_id) = os_shortcuts::parse_launch_tool_id() {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
                let id = tools::parse_tool_id(&tool_id)?;
                tools::launch_tool(app.handle(), id)?;
                std::process::exit(0);
            }
            os_shortcuts::ensure_registered(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_launcher_state,
            save_settings,
            launch_tool,
            list_running_tools
        ])
        .run(tauri::generate_context!())
        .expect("error while running OMT Tools launcher");
}
