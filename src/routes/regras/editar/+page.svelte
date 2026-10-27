<script lang="ts">
  // New rule (no ?id) or edit an existing one (?id=...).
  import { page } from "$app/state";
  import * as api from "$lib/api";
  import Placeholder from "$lib/components/Placeholder.svelte";
  import RuleWizard from "$lib/components/wizard/RuleWizard.svelte";
  import { t } from "$lib/i18n";
  import type { Rule } from "$lib/types";

  const id = $derived(page.url.searchParams.get("id"));
  // undefined while loading; null for a new rule.
  let initial = $state<Rule | null | undefined>(undefined);
  let error = $state<string | null>(null);

  $effect(() => {
    const ruleId = id;
    initial = undefined;
    error = null;
    if (!ruleId) {
      initial = null;
      return;
    }
    api.getRule(ruleId).then(
      (rule) => (initial = rule),
      (e) => (error = String(e)),
    );
  });
</script>

{#if error}
  <Placeholder title={t("wizard.titleEdit")} message={error} />
{:else if initial === undefined}
  <p class="loading">{t("wizard.loading")}</p>
{:else}
  {#key id}
    <RuleWizard {initial} />
  {/key}
{/if}

<style>
  .loading {
    padding: 24px;
    color: var(--muted);
  }
</style>
