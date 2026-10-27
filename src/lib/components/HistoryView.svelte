<script lang="ts">
  // "Histórico": what happened since the app opened (the backend keeps the last
  // 500 events), newest first, with filters. Rows are plain objects in a raw
  // array, so a new event adds one row instead of re-rendering the table.
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import { locale, t, type MessageKey } from "$lib/i18n";
  import type { HistoryEntry } from "$lib/types";
  import PageHeader from "./PageHeader.svelte";
  import Toast from "./Toast.svelte";

  type Result = "moved" | "copied" | "ignored" | "error";

  interface Row {
    key: number;
    at: number;
    rule: string;
    /** Relative to the rule's folder; null for rule-level problems. */
    file: string | null;
    /** Destination folder, for moved/copied files. */
    destination: string | null;
    result: Result;
    /** Why it was ignored or failed, in pt-BR. */
    detail: string | null;
    /** What "Abrir pasta" opens. */
    folder: string | null;
  }

  const LIMIT = 500;
  const RESULTS: Result[] = ["moved", "copied", "ignored", "error"];
  const RESULT_KEY: Record<Result, MessageKey> = {
    moved: "result.moved",
    copied: "result.copied",
    ignored: "result.ignored",
    error: "result.error",
  };

  let rows = $state.raw<Row[]>([]);
  let ruleFilter = $state("");
  let resultFilter = $state<Result | "">("");
  let search = $state("");
  let problem = $state<string | null>(null);
  let nextKey = 0;

  const when = new Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "medium" });

  function parentOf(path: string): string {
    const cut = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    return cut > 0 ? path.slice(0, cut) : path;
  }

  function toRow(entry: HistoryEntry): Row | null {
    const base = { key: nextKey++, at: entry.at, rule: entry.ruleName };
    switch (entry.type) {
      case "file-processed": {
        const o = entry.outcome;
        const to = "to" in o ? o.to : null;
        const result: Result =
          o.kind === "moved" ? "moved" : o.kind === "copied" ? "copied" : o.kind === "failed" ? "error" : "ignored";
        return {
          ...base,
          file: entry.relativePath,
          destination: (result === "moved" || result === "copied") && to ? parentOf(to) : null,
          result,
          detail: o.kind === "failed" ? o.error : o.kind === "skipped" ? o.reason : null,
          folder: to ?? entry.file,
        };
      }
      case "rule-error":
      case "rule-paused":
        return { ...base, file: null, destination: null, result: "error", detail: entry.messagePt, folder: null };
      default:
        return null;
    }
  }

  function prepend(entry: HistoryEntry) {
    const row = toRow(entry);
    if (row) rows = [row, ...rows].slice(0, LIMIT);
  }

  onMount(() => {
    api
      .getHistory(LIMIT)
      .then((entries) => {
        // Live events may have arrived meanwhile; keep them on top.
        const loaded = entries.map(toRow).filter((r): r is Row => r !== null);
        rows = [...rows, ...loaded].slice(0, LIMIT);
      })
      .catch((e) => (problem = String(e)));

    const listeners = [
      api.onFileProcessed((e) => prepend({ type: "file-processed", ...e })),
      api.onRuleError((e) => prepend({ type: "rule-error", ...e })),
      api.onRulePaused((e) => prepend({ type: "rule-paused", ...e })),
    ];
    return () => listeners.forEach((l) => l.then((stop) => stop()));
  });

  const ruleNames = $derived([...new Set(rows.map((r) => r.rule))].sort((a, b) => a.localeCompare(b, locale)));

  const shown = $derived.by(() => {
    const needle = search.trim().toLocaleLowerCase(locale);
    return rows.filter(
      (r) =>
        (!ruleFilter || r.rule === ruleFilter) &&
        (!resultFilter || r.result === resultFilter) &&
        (!needle || (r.file ?? "").toLocaleLowerCase(locale).includes(needle)),
    );
  });

  async function openFolder(path: string) {
    try {
      await api.openFolder(path);
    } catch (e) {
      problem = String(e);
    }
  }

  async function openLog() {
    try {
      await api.openLog();
    } catch (e) {
      problem = String(e);
    }
  }
