<script lang="ts">
  import { onMount } from "svelte";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { getHistory, getLogPath, onFileProcessed, onRuleError, onRulePaused } from "$lib/api";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { AppEvent, HistoryEntry, Outcome, Rule } from "$lib/types";

  type ResultKind = "moved" | "copied" | "skipped" | "error";
  interface HistoryItem { key: string; entry: HistoryEntry; }
  interface Row { key: string; timestamp: number; file: string; ruleId: string; ruleName: string; destination: string; folder: string; result: ResultKind; detail: string; }

  let { rules, onNotice }: { rules: Rule[]; onNotice: (message: string) => void } = $props();
  let items = $state<HistoryItem[]>([]);
  let loading = $state(true);
  let loadError = $state("");
  let ruleFilter = $state("");
  let resultFilter = $state("");
  let search = $state("");
  let sequence = 0;
  const dateFormatter = new Intl.DateTimeFormat("pt-BR", { dateStyle: "short", timeStyle: "medium" });

  const ruleMap = $derived(new Map(rules.map((rule) => [rule.id, rule])));
  const rows = $derived(items.map(({ key, entry }) => normalize(key, entry, ruleMap)));
  const filteredRows = $derived(rows.filter((row) =>
    (!ruleFilter || row.ruleId === ruleFilter) &&
    (!resultFilter || row.result === resultFilter) &&
    (!search.trim() || row.file.toLocaleLowerCase("pt-BR").includes(search.trim().toLocaleLowerCase("pt-BR")))
  ));

  function fileName(path: string) { return path.split(/[\\/]/).filter(Boolean).at(-1) || path; }
  function parentPath(path: string) {
    const index = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
    if (index < 0) return path;
    if (index === 0) return path.slice(0, 1);
    if (index === 2 && path[1] === ":") return path.slice(0, 3);
    return path.slice(0, index);
  }
  function fixedDestination(rule?: Rule) { return rule?.destination.split("{")[0].replace(/[\\/]+$/, "") || ""; }
  function outcomeData(outcome: Outcome, rule?: Rule): { result: ResultKind; destination: string; folder: string; detail: string } {
    if (outcome.kind === "moved" || outcome.kind === "wouldMove") return { result: "moved", destination: outcome.to, folder: parentPath(outcome.to), detail: "" };
    if (outcome.kind === "copied" || outcome.kind === "wouldCopy") return { result: "copied", destination: outcome.to, folder: parentPath(outcome.to), detail: "" };
    if (outcome.kind === "failed") return { result: "error", destination: fixedDestination(rule), folder: fixedDestination(rule), detail: outcome.error };
    if (outcome.kind === "skipped") return { result: "skipped", destination: fixedDestination(rule), folder: fixedDestination(rule), detail: outcome.reason };
    return { result: "skipped", destination: fixedDestination(rule), folder: fixedDestination(rule), detail: ptBR.notMatchedReason };
  }
  function normalize(key: string, entry: HistoryEntry, rulesById: Map<string, Rule>): Row {
    const event = entry.event;
    const payload = event.payload;
    const rule = rulesById.get(payload.ruleId);
    if (event.name === "file-processed") {
      const data = outcomeData(event.payload.outcome, rule);
      return { key, timestamp: entry.timestampMs, file: fileName(event.payload.file), ruleId: payload.ruleId, ruleName: rule?.name || ptBR.removedRule, ...data };
    }
    const detail = event.name === "rule-error" ? event.payload.messagePt : event.payload.reasonPt;
    const destination = fixedDestination(rule);
    return { key, timestamp: entry.timestampMs, file: "", ruleId: payload.ruleId, ruleName: rule?.name || ptBR.removedRule, destination, folder: destination, result: "error", detail };
  }
  function resultLabel(result: ResultKind) {
    return result === "moved" ? ptBR.resultMoved : result === "copied" ? ptBR.resultCopied : result === "skipped" ? ptBR.resultSkipped : ptBR.resultError;
  }
  function addEvent(event: AppEvent) {
    const timestampMs = Date.now();
    items = [{ key: `live-${timestampMs}-${sequence++}`, entry: { timestampMs, event } }, ...items].slice(0, 500);
  }
  async function openFolder(path: string) {
    try { await openPath(path); } catch (error) { onNotice(`${ptBR.actionError} ${String(error)}`); }
  }
  async function openLog() {
    try { await openPath(await getLogPath()); } catch (error) { onNotice(`${ptBR.actionError} ${String(error)}`); }
  }

  onMount(() => {
    const subscriptions = [
      onFileProcessed((payload) => addEvent({ name: "file-processed", payload })),
      onRuleError((payload) => addEvent({ name: "rule-error", payload })),
      onRulePaused((payload) => addEvent({ name: "rule-paused", payload })),
    ];
    void getHistory(500).then((history) => {
      const live = items.filter((item) => item.key.startsWith("live-"));
      const liveEvents = new Set(live.map((item) => JSON.stringify(item.entry.event)));
      const stored = history
        .filter((entry) => !liveEvents.has(JSON.stringify(entry.event)))
        .map((entry, index) => ({ key: `stored-${entry.timestampMs}-${index}`, entry }));
      items = [...live, ...stored].slice(0, 500);
    }).catch((error) => (loadError = `${ptBR.historyLoadError} ${String(error)}`)).finally(() => (loading = false));
    return () => { for (const subscription of subscriptions) void subscription.then((unlisten) => unlisten()); };
  });
