<script lang="ts">
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { TestRow } from "$lib/types";
  let { rows, testing, testError, onTest }: { rows: TestRow[]; testing: boolean; testError: string; onTest: () => void } = $props();
  const moveCount = $derived(rows.filter((row) => row.outcome.kind === "wouldMove" || row.outcome.kind === "moved").length);
  const copyCount = $derived(rows.filter((row) => row.outcome.kind === "wouldCopy" || row.outcome.kind === "copied").length);
  const ignoredCount = $derived(rows.filter((row) => row.outcome.kind === "skipped" || row.outcome.kind === "notMatched").length);
  const failedCount = $derived(rows.filter((row) => row.outcome.kind === "failed").length);
  function result(row: TestRow): string {
    switch (row.outcome.kind) {
      case "wouldMove": case "wouldCopy": case "moved": case "copied": return row.outcome.to;
      case "skipped": return row.outcome.reason;
      case "failed": return row.outcome.error;
      case "notMatched": return ptBR.notMatchedReason;
    }
  }
</script>

<div class="step-stack review">
  <div class="summary" aria-live="polite">
    {#if moveCount}<span class="ok"><strong>{moveCount}</strong> {ptBR.filesWillMove}</span>{/if}
    {#if copyCount}<span class="ok"><strong>{copyCount}</strong> {ptBR.filesWillCopy}</span>{/if}
    {#if ignoredCount}<span><strong>{ignoredCount}</strong> {ptBR.filesIgnored}</span>{/if}
    {#if failedCount}<span class="bad"><strong>{failedCount}</strong> {ptBR.filesFailed}</span>{/if}
  </div>
  <button class="picker" type="button" disabled={testing} onclick={onTest}>{testing ? ptBR.testingRule : ptBR.testAgain}</button>
  {#if testError}<small class="field-error">{testError}</small>{/if}
  {#if !testing && rows.length === 0}<p class="empty-test">{ptBR.noTestFiles}</p>{/if}
  {#if rows.length}<div class="table-wrap"><table><thead><tr><th>{ptBR.testFileColumn}</th><th>{ptBR.testResultColumn}</th></tr></thead><tbody>{#each rows as row}<tr><td>{row.relativePath}</td><td class:failed={row.outcome.kind === "failed"}>{result(row)}</td></tr>{/each}</tbody></table></div>{/if}
</div>

<style>
  .summary { display: flex; flex-wrap: wrap; gap: .5rem; } .summary span { padding: .45rem .65rem; border-radius: .55rem; color: var(--text-soft); background: var(--surface-raised); font-size: .86rem; } .summary .ok { color: var(--accent-strong); background: var(--accent-soft); } .summary .bad { color: var(--danger); }
  .table-wrap { max-height: 300px; overflow: auto; border: 1px solid var(--border); border-radius: .7rem; } table { width: 100%; border-collapse: collapse; font-size: .82rem; } th, td { padding: .65rem .7rem; border-bottom: 1px solid var(--border); text-align: left; overflow-wrap: anywhere; } th { position: sticky; top: 0; color: var(--text-soft); background: var(--surface-raised); } td:first-child { width: 38%; font-weight: 650; } td.failed { color: var(--danger); } .empty-test { padding: 2rem; color: var(--text-soft); text-align: center; }
</style>