</script>

<PageHeader title={t("history.title")}>
  {#snippet actions()}
    <button onclick={openLog}>{t("history.openLog")}</button>
  {/snippet}
</PageHeader>

<main>
  <div class="filters">
    <div class="field">
      <label for="f-rule">{t("history.filterRule")}</label>
      <select id="f-rule" bind:value={ruleFilter}>
        <option value="">{t("history.all")}</option>
        {#each ruleNames as name (name)}<option value={name}>{name}</option>{/each}
      </select>
    </div>
    <div class="field">
      <label for="f-result">{t("history.filterResult")}</label>
      <select id="f-result" bind:value={resultFilter}>
        <option value="">{t("history.allResults")}</option>
        {#each RESULTS as r (r)}<option value={r}>{t(RESULT_KEY[r])}</option>{/each}
      </select>
    </div>
    <div class="field grow">
      <label for="f-search">{t("history.search")}</label>
      <input id="f-search" type="text" bind:value={search} placeholder={t("history.searchPlaceholder")} />
    </div>
  </div>

  <p class="help">
    {t("history.count", { shown: shown.length, total: rows.length })} · {t("history.keep")}
  </p>

  {#if rows.length === 0}
    <p class="empty">{t("history.empty")}</p>
  {:else if shown.length === 0}
    <p class="empty">{t("history.noMatch")}</p>
  {:else}
    <div class="table">
      <table>
        <thead>
          <tr>
            <th scope="col">{t("history.when")}</th>
            <th scope="col">{t("history.file")}</th>
            <th scope="col">{t("history.rule")}</th>
            <th scope="col">{t("history.destination")}</th>
            <th scope="col">{t("history.result")}</th>
            <th scope="col"><span class="visually-hidden">{t("history.actions")}</span></th>
          </tr>
        </thead>
        <tbody>
          {#each shown as row (row.key)}
            <tr>
              <td class="when">{when.format(row.at)}</td>
              <td class="file">{row.file ?? "—"}</td>
              <td>{row.rule}</td>
              <td class="path">{row.destination ?? "—"}</td>
              <td>
                <span class="label {row.result}">{t(RESULT_KEY[row.result])}</span>
                {#if row.detail}<p class="detail">{row.detail}</p>{/if}
              </td>
              <td>
                {#if row.folder}
                  <button
                    class="small"
                    aria-label={t("history.openFolderOf", { file: row.file ?? "" })}
                    onclick={() => row.folder && openFolder(row.folder)}
                  >
                    {t("history.openFolder")}
                  </button>
                {/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</main>

<Toast message={problem} error onclose={() => (problem = null)} />

<style>
  main {
    max-width: 1100px;
    margin: 0 auto;
    padding: 16px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    align-items: flex-end;
  }
  .grow {
    flex: 1;
    min-width: 200px;
  }
  .empty {
    padding: 32px;
    text-align: center;
    color: var(--muted);
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .table {
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    vertical-align: top;
    padding: 8px 10px;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
    font-size: 0.9rem;
  }
  tbody tr + tr td {
    border-top: 1px solid var(--border);
  }
  .when {
    white-space: nowrap;
    color: var(--muted);
    font-size: 0.9rem;
  }
  .file,
  .path {
    overflow-wrap: anywhere;
  }
  .file {
    font-weight: 600;
  }
  .path {
    color: var(--muted);
    font-size: 0.9rem;
  }
  .label {
    display: inline-block;
    padding: 2px 10px;
    border-radius: 999px;
    font-size: 0.85rem;
    font-weight: 700;
  }
  .label.moved,
  .label.copied {
    background: var(--ok-bg);
    color: var(--ok-text);
  }
  .label.ignored {
    background: var(--badge-bg);
    color: var(--badge-text);
  }
  .label.error {
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .detail {
    margin: 4px 0 0;
    font-size: 0.88rem;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .small {
    white-space: nowrap;
  }
</style>
