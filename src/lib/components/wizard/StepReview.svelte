<script lang="ts">
  // Final step: "Testar regra" on the draft (dry run, nothing changes on disk).
  import * as api from "$lib/api";
  import { outcomeText } from "$lib/format";
  import { t, type MessageKey } from "$lib/i18n";
  import type { Rule, TestRow } from "$lib/types";

  let { draft }: { draft: Rule } = $props();

  let rows = $state<TestRow[] | null>(null);
  let testedJson = $state("");
  let running = $state(false);
  let error = $state<string | null>(null);

  const draftJson = $derived(JSON.stringify(draft));
  const outdated = $derived(rows !== null && testedJson !== draftJson);

  async function run() {
    running = true;
    error = null;
    const snapshot = $state.snapshot(draft);
    try {
      rows = await api.testRule(snapshot);
      testedJson = JSON.stringify(snapshot);
    } catch (e) {
      error = String(e);
      rows = null;
    } finally {
      running = false;
    }
  }

  function organizes(row: TestRow) {
    return row.outcome.kind === "would_move" || row.outcome.kind === "would_copy";
  }

  function count(n: number, one: MessageKey, many: MessageKey) {
    return n === 1 ? t(one) : t(many, { count: n });
  }

  const summary = $derived.by(() => {
    if (!rows) return "";
    const done = rows.filter(organizes).length;
    const failed = rows.filter((r) => r.outcome.kind === "failed").length;
    const ignored = rows.length - done - failed;
    const parts = [
      draft.action === "copy"
        ? count(done, "review.copyOne", "review.copyMany")
        : count(done, "review.moveOne", "review.moveMany"),
      count(ignored, "review.ignoredOne", "review.ignoredMany"),
    ];
    if (failed > 0) parts.push(count(failed, "review.problemsOne", "review.problemsMany"));
    return parts.join(", ") + ".";
  });
</script>

<div class="step">
  <p>{t("review.intro")}</p>
  <div class="run">
    <button class="primary" onclick={run} disabled={running}>
      {rows ? t("review.rerun") : t("review.run")}
    </button>
    {#if running}<span aria-live="polite">{t("test.running")}</span>{/if}
    {#if outdated && !running}<span class="outdated">{t("review.outdated")}</span>{/if}
  </div>

  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if rows}
    {#if rows.length === 0}
      <p>{t("test.noFiles")}</p>
    {:else}
      <p class="summary" aria-live="polite"><strong>{summary}</strong></p>
      <div class="table">
        <table>
          <thead>
            <tr><th scope="col">{t("review.file")}</th><th scope="col">{t("review.result")}</th></tr>
          </thead>
          <tbody>
            {#each rows as row (row.relativePath)}
              <tr class:match={organizes(row)}>
                <td>{row.relativePath}</td>
                <td>{outcomeText(row.outcome)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
  {/if}
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }
  p {
    margin: 0;
  }
  .run {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 12px;
  }
  .outdated {
    color: var(--warn-text);
    background: var(--warn-bg);
    padding: 4px 10px;
    border-radius: 6px;
  }
  .error {
    color: var(--danger-text);
    background: var(--danger-bg);
    padding: 10px 12px;
    border-radius: 8px;
  }
  .table {
    max-height: 320px;
    overflow: auto;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: 8px 12px;
    vertical-align: top;
    overflow-wrap: anywhere;
  }
  th {
    position: sticky;
    top: 0;
    background: var(--surface-raised);
    border-bottom: 1px solid var(--border);
  }
  tr + tr td {
    border-top: 1px solid var(--border);
  }
  tbody tr {
    color: var(--muted);
  }
  tbody tr.match {
    color: var(--text);
  }
</style>
