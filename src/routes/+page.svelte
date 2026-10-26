<script lang="ts">
  import { onMount } from "svelte";

  import {
    listRules,
    onFileProcessed,
    onRuleError,
    onRulePaused,
    testRule,
  } from "$lib/api";
  import type { Rule } from "$lib/types";

  let rules = $state<Rule[]>([]);
  let output = $state("Clique em “Listar regras” para testar a ponte com o backend.");
  let busy = $state(false);

  async function run(label: string, operation: () => Promise<unknown>) {
    busy = true;
    try {
      output = JSON.stringify(await operation(), null, 2);
    } catch (error) {
      output = `${label}: ${String(error)}`;
    } finally {
      busy = false;
    }
  }

  async function loadRules() {
    await run("Falha ao listar regras", async () => {
      rules = await listRules();
      return rules;
    });
  }

  async function testFirstRule() {
    const first = rules[0];
    if (!first) {
      output = "Não há regras para testar.";
      return;
    }
    await run("Falha ao testar a regra", () => testRule(first));
  }

  onMount(() => {
    const subscriptions = [
      onFileProcessed((payload) => {
        output = JSON.stringify({ event: "file-processed", payload }, null, 2);
      }),
      onRuleError((payload) => {
        output = JSON.stringify({ event: "rule-error", payload }, null, 2);
      }),
      onRulePaused((payload) => {
        output = JSON.stringify({ event: "rule-paused", payload }, null, 2);
      }),
    ];
    void loadRules();

    return () => {
      for (const subscription of subscriptions) {
        void subscription.then((unlisten) => unlisten());
      }
    };
  });
</script>

<svelte:head>
  <title>Olhorganizer — diagnóstico</title>
</svelte:head>

<main>
  <h1>Olhorganizer</h1>
  <p>Página temporária para validar comandos e eventos do Tauri.</p>

  <div class="actions">
    <button onclick={loadRules} disabled={busy}>Listar regras</button>
    <button onclick={testFirstRule} disabled={busy || rules.length === 0}>Testar primeira regra</button>
  </div>

  <p>{rules.length} regra(s) carregada(s).</p>
  <pre>{output}</pre>
</main>

<style>
  :global(body) {
    margin: 0;
    color: #17202a;
    background: #f4f6f7;
    font-family: system-ui, sans-serif;
  }

  main {
    box-sizing: border-box;
    width: min(960px, 100%);
    margin: 0 auto;
    padding: 2rem;
  }

  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.75rem;
    margin: 1.5rem 0;
  }

  button {
    padding: 0.7rem 1rem;
    border: 0;
    border-radius: 0.5rem;
    color: white;
    background: #21618c;
    cursor: pointer;
  }

  button:disabled {
    cursor: not-allowed;
    opacity: 0.55;
  }

  pre {
    min-height: 18rem;
    overflow: auto;
    padding: 1rem;
    border-radius: 0.5rem;
    color: #eaf2f8;
    background: #17202a;
    white-space: pre-wrap;
  }
</style>
