<script lang="ts">
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { Rule } from "$lib/types";
  let { draft = $bindable(), errors }: { draft: Rule; errors: string[] } = $props();
  const intervalMode = $derived(typeof draft.checkMode === "object");
  const intervalMinutes = $derived(typeof draft.checkMode === "object" ? draft.checkMode.interval.minutes : 5);
</script>

<div class="step-stack">
  <fieldset class="field-group"><legend>{ptBR.schedule}</legend><div class="choice-grid">
    <label class:selected={!intervalMode}><input type="radio" name="schedule" checked={!intervalMode} onchange={() => (draft.checkMode = "instant")} /><strong>{ptBR.instant}</strong></label>
    <label class:selected={intervalMode}><input type="radio" name="schedule" checked={intervalMode} onchange={() => (draft.checkMode = { interval: { minutes: Math.max(1, intervalMinutes) } })} /><span>{ptBR.interval} <input class="minutes" type="number" min="1" value={intervalMinutes} disabled={!intervalMode} oninput={(event) => (draft.checkMode = { interval: { minutes: Number(event.currentTarget.value) } })} /> {ptBR.minutesLabel}</span></label>
  </div>{#each errors as error}<small class="field-error">{error}</small>{/each}</fieldset>

  <fieldset class="field-group"><legend>{ptBR.operation}</legend><div class="choice-grid">
    <label class:selected={draft.action === "move"}><input type="radio" name="action" value="move" bind:group={draft.action} /><strong>{ptBR.move}</strong></label>
    <label class:selected={draft.action === "copy"}><input type="radio" name="action" value="copy" bind:group={draft.action} /><strong>{ptBR.copy}</strong></label>
  </div></fieldset>

  <fieldset class="field-group"><legend>{ptBR.conflictQuestion}</legend><div class="choice-grid three">
    <label class:selected={draft.onConflict === "rename"}><input type="radio" name="conflict" value="rename" bind:group={draft.onConflict} />{ptBR.conflictRename}</label>
    <label class:selected={draft.onConflict === "overwrite"}><input type="radio" name="conflict" value="overwrite" bind:group={draft.onConflict} />{ptBR.conflictOverwrite}</label>
    <label class:selected={draft.onConflict === "skip"}><input type="radio" name="conflict" value="skip" bind:group={draft.onConflict} />{ptBR.conflictSkip}</label>
  </div></fieldset>
</div>

<style>
  .minutes { width: 72px; min-height: 40px; margin-inline: .25rem; padding: .35rem; border: 1px solid var(--border); border-radius: .45rem; color: var(--text); background: var(--surface); font: inherit; }
  @media (max-width: 620px) { .choice-grid.three { grid-template-columns: 1fr; } }
</style>
