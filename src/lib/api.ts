// Typed wrappers for every Tauri command and event. Commands reject with a
// pt-BR message (string) ready to show to the user.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { open, save } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import type {
  FileProcessed,
  FilterTemplate,
  HistoryEntry,
  ImportMode,
  ImportSummary,
  MarkerDef,
  Rule,
  RuleNotice,
  RuleStatus,
  SaveRuleResult,
  Settings,
  TestRow,
  Uuid,
  ValidationResult,
} from "./types";

// Commands

export const listRules = () => invoke<Rule[]>("list_rules");

export const getRule = (id: Uuid) => invoke<Rule>("get_rule", { id });

/** Validates (including cycles with other rules), saves and restarts the watcher. */
export const saveRule = (rule: Rule) => invoke<SaveRuleResult>("save_rule", { rule });

export const deleteRule = (id: Uuid) => invoke<void>("delete_rule", { id });

export const toggleRule = (id: Uuid, enabled: boolean) =>
  invoke<Rule>("toggle_rule", { id, enabled });

/** `ids` must list every rule once, in the new order. */
export const reorderRules = (ids: Uuid[]) => invoke<Rule[]>("reorder_rules", { ids });

/** Dry run on the files in the rule's folder; works on unsaved rules. */
export const testRule = (rule: Rule) => invoke<TestRow[]>("test_rule", { rule });

/** Live feedback for the editor: rule problems, cycles and chains. */
export const validateRule = (rule: Rule) => invoke<ValidationResult>("validate_rule", { rule });

/** Files in `dir` as paths relative to it; with `filter`, only matching ones. */
export const listFolderFiles = (
  dir: string,
  includeSubfolders: boolean,
  filter?: string,
  limit?: number,
) => invoke<string[]>("list_folder_files", { dir, includeSubfolders, filter, limit });

/** Native folder picker. Resolves to null when the user cancels. */
export async function pickFolder(title: string, defaultPath?: string): Promise<string | null> {
  const chosen = await open({ directory: true, multiple: false, title, defaultPath });
  return typeof chosen === "string" ? chosen : null;
}

export const previewDestination = (rule: Rule, sampleFileName: string) =>
  invoke<string>("preview_destination", { rule, sampleFileName });

export const listMarkers = () => invoke<MarkerDef[]>("list_markers");

export const listFilterTemplates = () => invoke<FilterTemplate[]>("list_filter_templates");

/** Newest first; the backend keeps the last 500. */
export const getHistory = (limit?: number) => invoke<HistoryEntry[]>("get_history", { limit });

/** Ok / paused / error and last processed file of every rule. */
export const getRuleStatuses = () => invoke<RuleStatus[]>("get_rule_statuses");

/** Restarts a rule paused by the loop protection. */
export const resumeRule = (id: Uuid) => invoke<void>("resume_rule", { id });

/** Writes all rules to a JSON file; resolves to how many were exported. */
export const exportRules = (path: string) => invoke<number>("export_rules", { path });

/** Checks a rules file without changing anything. */
export const inspectImport = (path: string) => invoke<ImportSummary>("inspect_import", { path });

/** Imports a checked rules file; resolves to warnings about rule chains. */
export const importRules = (path: string, mode: ImportMode) =>
  invoke<string[]>("import_rules", { path, mode });

/** Opens a folder (or the folder holding a file) in the system file manager. */
export const openFolder = (path: string) => invoke<void>("open_folder", { path });

/** Opens the newest log file. */
export const openLog = () => invoke<void>("open_log");

/** Native "save as" picker for a JSON file. Null when cancelled. */
export async function pickSaveJson(title: string, defaultPath: string): Promise<string | null> {
  return save({ title, defaultPath, filters: [{ name: "JSON", extensions: ["json"] }] });
}

/** Native picker for one JSON file. Null when cancelled. */
export async function pickJsonFile(title: string): Promise<string | null> {
  const chosen = await open({
    title,
    multiple: false,
    directory: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  });
  return typeof chosen === "string" ? chosen : null;
}

export const appVersion = () => getVersion();

/** Opens a web page in the default browser. */
export const openWebPage = (url: string) => openUrl(url);

export const getSettings = () => invoke<Settings>("get_settings");

export const saveSettings = (settings: Settings) =>
  invoke<Settings>("save_settings", { settings });

// Events. Each returns a function that stops listening.

export const onFileProcessed = (handler: (event: FileProcessed) => void): Promise<UnlistenFn> =>
  listen<FileProcessed>("file-processed", (e) => handler(e.payload));

export const onRuleError = (handler: (event: RuleNotice) => void): Promise<UnlistenFn> =>
  listen<RuleNotice>("rule-error", (e) => handler(e.payload));

export const onRulePaused = (handler: (event: RuleNotice) => void): Promise<UnlistenFn> =>
  listen<RuleNotice>("rule-paused", (e) => handler(e.payload));

export const onRuleResumed = (handler: (event: RuleNotice) => void): Promise<UnlistenFn> =>
  listen<RuleNotice>("rule-resumed", (e) => handler(e.payload));

export const onRuleHint = (handler: (event: RuleNotice) => void): Promise<UnlistenFn> =>
  listen<RuleNotice>("rule-hint", (e) => handler(e.payload));
