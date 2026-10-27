export type CustomMarkerKind = "TEXTO" | "NUM";

export interface CustomMarker {
  name: string;
  kind: CustomMarkerKind;
  token: string;
}

export const knownFileTypes = ["pdf", "jpg", "png", "docx", "xlsx", "txt"] as const;

export function fileNameFromPath(path: string): string {
  return path.split(/[\\/]/).filter(Boolean).at(-1) || path;
}

export function folderNameFromPath(path: string): string {
  return fileNameFromPath(path);
}

export function typesFromFilter(filter: string): string[] {
  const group = /\.\{([^}]+)\}$/.exec(filter)?.[1];
  const single = /\.([a-zA-Z0-9]+)$/.exec(filter)?.[1];
  const values = group ? group.split(",") : single ? [single] : [];
  return [...new Set(values.map((value) => value.toLowerCase() === "jpeg" ? "jpg" : value.toLowerCase()))];
}

export function filterWithTypes(filter: string, types: string[]): string {
  const base = filter.replace(/\.(?:\{[^}]*\}|[a-zA-Z0-9]+|\*)$/, "");
  const extensions = types;
  if (extensions.length === 0) return base || "*";
  return `${base || "*"}.${extensions.length === 1 ? extensions[0] : `{${extensions.join(",")}}`}`;
}

export function customMarkersFromFilter(filter: string): CustomMarker[] {
  return [...filter.matchAll(/\{(TEXTO|NUM):([a-zA-Z][a-zA-Z0-9_-]*)\}/g)].map((match) => ({
    kind: match[1] as CustomMarkerKind,
    name: match[2],
    token: `{${match[2]}}`,
  }));
}
