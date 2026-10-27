<script lang="ts">
  // "Testar regra": a dry run on the rule's folder. Nothing on disk changes.
  import * as api from "$lib/api";
  import { outcomeText } from "$lib/format";
  import { t } from "$lib/i18n";
  import type { Rule, TestRow } from "$lib/types";
  import Modal from "./Modal.svelte";

  let { rule, onclose }: { rule: Rule | null; onclose: () => void } = $props();

  let rows = $state<TestRow[] | null>(null);
  let error = $state<string | null>(null);

  $effect(() => {
    if (!rule) return;
    const current = rule;
    rows = null;
    error = null;
    api
      .testRule(current)
      .then((result) => {
        if (rule === current) rows = result;
      })
      .catch((e) => {
        if (rule === current) error = String(e);
      });
  });

  const organized = $derived(
    rows?.filter((r) => r.outcome.kind === "would_move" || r.outcome.kind === "would_copy")
      .length ?? 0,
  );
</script>

<Modal open={rule !== null} title={t("test.title", { name: rule?.name ?? "" })} {onclose} wide>
  {#if error}
    <p class="error" role="alert">{error}</p>
  {:else if rows === null}
    <p aria-live="polite">{t("test.running")}</p>
  {:else if rows.length === 0}
    <p>{t("test.noFiles")}</p>
  {:else}
    <p>{t("test.intro")} <strong>{t("test.summary", { count: organized, total: rows.length })}</strong></p>
    <ul>
      {#each rows as row (row.relativePath)}
        <li class:match={row.outcome.kind === "would_move" || row.outcome.kind === "would_copy"}>
          <span class="file">{row.relativePath}</span>
          <span class="result">{outcomeText(row.outcome)}</span>
        </li>
      {/each}
    </ul>
  {/if}
  {#snippet actions()}
    <button class="primary" data-autofocus onclick={onclose}>{t("dialog.close")}</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0 0 12px;
  }
  .error {
    color: var(--danger-text);
    background: var(--danger-bg);
    padding: 12px;
    border-radius: 8px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  li + li {
    border-top: 1px solid var(--border);
  }
  li.match {
    color: var(--text);
  }
  .file {
    font-weight: 600;
  }
</style>
