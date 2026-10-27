<script lang="ts">
  import { onMount } from "svelte";
  import { deleteRule, getHistory, listRules, onFileProcessed, onRuleError, onRulePaused, reorderRules, saveRule, testRule, toggleRule } from "$lib/api";
  import ConfirmDialog from "$lib/components/ConfirmDialog.svelte";
  import EmptyState from "$lib/components/EmptyState.svelte";
  import HistoryView from "$lib/components/HistoryView.svelte";
  import RuleList from "$lib/components/RuleList.svelte";
  import SettingsView from "$lib/components/SettingsView.svelte";
  import RuleWizard from "$lib/components/wizard/RuleWizard.svelte";
  import type { CardAction, RuleActivity } from "$lib/components/RuleCard.svelte";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { AppEvent, FileProcessedPayload, HistoryEntry, Rule } from "$lib/types";

  let rules = $state<Rule[]>([]);
  let activities = $state<Record<string, RuleActivity>>({});
  let busyIds = $state(new Set<string>());
  let loading = $state(true);
  let loadFailed = $state(false);
  let notice = $state("");
  let deleteCandidate = $state<Rule | null>(null);
  let wizardOpen = $state(false);
  let wizardRule = $state<Rule | undefined>(undefined);
  let view = $state<"rules" | "history" | "settings">("rules");
  const sortedRules = $derived([...rules].sort((a, b) => a.priority - b.priority));
  const currentPageTitle = $derived(view === "history" ? ptBR.historyPageTitle : view === "settings" ? ptBR.settingsPageTitle : ptBR.pageTitle);

  function setBusy(id: string, busy: boolean) {
    const next = new Set(busyIds); if (busy) next.add(id); else next.delete(id); busyIds = next;
  }
  function showNotice(message: string) { notice = message; }
  function openWizard(rule?: Rule) { wizardRule = rule; wizardOpen = true; }
  function wizardSaved(saved: Rule, message: string) {
    rules = rules.some((rule) => rule.id === saved.id) ? rules.map((rule) => rule.id === saved.id ? saved : rule) : [...rules, saved];
    wizardOpen = false; wizardRule = undefined; showNotice(message);
  }
  function applyFileProcessed(payload: FileProcessedPayload, timestamp = Date.now()) {
    if (payload.outcome.kind === "notMatched") return;
    const failed = payload.outcome.kind === "failed";
    const message = payload.outcome.kind === "failed" ? payload.outcome.error : undefined;
    activities = { ...activities, [payload.ruleId]: { status: failed ? "error" : "ok", message, lastFile: payload.file, lastProcessedAt: timestamp } };
  }
  function applyEvent(event: AppEvent, timestamp = Date.now()) {
    if (event.name === "file-processed") applyFileProcessed(event.payload, timestamp);
    else if (event.name === "rule-error") activities = { ...activities, [event.payload.ruleId]: { ...activities[event.payload.ruleId], status: "error", message: event.payload.messagePt } };
    else activities = { ...activities, [event.payload.ruleId]: { ...activities[event.payload.ruleId], status: "paused", message: event.payload.reasonPt } };
  }
  function loadActivity(history: HistoryEntry[]) { for (const entry of [...history].reverse()) applyEvent(entry.event, Number(entry.timestampMs)); }

  async function load() {
    loading = true; loadFailed = false;
    try { const [loadedRules, history] = await Promise.all([listRules(), getHistory(500)]); rules = loadedRules; loadActivity(history); }
    catch { loadFailed = true; }
    finally { loading = false; }
  }
  async function changeEnabled(rule: Rule, enabled: boolean) {
    setBusy(rule.id, true);
    try {
      const updated = await toggleRule(rule.id, enabled); rules = rules.map((item) => item.id === updated.id ? updated : item);
      activities = { ...activities, [rule.id]: { ...activities[rule.id], status: enabled ? "ok" : "paused", message: enabled ? undefined : ptBR.statusDisabled } };
    } catch (error) { showNotice(`${ptBR.actionError} ${String(error)}`); }
    finally { setBusy(rule.id, false); }
  }
  async function resumeRule(rule: Rule) {
    setBusy(rule.id, true);
    try {
      await toggleRule(rule.id, false); const updated = await toggleRule(rule.id, true);
      rules = rules.map((item) => item.id === updated.id ? updated : item);
      activities = { ...activities, [rule.id]: { ...activities[rule.id], status: "ok", message: undefined } };
      showNotice(ptBR.ruleResumed);
    } catch (error) { showNotice(`${ptBR.actionError} ${String(error)}`); await load(); }
    finally { setBusy(rule.id, false); }
  }
  async function duplicateRule(rule: Rule) {
    setBusy(rule.id, true);
    try {
      const copy: Rule = { ...rule, id: crypto.randomUUID(), name: `${rule.name} (${ptBR.duplicateSuffix})`, enabled: false, priority: Math.max(-1, ...rules.map((item) => item.priority)) + 1 };
      rules = [...rules, await saveRule(copy)]; showNotice(ptBR.ruleDuplicated);
    } catch (error) { showNotice(`${ptBR.actionError} ${String(error)}`); }
    finally { setBusy(rule.id, false); }
  }
  async function runTest(rule: Rule) {
    setBusy(rule.id, true); showNotice(ptBR.testStarted);
    try { const rows = await testRule(rule); showNotice(`${ptBR.testFinishedPrefix} ${rows.length} ${ptBR.testFinishedSuffix}`); }
    catch (error) { showNotice(`${ptBR.actionError} ${String(error)}`); }
    finally { setBusy(rule.id, false); }
  }
  function handleAction(rule: Rule, action: CardAction) {
    if (action === "delete") deleteCandidate = rule;
    else if (action === "test") void runTest(rule);
    else if (action === "duplicate") void duplicateRule(rule);
    else openWizard(rule);
  }
  async function confirmDelete() {
    const rule = deleteCandidate; deleteCandidate = null; if (!rule) return; setBusy(rule.id, true);
    try { await deleteRule(rule.id); rules = rules.filter((item) => item.id !== rule.id); showNotice(ptBR.ruleDeleted); }
    catch (error) { showNotice(`${ptBR.actionError} ${String(error)}`); }
    finally { setBusy(rule.id, false); }
  }
  async function reorder(ids: string[]) {
    const previous = rules; const positions = new Map(ids.map((id, index) => [id, index]));
    rules = [...rules].sort((a, b) => (positions.get(a.id) ?? 0) - (positions.get(b.id) ?? 0)).map((rule, priority) => ({ ...rule, priority }));
    try { rules = await reorderRules(ids); showNotice(ptBR.orderSaved); }
    catch (error) { rules = previous; showNotice(`${ptBR.actionError} ${String(error)}`); }
  }

  onMount(() => {
    const subscriptions = [onFileProcessed(applyFileProcessed), onRuleError((payload) => applyEvent({ name: "rule-error", payload })), onRulePaused((payload) => applyEvent({ name: "rule-paused", payload }))];
    void load();
    return () => { for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten()); };
  });
