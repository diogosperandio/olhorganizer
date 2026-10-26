export type Uuid = string;

export type Action = "move" | "copy";
export type DateSource = "file_name" | "modified" | "detected";
export type CheckMode = "instant" | { interval: { minutes: number } };
export type ConflictPolicy = "rename" | "overwrite" | "skip";

export interface Rule {
  id: Uuid;
  name: string;
  enabled: boolean;
  sourceDir: string;
  filter: string;
  destination: string;
  action: Action;
  dateSource: DateSource;
  checkMode: CheckMode;
  includeSubfolders: boolean;
  onConflict: ConflictPolicy;
  priority: number;
}

export interface Settings {
  checkUpdatesOnStart: boolean;
  skippedVersion: string | null;
  notifications: boolean;
  startWithSystem: boolean;
}

export type Outcome =
  | { kind: "moved"; to: string }
  | { kind: "copied"; to: string }
  | { kind: "skipped"; reason: string }
  | { kind: "wouldMove"; to: string }
  | { kind: "wouldCopy"; to: string }
  | { kind: "notMatched" }
  | { kind: "failed"; error: string };

export interface TestRow {
  relativePath: string;
  outcome: Outcome;
}

export interface ValidationResult {
  errors: string[];
  warnings: string[];
}

export type MarkerKind =
  | "year4"
  | "year2"
  | "month2"
  | "monthName"
  | "day2"
  | "name"
  | "ext";

export interface MarkerDef {
  token: string;
  kind: MarkerKind;
  descriptionPt: string;
  example: string;
}

export interface FilterTemplate {
  namePt: string;
  filter: string;
}

export interface FileProcessedPayload {
  ruleId: Uuid;
  file: string;
  outcome: Outcome;
}

export interface RuleErrorPayload {
  ruleId: Uuid;
  messagePt: string;
}

export interface RulePausedPayload {
  ruleId: Uuid;
  reasonPt: string;
}

export type AppEvent =
  | { name: "file-processed"; payload: FileProcessedPayload }
  | { name: "rule-error"; payload: RuleErrorPayload }
  | { name: "rule-paused"; payload: RulePausedPayload };

export interface HistoryEntry {
  timestampMs: number;
  event: AppEvent;
}
