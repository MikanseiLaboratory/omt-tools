import { invoke } from "@tauri-apps/api/core";

type ToolCard = {
  id: string;
  title: string;
  description: string;
  binary: string;
  enabled: boolean;
  available: boolean;
  version: string;
};

type Labels = {
  title: string;
  subtitle: string;
  settings: string;
  language: string;
  theme: string;
  version: string;
  launch: string;
  back: string;
  themeLight: string;
  themeDark: string;
  themeSystem: string;
  unavailable: string;
};

type LauncherState = {
  language: string;
  theme: string;
  suiteVersion: string;
  labels: Labels;
  tools: ToolCard[];
  manifest: {
    suite_version: string;
    target: string;
    tools: Array<{ id: string; version: string; binary: string; enabled: boolean }>;
  };
};

let state: LauncherState | null = null;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function applyTheme(theme: string) {
  const root = document.documentElement;
  root.dataset.theme = theme;
  if (theme === "system") {
    root.removeAttribute("data-theme");
  }
}

function renderLabels(labels: Labels) {
  $("title").textContent = labels.title;
  $("subtitle").textContent = labels.subtitle;
  $("settings-btn").textContent = labels.settings;
  $("settings-title").textContent = labels.settings;
  $("language-label").textContent = labels.language;
  $("theme-label").textContent = labels.theme;
  $("version-label").textContent = labels.version;
  $("back-btn").textContent = labels.back;
  $("save-settings").textContent = labels.settings;

  const themeSelect = $("theme-select") as HTMLSelectElement;
  themeSelect.options[0].text = labels.themeSystem;
  themeSelect.options[1].text = labels.themeLight;
  themeSelect.options[2].text = labels.themeDark;
}

function renderTools(tools: ToolCard[], labels: Labels) {
  const grid = $("tool-grid");
  grid.innerHTML = "";
  for (const tool of tools) {
    const card = document.createElement("article");
    card.className = "tool-card";
    const canLaunch = tool.enabled && tool.available;
    card.innerHTML = `
      <h3>${tool.title}</h3>
      <p>${tool.description}</p>
      <div class="meta">v${tool.version}</div>
      <button type="button" data-tool="${tool.id}" ${canLaunch ? "" : "disabled"}>
        ${canLaunch ? labels.launch : labels.unavailable}
      </button>
    `;
    grid.appendChild(card);
  }
  grid.querySelectorAll<HTMLButtonElement>("button[data-tool]").forEach((btn) => {
    btn.addEventListener("click", async () => {
      const toolId = btn.dataset.tool;
      if (!toolId) return;
      try {
        $("status").textContent = "";
        await invoke("launch_tool", { toolId });
      } catch (err) {
        $("status").textContent = String(err);
      }
    });
  });
}

function renderManifest(state: LauncherState) {
  $("suite-version").textContent = state.suiteVersion;
  const lines = state.manifest.tools.map(
    (t) => `${String(t.id)} ${t.version} (${t.enabled ? "enabled" : "preview"})`,
  );
  $("manifest-tools").textContent = lines.join("\n");
}

async function refresh() {
  state = await invoke<LauncherState>("get_launcher_state");
  applyTheme(state.theme);
  renderLabels(state.labels);
  renderTools(state.tools, state.labels);
  renderManifest(state);
  ($("language-select") as HTMLSelectElement).value = state.language;
  ($("theme-select") as HTMLSelectElement).value = state.theme;
}

function showSettings(show: boolean) {
  $("home-view").classList.toggle("hidden", show);
  $("settings-view").classList.toggle("hidden", !show);
}

window.addEventListener("DOMContentLoaded", async () => {
  $("settings-btn").addEventListener("click", () => showSettings(true));
  $("back-btn").addEventListener("click", () => showSettings(false));
  $("save-settings").addEventListener("click", async () => {
    const language = ($("language-select") as HTMLSelectElement).value;
    const theme = ($("theme-select") as HTMLSelectElement).value;
    try {
      state = await invoke<LauncherState>("save_settings", {
        args: { language, theme },
      });
      applyTheme(state.theme);
      renderLabels(state.labels);
      renderTools(state.tools, state.labels);
      renderManifest(state);
      $("settings-status").textContent = "OK";
      showSettings(false);
    } catch (err) {
      $("settings-status").textContent = String(err);
    }
  });

  try {
    await refresh();
  } catch (err) {
    $("status").textContent = String(err);
  }
});
