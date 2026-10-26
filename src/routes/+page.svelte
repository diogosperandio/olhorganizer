<script lang="ts">
  // TEMPORARY debug page proving the Rust <-> UI bridge. Replaced by the real
  // screens in a later step.
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import type { Rule } from "$lib/types";

  let rules = $state<Rule[]>([]);
  let output = $state("");
  let events = $state<string[]>([]);

  function show(value: unknown) {
    output = JSON.stringify(value, null, 2);
  }

  async function run(action: () => Promise<unknown>) {
    try {
      show(await action());
    } catch (e) {
      output = `Erro: ${e}`;
    }
  }

  async function listRules() {
    await run(async () => (rules = await api.listRules()));
  }

  async function testFirstRule() {
    if (rules.length === 0) {
      output = "Nenhuma regra carregada. Clique em list_rules primeiro.";
      return;
    }
    await run(() => api.testRule(rules[0]));
  }

  function log(type: string, payload: unknown) {
    const time = new Date().toLocaleTimeString("pt-BR");
    events = [`${time} ${type} ${JSON.stringify(payload)}`, ...events].slice(0, 100);
  }

  onMount(() => {
    const unlisten = [
      api.onFileProcessed((e) => log("file-processed", e)),
      api.onRuleError((e) => log("rule-error", e)),
      api.onRulePaused((e) => log("rule-paused", e)),
      api.onRuleResumed((e) => log("rule-resumed", e)),
      api.onRuleHint((e) => log("rule-hint", e)),
    ];
    listRules();
    return () => unlisten.forEach((p) => p.then((stop) => stop()));
  });
</script>

<main>
  <h1>Olhorganizer — depuração</h1>
  <p>Página temporária para testar a ponte entre a interface e o Rust.</p>

  <div class="buttons">
    <button onclick={listRules}>list_rules</button>
    <button onclick={testFirstRule}>test_rule (primeira regra)</button>
    <button onclick={() => run(() => api.getHistory(20))}>get_history</button>
    <button onclick={() => run(api.listMarkers)}>list_markers</button>
    <button onclick={() => run(api.getSettings)}>get_settings</button>
  </div>

  <h2>Resultado</h2>
  <pre>{output}</pre>

  <h2>Eventos ({events.length})</h2>
  <pre>{events.join("\n")}</pre>
</main>

<style>
  main {
    font-family: system-ui, sans-serif;
    padding: 1rem;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  pre {
    background: #f4f4f4;
    color: #222;
    padding: 0.75rem;
    max-height: 40vh;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-all;
  }
</style>
