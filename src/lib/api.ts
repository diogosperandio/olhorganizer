import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { toPlainJson } from "./ipc";

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
  UpdateInstallKind,
  Uuid,
  ValidationResult,
} from "./types";

function invokeCommand<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const plainArgs = args ? toPlainJson(args) : undefined;
  return invoke<T>(command, plainArgs);
}

export const listRules = (): Promise<Rule[]> => invokeCommand("list_rules");
export const getRule = (id: Uuid): Promise<Rule> => invokeCommand("get_rule", { id });
export const saveRule = (rule: Rule): Promise<Rule> => invokeCommand("save_rule", { rule });
export const deleteRule = (id: Uuid): Promise<void> => invokeCommand("delete_rule", { id });
export const toggleRule = (id: Uuid, enabled: boolean): Promise<Rule> =>
  invokeCommand("toggle_rule", { id, enabled });
export const reorderRules = (ids: Uuid[]): Promise<Rule[]> => invokeCommand("reorder_rules", { ids });
export const testRule = (rule: Rule): Promise<TestRow[]> => invokeCommand("test_rule", { rule });
export const validateRule = (rule: Rule): Promise<ValidationResult> =>
  invokeCommand("validate_rule", { rule });
export const previewDestination = (rule: Rule, sampleFileName: string): Promise<string> =>
  invokeCommand("preview_destination", { rule, sampleFileName });
export const listMarkers = (): Promise<MarkerDef[]> => invokeCommand("list_markers");
export const listFilterTemplates = (): Promise<FilterTemplate[]> =>
  invokeCommand("list_filter_templates");
export const getHistory = (limit: number): Promise<HistoryEntry[]> =>
  invokeCommand("get_history", { limit });
export const getSettings = (): Promise<Settings> => invokeCommand("get_settings");
export const saveSettings = (settings: Settings): Promise<Settings> =>
  invokeCommand("save_settings", { settings });
export const exportRules = (path: string): Promise<void> => invokeCommand("export_rules", { path });
export const importRules = (path: string, mode: ImportMode): Promise<Rule[]> =>
  invokeCommand("import_rules", { path, mode });
export const getLogPath = (): Promise<string> => invokeCommand("get_log_path");
export const prepareForUpdate = (): Promise<void> => invokeCommand("prepare_for_update");
export const resumeMonitorAfterUpdateError = (): Promise<void> =>
  invokeCommand("resume_monitor_after_update_error");
export const getUpdateInstallKind = (): Promise<UpdateInstallKind> =>
  invokeCommand("update_install_kind");
export const logUpdateError = (message: string): Promise<void> =>
  invokeCommand("log_update_error", { message });
export const showUpdateWindow = (): Promise<void> => invokeCommand("show_update_window");

export const onFileProcessed = (
  handler: (payload: FileProcessedPayload) => void,
): Promise<UnlistenFn> => listen<FileProcessedPayload>("file-processed", ({ payload }) => handler(payload));

export const onRuleError = (
  handler: (payload: RuleErrorPayload) => void,
): Promise<UnlistenFn> => listen<RuleErrorPayload>("rule-error", ({ payload }) => handler(payload));

export const onRulePaused = (
  handler: (payload: RulePausedPayload) => void,
): Promise<UnlistenFn> => listen<RulePausedPayload>("rule-paused", ({ payload }) => handler(payload));

export const onNavigateHistory = (handler: () => void): Promise<UnlistenFn> =>
  listen("navigate-history", handler);
