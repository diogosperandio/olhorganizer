// Typed wrappers for every Tauri command and event. Commands reject with a
// pt-BR message (string) ready to show to the user.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  FileProcessed,
  FilterTemplate,
  HistoryEntry,
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