</script>

<section class="view-heading"><div><h1>{ptBR.historyTitle}</h1><p>{ptBR.historySummary}</p></div><button class="primary" type="button" onclick={openLog}>{ptBR.openLog}</button></section>

<section class="filters">
  <label><span>{ptBR.filterRule}</span><select bind:value={ruleFilter}><option value="">{ptBR.allRules}</option>{#each rules as rule}<option value={rule.id}>{rule.name}</option>{/each}</select></label>
  <label><span>{ptBR.filterResult}</span><select bind:value={resultFilter}><option value="">{ptBR.allResults}</option><option value="moved">{ptBR.resultMoved}</option><option value="copied">{ptBR.resultCopied}</option><option value="skipped">{ptBR.resultSkipped}</option><option value="error">{ptBR.resultError}</option></select></label>
  <label class="search"><span>{ptBR.searchFile}</span><input bind:value={search} placeholder={ptBR.searchFilePlaceholder} /></label>
</section>

{#if loading}<div class="view-state">{ptBR.loadingHistory}</div>
{:else if loadError}<div class="view-state error">{loadError}</div>
{:else if filteredRows.length === 0}<div class="view-state">{ptBR.noHistory}</div>
{:else}<div class="history-table"><table><thead><tr><th>{ptBR.historyDate}</th><th>{ptBR.historyFile}</th><th>{ptBR.historyRule}</th><th>{ptBR.historyDestination}</th><th>{ptBR.historyResult}</th><th>{ptBR.historyActions}</th></tr></thead>
  <tbody>{#each filteredRows as row (row.key)}<tr>
    <td><time datetime={new Date(row.timestamp).toISOString()}>{dateFormatter.format(row.timestamp)}</time></td>
    <td><strong title={row.file}>{row.file || ptBR.noFile}</strong>{#if row.detail}<small class:error-detail={row.result === "error"}>{row.detail}</small>{/if}</td>
    <td>{row.ruleName}</td><td class="destination" title={row.destination}>{row.destination || ptBR.noDestination}</td>
    <td><span class="result {row.result}">{resultLabel(row.result)}</span></td>
    <td>{#if row.folder}<button class="open" type="button" onclick={() => openFolder(row.folder)}>{ptBR.openFolder}</button>{/if}</td>
  </tr>{/each}</tbody>
</table></div>{/if}

<style>
  .view-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; margin-bottom: 1.1rem; } h1 { margin: 0; font-size: clamp(1.35rem, 3vw, 1.75rem); } .view-heading p { margin: .3rem 0 0; color: var(--text-soft); font-size: .9rem; }
  .primary, .open { min-height: 40px; padding: .55rem .8rem; border: 1px solid var(--border); border-radius: .6rem; color: var(--text); background: var(--surface); font: inherit; font-weight: 700; cursor: pointer; } .primary { color: white; border-color: var(--accent); background: var(--accent); }
  .filters { display: grid; grid-template-columns: minmax(150px, .8fr) minmax(150px, .7fr) minmax(220px, 1.2fr); gap: .65rem; margin-bottom: .8rem; padding: .8rem; border: 1px solid var(--border); border-radius: .8rem; background: var(--surface); }
  .filters label { display: grid; gap: .3rem; color: var(--text-soft); font-size: .76rem; font-weight: 700; } select, input { min-height: 40px; padding: .45rem .55rem; border: 1px solid var(--border); border-radius: .5rem; color: var(--text); background: var(--surface-raised); font: inherit; }
  .history-table { max-height: calc(100vh - 245px); overflow: auto; border: 1px solid var(--border); border-radius: .8rem; background: var(--surface); } table { width: 100%; border-collapse: collapse; table-layout: fixed; font-size: .8rem; } th, td { padding: .65rem; border-bottom: 1px solid var(--border); text-align: left; vertical-align: top; overflow-wrap: anywhere; } th { position: sticky; z-index: 2; top: 0; color: var(--text-soft); background: var(--surface-raised); } th:nth-child(1) { width: 130px; } th:nth-child(2) { width: 20%; } th:nth-child(3) { width: 16%; } th:nth-child(5) { width: 90px; } th:nth-child(6) { width: 110px; }
  td strong { display: block; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; } td small { display: block; margin-top: .3rem; color: var(--text-soft); line-height: 1.35; } td small.error-detail { color: var(--danger); } .destination { color: var(--text-soft); }
  .result { display: inline-block; padding: .25rem .45rem; border-radius: 999px; font-size: .68rem; font-weight: 800; } .result.moved, .result.copied { color: var(--accent-strong); background: var(--accent-soft); } .result.skipped { color: var(--warning-text); background: var(--warning-soft); } .result.error { color: var(--danger); background: color-mix(in srgb, var(--danger) 12%, transparent); }
  .open { padding: .4rem .55rem; font-size: .74rem; } .view-state { display: grid; place-items: center; min-height: 220px; color: var(--text-soft); } .view-state.error { color: var(--danger); }
  @media (max-width: 720px) { .filters { grid-template-columns: 1fr 1fr; } .filters .search { grid-column: 1 / -1; } .history-table { max-height: calc(100vh - 300px); } table { min-width: 780px; } .view-heading { align-items: flex-start; } }
</style>
