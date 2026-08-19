#!/usr/bin/env bun
// Build sidecar tools and copy them into apps/launcher/src-tauri/binaries
// with the host target-triple suffix that Tauri's externalBin expects.
// Replaces cmd.exe placeholders that used to be copied by ensure-sidecar-placeholders.

import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, statSync, unlinkSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const names = [
  "omt-studio-monitor",
  "omt-test-patterns",
  "omt-config-manager",
  "omt-discovery-server-gui",
  "omt-discovery-server",
];

function hostTriple() {
  const out = spawnSync("rustc", ["-vV"], { encoding: "utf8" });
  const line = (out.stdout ?? "").split(/\r?\n/).find((l) => l.startsWith("host:"));
  if (!line) {
    throw new Error("could not read rustc host triple");
  }
  return line.split(/\s+/)[1];
}

function cmdExePath() {
  const windir = process.env.WINDIR ?? "C:\\Windows";
  return join(windir, "System32", "cmd.exe");
}

function isCmdClone(path) {
  if (process.platform !== "win32" || !existsSync(path)) {
    return false;
  }
  const cmd = cmdExePath();
  if (!existsSync(cmd)) {
    return false;
  }
  return statSync(path).size === statSync(cmd).size;
}

function isStub(path) {
  if (!existsSync(path)) {
    return true;
  }
  return statSync(path).size < 64 * 1024;
}

const windows = process.platform === "win32";
const debugDir = join(root, "target", "debug");

for (const name of names) {
  const debugBin = join(debugDir, windows ? `${name}.exe` : name);
  if (isCmdClone(debugBin) || (existsSync(debugBin) && isStub(debugBin))) {
    unlinkSync(debugBin);
  }
}

const cargo = spawnSync(
  "cargo",
  [
    "build",
    "-p",
    "omt-studio-monitor",
    "-p",
    "omt-test-patterns",
    "-p",
    "omt-config-manager",
    "-p",
    "omt-discovery-server",
  ],
  { cwd: root, stdio: "inherit", shell: windows },
);
if (cargo.status !== 0) {
  process.exit(cargo.status ?? 1);
}

const triple = hostTriple();
const destDir = join(root, "apps", "launcher", "src-tauri", "binaries");
mkdirSync(destDir, { recursive: true });

for (const name of names) {
  const src = join(debugDir, windows ? `${name}.exe` : name);
  if (!existsSync(src) || isCmdClone(src) || isStub(src)) {
    console.warn(`skip ${name}: missing or placeholder at ${src}`);
    continue;
  }
  const dst = join(destDir, windows ? `${name}-${triple}.exe` : `${name}-${triple}`);
  copyFileSync(src, dst);
  console.log(`prepared ${name}`);
}
