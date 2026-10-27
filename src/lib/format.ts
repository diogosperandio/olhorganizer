// Turns rule data into words a non-technical user understands.

import { locale, t, type MessageKey } from "./i18n";
import type { Outcome } from "./types";

/** Last folder name of a path, Windows or Linux: "D:\\Docs\\NEWS" -> "NEWS". */
export function folderName(path: string): string {
  const parts = path.split(/[\\/]+/).filter(Boolean);
  return parts.at(-1) ?? path;
}

export type DestinationPart = { kind: "text"; text: string } | { kind: "marker"; label: string };

/** "D:\\Jornais\\{YYYY}\\{MM}" -> text "D:\\Jornais\\", marker "Ano", text "\\", marker "Mês". */
export function destinationParts(destination: string): DestinationPart[] {
  const parts: DestinationPart[] = [];
  let last = 0;
  for (const match of destination.matchAll(/\{([^{}]*)\}/g)) {
    if (match.index > last) {
      parts.push({ kind: "text", text: destination.slice(last, match.index) });
    }
    parts.push({ kind: "marker", label: markerLabel(match[1]) });
    last = match.index + match[0].length;
  }
  if (last < destination.length) {
    parts.push({ kind: "text", text: destination.slice(last) });
  }
  return parts;
}

/** Built-in markers get a friendly name; custom captures keep theirs. */
function markerLabel(token: string): string {
  const key = `marker.${token.toUpperCase()}`;
  return key in markerKeys ? t(key as MessageKey) : token;
}

const markerKeys = {
  "marker.YYYY": true,
  "marker.YY": true,
  "marker.MM": true,
  "marker.MES": true,
  "marker.DD": true,
  "marker.NOME": true,
  "marker.EXT": true,
};

/** File types a filter accepts, upper-cased: "*.{pdf,jpg}" -> ["PDF", "JPG"].
 *  Empty when the filter accepts any type. */
export function fileTypes(filter: string): string[] {
  const list = filter.match(/\.\{([^{}:]*,[^{}:]*|[a-z0-9]+)\}\s*$/i);
  const single = filter.match(/\.([a-z0-9]+)\s*$/i);
  const raw = list ? list[1].split(",") : single ? [single[1]] : [];
  const types = raw.map((e) => e.trim().toUpperCase()).filter(Boolean);
  // A list made only of markers such as {EXT} is not a type list.
  if (list && types.every((ty) => `marker.${ty}` in markerKeys)) return [];
  return [...new Set(types)];
}

const rtf = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });

/** "agora", "há 3 minutos", "ontem"... */
export function relativeTime(at: number, now: number): string {
  const seconds = Math.round((at - now) / 1000);
  const abs = Math.abs(seconds);
  if (abs < 45) return rtf.format(0, "second");
  if (abs < 60 * 60) return rtf.format(Math.round(seconds / 60), "minute");
  if (abs < 60 * 60 * 24) return rtf.format(Math.round(seconds / 3600), "hour");
  return rtf.format(Math.round(seconds / 86400), "day");
}

/** One line describing a dry-run (or real) result. */
export function outcomeText(outcome: Outcome): string {
  switch (outcome.kind) {
    case "would_move":
      return t("test.wouldMove", { to: outcome.to });
    case "would_copy":
      return t("test.wouldCopy", { to: outcome.to });
    case "moved":
      return t("test.moved", { to: outcome.to });
    case "copied":
      return t("test.copied", { to: outcome.to });
    case "already_copied":
      return t("test.alreadyCopied", { to: outcome.to });
    case "skipped":
      return t("test.skipped", { reason: outcome.reason });
    case "not_matched":
      return t("test.notMatched");
    case "failed":
      return t("test.failed", { error: outcome.error });
  }
}
