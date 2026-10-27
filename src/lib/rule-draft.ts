// Helpers for editing a rule in the wizard: the filter is kept as one string
// (the source of truth) and split into "which names" + "which types" for the
// clickable controls.

import type { Rule } from "./types";

const BUILTIN = ["YYYY", "YY", "MM", "MES", "DD", "NOME", "EXT"];

export function newDraft(): Rule {
  return {
    id: null,
    name: "",
    enabled: true,
    sourceDir: "",
    includeSubfolders: false,
    filter: "*",
    destination: "",
    action: "move",
    dateSource: "modified",
    checkMode: { mode: "instant" },
    onConflict: "rename",
    priority: 0,
  };
}

export interface FilterParts {
  /** Name part, without the extension: "*", "{YYYY}{MM}{DD}", "{TEXTO:cliente}_*". */
  pattern: string;
  /** Lower-case extensions; empty means any type. */
  types: string[];
}

/** "{YYYY}{MM}{DD}.{pdf,jpg}" -> { pattern: "{YYYY}{MM}{DD}", types: ["pdf", "jpg"] }.
 *  Null when the filter does not fit that shape (edited by hand). */
export function splitFilter(filter: string): FilterParts | null {
  const f = filter.trim();
  if (f === "" || f === "*") return { pattern: "*", types: [] };
  const anyType = f.match(/^(.+)\.\*$/);
  if (anyType) return { pattern: anyType[1], types: [] };
  const list = f.match(/^(.*)\.\{([a-z0-9]+(?:\s*,\s*[a-z0-9]+)*)\}$/i);
  const single = f.match(/^(.+)\.([a-z0-9]+)$/i);
  const parts = list
    ? { pattern: list[1] || "*", types: list[2].split(",") }
    : single
      ? { pattern: single[1], types: [single[2]] }
      : null;
  if (!parts) return null;
  const types = parts.types.map((t) => t.trim().toLowerCase());
  // ".{EXT}" or ".{YYYY}" are markers, not a type list.
  if (types.some((t) => BUILTIN.includes(t.toUpperCase()))) return null;
  return { pattern: parts.pattern, types };
}

export function joinFilter({ pattern, types }: FilterParts): string {
  const p = pattern.trim() || "*";
  if (types.length === 0) return p === "*" ? "*" : `${p}.*`;
  if (types.length === 1) return `${p}.${types[0]}`;
  return `${p}.{${types.join(",")}}`;
}

/** Whether the filter captures a date ({YYYY}, {MM}, {DD}...). */
export function hasDate(filter: string): boolean {
  return /\{(YYYY|YY|MM|MES|DD)\}/i.test(filter);
}

/** Names of {TEXTO:x} / {NUM:x} captures, usable as {x} in the destination. */
export function customMarkers(filter: string): string[] {
  const names = [...filter.matchAll(/\{(?:TEXTO|NUM):([^{}]+)\}/gi)].map((m) => m[1]);
  return [...new Set(names)];
}

/** A custom marker name: letters, digits and _, not a built-in marker. */
export function validMarkerName(name: string): boolean {
  return /^[\p{L}\p{N}_]+$/u.test(name) && !BUILTIN.includes(name.toUpperCase());
}

/** The path separator a folder uses: "\\" for Windows paths, "/" otherwise. */
export function separatorOf(path: string): string {
  return /^[a-z]:\\|^\\\\/i.test(path) || (path.includes("\\") && !path.includes("/"))
    ? "\\"
    : "/";
}

/** The destination after its fixed base folder: "D:\\J\\{YYYY}\\{MM}" -> "{YYYY}\\{MM}". */
export function destinationTail(destination: string): string {
  const marker = destination.indexOf("{");
  if (marker < 0) return "";
  const before = destination.slice(0, marker);
  const sep = Math.max(before.lastIndexOf("/"), before.lastIndexOf("\\"));
  return destination.slice(sep + 1);
}

export type PieceKind = "texto" | "num" | "data";

/** A piece of a sample file name chosen by the user. */
export interface NamePiece {
  start: number;
  end: number;
  kind: PieceKind;
  /** Marker name for "texto" and "num". */
  name: string;
}

/** The date markers for a selected date such as "20261022", "2026-10-22" or
 *  "22.10.2026"; null if the text is not a date. */
export function dateMarkers(text: string): string | null {
  const ymd = text.match(/^(\d{4})(\D?)(\d{2})\2(\d{2})$/);
  if (ymd) return `{YYYY}${ymd[2]}{MM}${ymd[2]}{DD}`;
  const dmy = text.match(/^(\d{2})(\D?)(\d{2})\2(\d{4})$/);
  if (dmy) return `{DD}${dmy[2]}{MM}${dmy[2]}{YYYY}`;
  return null;
}

/** Turns a sample name and the chosen pieces into a name pattern. Chosen
 *  pieces become markers; in the rest, separators stay and words/numbers
 *  become "*", so other files of the same kind match too.
 *  "Acme_20261022" with "Acme" as cliente -> "{TEXTO:cliente}_*". */
export function patternFromSample(stem: string, pieces: NamePiece[]): string {
  let out = "";
  let at = 0;
  for (const piece of [...pieces].sort((a, b) => a.start - b.start)) {
    out += generalize(stem.slice(at, piece.start));
    const text = stem.slice(piece.start, piece.end);
    out +=
      piece.kind === "data"
        ? (dateMarkers(text) ?? "*")
        : piece.kind === "num"
          ? `{NUM:${piece.name}}`
          : `{TEXTO:${piece.name}}`;
    at = piece.end;
  }
  out += generalize(stem.slice(at));
  return out.replace(/\*{2,}/g, "*") || "*";
}

function generalize(text: string): string {
  return text.replace(/[\p{L}\p{N}]+/gu, "*").replace(/[{}|*]+/g, "*");
}

/** "pasta/sub/Acme_1.pdf" -> { stem: "Acme_1", ext: "pdf" }. */
export function splitName(path: string): { stem: string; ext: string } {
  const name = path.split(/[\\/]/).at(-1) ?? path;
  const dot = name.lastIndexOf(".");
  return dot > 0 ? { stem: name.slice(0, dot), ext: name.slice(dot + 1) } : { stem: name, ext: "" };
}
