import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

import {
  getSettings,
  getUpdateInstallKind,
  logUpdateError,
  prepareForUpdate,
  resumeMonitorAfterUpdateError,
  saveSettings,
  showUpdateWindow,
} from "$lib/api";
import type { UpdateInstallKind } from "$lib/types";

const RELEASES_URL = "https://github.com/diogosperandio/olhorganizer/releases/latest";
const STARTUP_DELAY_MS = 5_000;

export type UpdatePhase = "available" | "downloading" | "installing";

export interface UpdateDialogState {
  version: string;
  notes: string;
  installKind: UpdateInstallKind;
  phase: UpdatePhase;
  downloaded: number;
  total: number | null;
}

type Listener = (state: UpdateDialogState | null) => void;
type CheckResult = "found" | "none" | "error" | "busy";

let activeUpdate: Update | null = null;
let dialogState: UpdateDialogState | null = null;
let checking = false;
const listeners = new Set<Listener>();

export function subscribeToUpdater(listener: Listener): () => void {
  listeners.add(listener);
  listener(dialogState);
  return () => listeners.delete(listener);
}

export function scheduleStartupUpdateCheck(): () => void {
  const timer = window.setTimeout(() => void checkAtStartup(), STARTUP_DELAY_MS);
  return () => window.clearTimeout(timer);
}

export async function checkForUpdatesNow(onLatest: () => void): Promise<void> {
  const result = await checkForUpdates(true);
  if (result === "none") onLatest();
}

export async function installAvailableUpdate(): Promise<void> {
  const update = activeUpdate;
  const state = dialogState;
  if (!update || !state || state.phase !== "available") return;

  if (state.installKind === "browser") {
    try {
      await openUrl(RELEASES_URL);
    } catch (error) {
      await recordError(error);
    }
    await dismissUpdate();
    return;
  }

  let monitorStopped = false;
  let downloaded = 0;
  let total: number | null = null;
  publish({ ...state, phase: "downloading", downloaded, total });

  try {
    await update.download((event) => {
      if (event.event === "Started") total = event.data.contentLength ?? null;
      if (event.event === "Progress") downloaded += event.data.chunkLength;
      if (event.event === "Finished" && total !== null) downloaded = total;
      publish({ ...state, phase: "downloading", downloaded, total });
    });
    publish({ ...state, phase: "installing", downloaded, total });
    await prepareForUpdate();
    monitorStopped = true;
    await update.install({ restartAfterInstall: true });
    await relaunch();
  } catch (error) {
    if (monitorStopped) {
      try {
        await resumeMonitorAfterUpdateError();
      } catch (resumeError) {
        await recordError(resumeError);
      }
    }
    await recordError(error);
    await dismissUpdate();
  }
}

export async function postponeUpdate(): Promise<void> {
  await dismissUpdate();
}

export async function skipAvailableVersion(): Promise<void> {
  const version = activeUpdate?.version;
  if (!version) return;
  try {
    const settings = await getSettings();
    await saveSettings({ ...settings, skippedVersion: version });
  } catch (error) {
    await recordError(error);
  }
  await dismissUpdate();
}

async function checkAtStartup(): Promise<void> {
  try {
    const settings = await getSettings();
    if (!settings.checkUpdatesOnStart) return;
    await checkForUpdates(false, settings.skippedVersion);
  } catch (error) {
    await recordError(error);
  }
}

async function checkForUpdates(manual: boolean, knownSkippedVersion?: string | null): Promise<CheckResult> {
  if (dialogState) return "found";
  if (checking) return "busy";
  checking = true;
  let checkedUpdate: Update | null = null;
  try {
    checkedUpdate = await check({ timeout: 15_000 });
    if (!checkedUpdate) return "none";

    const skippedVersion = knownSkippedVersion ?? (await getSettings()).skippedVersion;
    if (!manual && skippedVersion === checkedUpdate.version) {
      await checkedUpdate.close();
      checkedUpdate = null;
      return "none";
    }

    const installKind = await getUpdateInstallKind();
    if (activeUpdate) await activeUpdate.close();
    activeUpdate = checkedUpdate;
    checkedUpdate = null;
    publish({
      version: activeUpdate.version,
      notes: activeUpdate.body?.trim() || "",
      installKind,
      phase: "available",
      downloaded: 0,
      total: null,
    });
    try {
      await showUpdateWindow();
    } catch (error) {
      await recordError(error);
    }
    return "found";
  } catch (error) {
    if (checkedUpdate) {
      try { await checkedUpdate.close(); } catch { /* The original error is logged below. */ }
    }
    await recordError(error);
    return "error";
  } finally {
    checking = false;
  }
}

async function dismissUpdate(): Promise<void> {
  const update = activeUpdate;
  activeUpdate = null;
  publish(null);
  if (update) {
    try {
      await update.close();
    } catch (error) {
      await recordError(error);
    }
  }
}

function publish(state: UpdateDialogState | null): void {
  dialogState = state;
  for (const listener of listeners) listener(state);
}

async function recordError(error: unknown): Promise<void> {
  const message = error instanceof Error ? error.message : String(error);
  console.error("Falha ao atualizar o Olhorganizer:", error);
  try {
    await logUpdateError(message);
  } catch (logError) {
    console.error("Falha ao registrar o erro de atualização:", logError);
  }
}