</script>

<svelte:head><title>{currentPageTitle}</title></svelte:head>

<header><div class="header-inner">
  <button class="brand" type="button" aria-label={ptBR.rulesNav} onclick={() => (view = "rules")}><span aria-hidden="true">O</span>{ptBR.appName}</button>
  <nav aria-label={ptBR.appName}><button class:active={view === "rules"} type="button" onclick={() => (view = "rules")}>{ptBR.rulesNav}</button><button class:active={view === "history"} type="button" onclick={() => (view = "history")}>{ptBR.history}</button><button class:active={view === "settings"} type="button" onclick={() => (view = "settings")}>{ptBR.settings}</button></nav>
</div></header>

<main>
  {#if view === "rules"}
  <section class="intro"><div><h1>{ptBR.rulesHeading}</h1><p>{ptBR.rulesSummary}</p></div><button class="primary" type="button" onclick={() => openWizard()}>{ptBR.newRule}</button></section>
  {#if loading}<div class="state-message" aria-live="polite">{ptBR.loadingRules}</div>
  {:else if loadFailed}<div class="state-message error"><p>{ptBR.loadError}</p><button type="button" onclick={load}>{ptBR.tryAgain}</button></div>
  {:else if sortedRules.length === 0}<EmptyState onNew={() => openWizard()} />
  {:else}<RuleList rules={sortedRules} {activities} {busyIds} onToggle={changeEnabled} onResume={resumeRule} onAction={handleAction} onReorder={reorder} />{/if}
  {:else if view === "history"}<HistoryView rules={sortedRules} onNotice={showNotice} />
  {:else}<SettingsView onNotice={showNotice} onRulesImported={(imported) => (rules = imported)} />
  {/if}
</main>

{#if notice}<div class="toast" role="status"><span>{notice}</span><button type="button" aria-label={ptBR.closeMessage} onclick={() => (notice = "")}>×</button></div>{/if}
{#if deleteCandidate}<ConfirmDialog title={ptBR.confirmDelete} message={`${ptBR.deleteConfirmPrefix}${deleteCandidate.name}${ptBR.deleteConfirmSuffix}`} onConfirm={confirmDelete} onCancel={() => (deleteCandidate = null)} />{/if}
{#if wizardOpen}<RuleWizard rule={wizardRule} nextPriority={Math.max(-1, ...rules.map((rule) => rule.priority)) + 1} onClose={() => { wizardOpen = false; wizardRule = undefined; }} onSaved={wizardSaved} />{/if}

<style>
  :global(*) { box-sizing: border-box; }
  :global(:root) {
    font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; color: #17211c; background: #f4f7f5; font-synthesis: none;
    --text: #17211c; --text-soft: #516058; --text-faint: #76827c; --surface: #fff; --surface-raised: #f2f6f3; --border: #dce5df; --border-strong: #bdccc3;
    --accent: #267653; --accent-strong: #175e3f; --accent-soft: #e3f3ea; --danger: #b83b3b; --warning-text: #76520c; --warning-soft: #fff4d7;
    --status-ok: #2b9a61; --status-paused: #d29a25; --status-error: #d24b4b; --switch-off: #9aa59f; --tooltip-bg: #17211c; --tooltip-text: #fff;
    --shadow-sm: 0 1px 2px rgb(20 45 31 / .05); --shadow-md: 0 7px 20px rgb(20 45 31 / .08); --shadow-lg: 0 18px 48px rgb(10 25 17 / .2);
  }
  :global(body) { margin: 0; min-width: 320px; min-height: 100vh; color: var(--text); background: #f4f7f5; }
  :global(button:focus-visible), :global(a:focus-visible), :global([tabindex]:focus-visible) { outline: 3px solid color-mix(in srgb, var(--accent) 42%, transparent); outline-offset: 2px; }
  header { border-bottom: 1px solid var(--border); background: color-mix(in srgb, var(--surface) 94%, transparent); }
  .header-inner { display: flex; align-items: center; justify-content: space-between; width: min(880px, 100%); min-height: 68px; margin: 0 auto; padding: 0 1.5rem; }
  .brand { display: flex; align-items: center; gap: .65rem; padding: 0; border: 0; color: var(--text); background: transparent; font: inherit; font-size: 1.08rem; font-weight: 780; cursor: pointer; }
  .brand span { display: grid; place-items: center; width: 34px; height: 34px; border-radius: .7rem; color: white; background: var(--accent); font-size: .95rem; }
  nav { display: flex; gap: .35rem; } nav button { min-height: 40px; padding: .55rem .8rem; border: 0; border-radius: .55rem; color: var(--text-soft); background: transparent; font: inherit; font-weight: 600; cursor: pointer; }
  nav button:hover { color: var(--text); background: var(--surface-raised); }
  nav button.active { color: var(--accent-strong); background: var(--accent-soft); }
  main { width: min(880px, 100%); margin: 0 auto; padding: 1.55rem 1.5rem 2.5rem; }
  .intro { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: 1.25rem; }
  h1 { margin: 0; font-size: clamp(1.35rem, 3vw, 1.75rem); letter-spacing: -.025em; } .intro p { margin: .3rem 0 0; color: var(--text-soft); font-size: .9rem; }
  .primary { min-height: 44px; padding: .7rem 1rem; border: 0; border-radius: .7rem; color: white; background: var(--accent); box-shadow: 0 4px 12px color-mix(in srgb, var(--accent) 22%, transparent); font: inherit; font-weight: 750; cursor: pointer; }
  .primary:hover { background: var(--accent-strong); }
  .state-message { display: grid; place-items: center; min-height: 230px; color: var(--text-soft); } .state-message.error { align-content: center; gap: .6rem; } .state-message.error p { margin: 0; }
  .state-message.error button { min-height: 40px; padding: .55rem .85rem; border: 1px solid var(--border); border-radius: .55rem; color: var(--text); background: var(--surface); font: inherit; cursor: pointer; }
  .toast { position: fixed; z-index: 30; right: 1.25rem; bottom: 1.25rem; display: flex; align-items: center; max-width: min(440px, calc(100% - 2.5rem)); min-height: 48px; padding: .55rem .55rem .55rem .9rem; border: 1px solid var(--border); border-radius: .75rem; color: var(--text); background: var(--surface); box-shadow: var(--shadow-lg); font-size: .88rem; }
  .toast span { line-height: 1.4; } .toast button { min-width: 40px; min-height: 40px; margin-left: .5rem; border: 0; border-radius: .5rem; color: var(--text-soft); background: transparent; font-size: 1.2rem; cursor: pointer; }
  @media (prefers-color-scheme: dark) {
    :global(:root) { color: #e6eee9; background: #101612; --text: #e6eee9; --text-soft: #a7b5ad; --text-faint: #7f9087; --surface: #19221d; --surface-raised: #222d27; --border: #303d35; --border-strong: #46564d; --accent: #49a879; --accent-strong: #7bc79d; --accent-soft: #203a2c; --danger: #e16868; --warning-text: #f1cd78; --warning-soft: #3d321c; --switch-off: #59675f; --tooltip-bg: #ecf4ef; --tooltip-text: #152019; --shadow-sm: 0 1px 2px rgb(0 0 0 / .18); --shadow-md: 0 8px 22px rgb(0 0 0 / .25); --shadow-lg: 0 18px 48px rgb(0 0 0 / .5); }
    :global(body) { background: #101612; }
  }
  @media (max-width: 600px) { .header-inner, main { padding-inline: .8rem; } .brand { font-size: 0; gap: 0; } .brand span { font-size: .95rem; } nav button { padding-inline: .55rem; font-size: .85rem; } .intro { align-items: flex-end; } .intro p { display: none; } .primary { padding-inline: .8rem; } }
</style>
