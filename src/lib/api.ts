import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  FileProcessedPayload,
  FilterTemplate,
  HistoryEntry,
  ImportMode,
  MarkerDef,
  Rule,
  RuleErrorPayload,
  RulePausedPayload,
  Settings,
  TestRow,
  Uuid,
  ValidationResult,
} from "./types";

export const listRules = (): Promise<Rule[]> => invoke("list_rules");
export const getRule = (id: Uuid): Promise<Rule> => invoke("get_rule", { id });
export const saveRule = (rule: Rule): Promise<Rule> => invoke("save_rule", { rule });
export const deleteRule = (id: Uuid): Promise<void> => invoke("delete_rule", { id });
export const toggleRule = (id: Uuid, enabled: boolean): Promise<Rule> =>
  invoke("toggle_rule", { id, enabled });
export const reorderRules = (ids: Uuid[]): Promise<Rule[]> => invoke("reorder_rules", { ids });
export const testRule = (rule: Rule): Promise<TestRow[]> => invoke("test_rule", { rule });
export const validateRule = (rule: Rule): Promise<ValidationResult> =>
  invoke("validate_rule", { rule });
export const previewDestination = (rule: Rule, sampleFileName: string): Promise<string> =>
  invoke("preview_destination", { rule, sampleFileName });
export const listMarkers = (): Promise<MarkerDef[]> => invoke("list_markers");
export const listFilterTemplates = (): Promise<FilterTemplate[]> =>
  invoke("list_filter_templates");
export const getHistory = (limit: number): Promise<HistoryEntry[]> =>
  invoke("get_history", { limit });
export const getSettings = (): Promise<Settings> => invoke("get_settings");
export const saveSettings = (settings: Settings): Promise<Settings> =>
  invoke("save_settings", { settings });
export const exportRules = (path: string): Promise<void> => invoke("export_rules", { path });
export const importRules = (path: string, mode: ImportMode): Promise<Rule[]> =>
  invoke("import_rules", { path, mode });
export const getLogPath = (): Promise<string> => invoke("get_log_path");

export const onFileProcessed = (
  handler: (payload: FileProcessedPayload) => void,
): Promise<UnlistenFn> => listen<FileProcessedPayload>("file-processed", ({ payload }) => handler(payload));

export const onRuleError = (
  handler: (payload: RuleErrorPayload) => void,
): Promise<UnlistenFn> => listen<RuleErrorPayload>("rule-error", ({ payload }) => handler(payload));

export const onRulePaused = (
  handler: (payload: RulePausedPayload) => void,
): Promise<UnlistenFn> => listen<RulePausedPayload>("rule-paused", ({ payload }) => handler(payload));
