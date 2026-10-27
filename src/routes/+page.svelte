<script lang="ts">
  // Main screen: the user's rules as cards, updated live from the backend.
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import * as api from "$lib/api";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import RuleList from "$lib/components/RuleList.svelte";
  import TestDialog from "$lib/components/TestDialog.svelte";
  import { t } from "$lib/i18n";
  import type { Rule, RuleStatus, Uuid } from "$lib/types";

  let rules = $state<Rule[]>([]);
  let statuses = $state<Record<Uuid, RuleStatus>>({});
  let loading = $state(true);
  let notice = $state<string | null>(null);
  let now = $state(Date.now());
  let toDelete = $state<Rule | null>(null);
  let toTest = $state<Rule | null>(null);

  function fail(error: unknown) {
    notice = String(error);
  }

  async function loadRules() {
    rules = await api.listRules();
  }

  async function loadStatuses() {
    const list = await api.getRuleStatuses();
    statuses = Object.fromEntries(list.map((s) => [s.ruleId, s]));
  }

  // Events can arrive in bursts (a folder with many files): refresh once.
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;
  function onRuleEvent() {
    now = Date.now();
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(() => loadStatuses().catch(fail), 150);
  }

  onMount(() => {
    Promise.all([loadRules(), loadStatuses()])
      .catch(fail)
      .finally(() => (loading = false));

    const listeners = [
      api.onFileProcessed(onRuleEvent),
      api.onRuleError(onRuleEvent),
      api.onRulePaused(onRuleEvent),
      api.onRuleResumed(onRuleEvent),
    ];
    const clock = setInterval(() => (now = Date.now()), 30_000);
    return () => {
      clearInterval(clock);
      clearTimeout(refreshTimer);
      listeners.forEach((l) => l.then((stop) => stop()));
    };
  });

  const newRule = () => goto("/regras/editar");
  const editRule = (rule: Rule) => goto(`/regras/editar?id=${rule.id}`);

  async function toggle(rule: Rule, enabled: boolean) {
    try {
      const updated = await api.toggleRule(rule.id!, enabled);
      rules = rules.map((r) => (r.id === updated.id ? updated : r));
      await loadStatuses();
    } catch (e) {
      fail(e);
    }
  }

  function reorder(ids: Uuid[]) {
    // Show the new order at once; the backend confirms (or we reload).
    const byId = new Map(rules.map((r) => [r.id, r]));
    rules = ids.map((id) => byId.get(id)).filter((r): r is Rule => r !== undefined);
    api
      .reorderRules(ids)
      .then((saved) => (rules = saved))
      .catch((e) => {
        fail(e);
        loadRules().catch(fail);
      });
  }

  async function duplicate(rule: Rule) {
    try {
      // The copy starts switched off, so it never competes with the original.
      await api.saveRule({
        ...rule,
        id: null,
        name: t("card.copyName", { name: rule.name }),
        enabled: false,
      });
      await loadRules();
    } catch (e) {
      fail(e);
    }
  }

  async function confirmDelete() {
    const rule = toDelete;
    toDelete = null;
    if (!rule?.id) return;
    try {
      await api.deleteRule(rule.id);
      await loadRules();
    } catch (e) {
      fail(e);
    }
  }

  async function resume(rule: Rule) {
    try {
      await api.resumeRule(rule.id!);
      await loadStatuses();
    } catch (e) {
      fail(e);
    }
  }
</script>

<header class="top">
  <h1>{t("app.name")}</h1>
  <nav>
    <a class="button" href="/historico"><Icon name="history" />{t("nav.history")}</a>
    <a class="button" href="/configuracoes"><Icon name="settings" />{t("nav.settings")}</a>
  </nav>
</header>

<main>
  {#if notice}
    <div class="notice" role="alert">
      <span>{notice}</span>
      <button class="close" aria-label={t("notice.dismiss")} onclick={() => (notice = null)}>×</button>
    </div>
  {/if}

  {#if loading}
    <p class="loading" aria-live="polite">{t("rules.loading")}</p>
  {:else if rules.length === 0}
    <EmptyState oncreate={newRule} />
  {:else}
    <div class="new">
      <button class="primary big" onclick={newRule}><Icon name="plus" />{t("rules.new")}</button>
    </div>
    <RuleList
      {rules}
      {statuses}
      {now}
      onreorder={reorder}
      ontoggle={toggle}
      onedit={editRule}
      ontest={(rule) => (toTest = rule)}
      onduplicate={duplicate}
      ondelete={(rule) => (toDelete = rule)}
      onresume={resume}
    />
  {/if}
</main>

<ConfirmDialog
  open={toDelete !== null}
  title={t("delete.title")}
  message={t("delete.message", { name: toDelete?.name ?? "" })}
  confirmLabel={t("delete.confirm")}
  danger
  onconfirm={confirmDelete}
  oncancel={() => (toDelete = null)}
/>

<TestDialog rule={toTest} onclose={() => (toTest = null)} />

<style>
  .top {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 24px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0;
    font-size: 1.35rem;
  }
  nav {
    display: flex;
    gap: 8px;
  }
  main {
    max-width: 860px;
    margin: 0 auto;
    padding: 20px 24px 40px;
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .new {
    display: flex;
  }
  .big {
    min-height: 48px;
    padding: 0 24px;
    font-size: 1.05rem;
  }
  .loading {
    color: var(--muted);
  }
  .notice {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 8px 8px 16px;
    border-radius: 10px;
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .close {
    width: 40px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font-size: 1.4rem;
  }
</style>
