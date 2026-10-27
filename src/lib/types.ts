// Mirrors of the Rust structs in src-tauri/src/dto.rs and events.rs, plus
// motor's enums. Kept in sync by hand: change both sides together.

export type Uuid = string;

export type Action = "move" | "copy";

/** Where date markers ({YYYY}, {MM}...) take the date from. */
export type DateSource = "file_name" | "modified" | "detected";

export type CheckMode = { mode: "instant" } | { mode: "interval"; minutes: number };

export type ConflictPolicy = "rename" | "overwrite" | "skip";

export interface Rule {
  /** Missing or null for a rule not saved yet. */
  id?: Uuid | null;
  name: string;
  enabled: boolean;
  sourceDir: string;
  /** Also watch subfolders; the destination must then be outside sourceDir. */
  includeSubfolders: boolean;
  filter: string;
  destination: string;
  action: Action;
  dateSource: DateSource;
  checkMode: CheckMode;
  onConflict: ConflictPolicy;
  /** Lower runs first. Set by the backend for new rules and by reorderRules. */
  priority: number;
}

export interface Settings {
  checkUpdatesOnStart: boolean;
  skippedVersion: string | null;
  notifications: boolean;
  startWithSystem: boolean;
}

/** Result of processing one file (motor's Outcome; tag values are snake_case). */
export type Outcome =
  | { kind: "moved"; to: string }
  | { kind: "copied"; to: string }
  | { kind: "skipped"; reason: string }
  | { kind: "already_copied"; to: string }
  | { kind: "would_move"; to: string }
  | { kind: "would_copy"; to: string }
  | { kind: "not_matched" }
  | { kind: "failed"; error: string };

export interface TestRow {
  /** Relative to the rule's sourceDir, "/"-separated: "2025/fotos/IMG_0001.jpg". */
  relativePath: string;
  outcome: Outcome;
}

/** The editor field a validation message belongs to. */
export type RuleField = "name" | "sourceDir" | "filter" | "destination" | "dateSource" | "checkMode";

export interface FieldMessage {
  field: RuleField;
  message: string;
}

export interface ValidationResult {
  /** Block saving. */
  errors: FieldMessage[];
  /** Shown, but do not block saving. */
  warnings: FieldMessage[];
}

export interface SaveRuleResult {
  rule: Rule;
  warnings: string[];
}

export type MarkerKind = "Year4" | "Year2" | "Month2" | "MonthName" | "Day2" | "Name" | "Ext";

export interface MarkerDef {
  /** Text between the braces: "YYYY" for {YYYY}. */
  token: string;
  kind: MarkerKind;
  descriptionPt: string;
  example: string;
  usableInFilter: boolean;
}

export interface FilterTemplate {
  namePt: string;
  filter: string;
}

/** Payload of the "file-processed" event. */
export interface FileProcessed {
  /** Milliseconds since the Unix epoch. */
  at: number;
  ruleId: Uuid;
  ruleName: string;
  file: string;
  relativePath: string;
  outcome: Outcome;
  messagePt: string;
}

/** Payload of "rule-error", "rule-paused", "rule-resumed" and "rule-hint". */
export interface RuleNotice {
  at: number;
  ruleId: Uuid;
  ruleName: string;
  messagePt: string;
  /** Only on "rule-paused": paused by the loop protection; offer "Retomar". */
  canResume: boolean;
}

export type RuleState = "ok" | "paused" | "error";

/** Current state of a rule (src-tauri/src/status.rs). */
export interface RuleStatus {
  ruleId: Uuid;
  state: RuleState;
  messagePt: string | null;
  canResume: boolean;
  /** Last file moved or copied, relative to the source folder. */
  lastFile: string | null;
  /** Milliseconds since the Unix epoch. */
  lastAt: number | null;
}

export type HistoryEntry =
  | ({ type: "file-processed" } & FileProcessed)
  | ({ type: "rule-error" } & RuleNotice)
  | ({ type: "rule-paused" } & RuleNotice)
  | ({ type: "rule-resumed" } & RuleNotice)
  | ({ type: "rule-hint" } & RuleNotice);
