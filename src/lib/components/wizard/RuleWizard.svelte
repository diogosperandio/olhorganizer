<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { listFilterTemplates, listMarkers, previewDestination, saveRule, testRule, toggleRule, validateRule } from "$lib/api";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { FilterTemplate, MarkerDef, Rule, TestRow, ValidationResult } from "$lib/types";
  import StepBehavior from "./StepBehavior.svelte";
  import StepDestination from "./StepDestination.svelte";
  import StepSource from "./StepSource.svelte";
  import WizardReview from "./WizardReview.svelte";
  import { customMarkersFromFilter, typesFromFilter, type CustomMarker } from "./types";

  let { rule, nextPriority, onClose, onSaved }: {
    rule?: Rule; nextPriority: number; onClose: () => void; onSaved: (rule: Rule, message: string) => void;
  } = $props();

  const initialDraft = untrack((): Rule => rule ? structuredClone(rule) : ({
    id: crypto.randomUUID(), name: "", enabled: true, sourceDir: "", filter: "*", destination: "",
    action: "move", dateSource: "modified", checkMode: "instant", includeSubfolders: false,
    onConflict: "rename", priority: nextPriority,
  }));
  const editing = untrack(() => Boolean(rule));
  let draft = $state<Rule>(initialDraft);
  let templates = $state<FilterTemplate[]>([]);
  let markers = $state<MarkerDef[]>([]);
  let customMarkers = $state<CustomMarker[]>(customMarkersFromFilter(initialDraft.filter));
  let selectedTypes = $state<string[]>(typesFromFilter(initialDraft.filter));
  let selectedTemplate = $state(initialDraft.filter);
  let sampleFile = $state("20261022.pdf");
  let validation = $state<ValidationResult>({ errors: [], warnings: [] });
  let validationBusy = $state(false);
  let preview = $state("");
  let previewError = $state("");
  let step = $state(0);
  let maxVisited = $state(0);
  let testRows = $state<TestRow[]>([]);
  let testing = $state(false);
  let testError = $state("");
  let hasTested = $state(false);
  let saving = $state(false);
  let dialog: HTMLDialogElement;

  const steps = [ptBR.stepSource, ptBR.stepDestination, ptBR.stepBehavior, ptBR.stepTest];
  const destinationErrors = $derived(validation.errors.filter((error) => !isSourceError(error) && !isIntervalError(error)));
  const sourceErrors = $derived(validation.errors.filter(isSourceError));
  const behaviorErrors = $derived(validation.errors.filter(isIntervalError));
  const basicValid = $derived(Boolean(draft.name.trim() && draft.sourceDir && draft.destination && draft.filter));
  const canSave = $derived(basicValid && validation.errors.length === 0 && hasTested && !testing && !saving && !validationBusy);

  function isSourceError(error: string) { return error.includes(ptBR.validationSourceNeedle) && !error.includes(ptBR.validationDestinationNeedle) && !error.includes(ptBR.validationSubfoldersNeedle); }
  function isIntervalError(error: string) { return error.toLocaleLowerCase("pt-BR").includes(ptBR.validationIntervalNeedle); }

  async function loadOptions() {
    try {
      [templates, markers] = await Promise.all([listFilterTemplates(), listMarkers()]);
      selectedTemplate = templates.find((template) => template.filter === draft.filter)?.filter || "";
    } catch (error) { previewError = `${ptBR.actionError} ${String(error)}`; }
  }

  async function refreshValidation() {
    validationBusy = true;
    try { validation = await validateRule(structuredClone(draft)); }
    catch (error) { validation = { errors: [String(error)], warnings: [] }; }
    finally { validationBusy = false; }
    return validation;
  }

  async function runTest() {
    testing = true; testError = ""; testRows = [];
    try { testRows = await testRule(structuredClone(draft)); }
    catch (error) { testError = String(error); }
    finally { testing = false; hasTested = true; }
  }

  async function goTo(target: number) {
    if (!editing && target > maxVisited + 1) return;
    if (target > step) {
      const result = await refreshValidation();
      if (step === 0 && (!draft.name.trim() || !draft.sourceDir || sourceErrors.length)) return;
      if (step === 1 && (!draft.destination || result.errors.some((error) => !isSourceError(error) && !isIntervalError(error)))) return;
      if (step === 2 && (behaviorErrors.length || (typeof draft.checkMode === "object" && draft.checkMode.interval.minutes < 1))) return;
    }
    step = target; maxVisited = Math.max(maxVisited, target);
    if (target === 3 && !hasTested) void runTest();
  }

  async function save(processNow: boolean) {
    const result = await refreshValidation();
    if (!draft.name.trim() || !draft.sourceDir) { step = 0; return; }
    if (!draft.destination || result.errors.some((error) => !isSourceError(error) && !isIntervalError(error))) { step = 1; return; }
    if (result.errors.length || !hasTested) return;
    saving = true;
    try {
      const changedFromSaved = !rule || JSON.stringify(initialDraft) !== JSON.stringify(draft);
      let saved = await saveRule(structuredClone(draft));
      if (processNow) {
        if (!saved.enabled) saved = await toggleRule(saved.id, true);
        else if (editing && !changedFromSaved) {
          await toggleRule(saved.id, false);
          saved = await toggleRule(saved.id, true);
        }
      }
      onSaved(saved, processNow ? ptBR.savedAndProcessing : ptBR.savedRule);
    } catch (error) {
      validation = { errors: String(error).split("\n").filter(Boolean), warnings: validation.warnings };
      if (validation.errors.some(isSourceError)) step = 0;
      else if (validation.errors.some(isIntervalError)) step = 2;
      else step = 1;
    } finally { saving = false; }
  }

  $effect(() => {
    const snapshot = JSON.stringify(draft);
    const timer = window.setTimeout(async () => {
      validationBusy = true;
      try {
        const result = await validateRule(JSON.parse(snapshot) as Rule);
        if (JSON.stringify(draft) === snapshot) validation = result;
      } catch { /* A save action will surface backend errors. */ }
      finally { if (JSON.stringify(draft) === snapshot) validationBusy = false; }
    }, 180);
    return () => window.clearTimeout(timer);
  });

  $effect(() => {
    const snapshot = JSON.stringify({ rule: draft, sampleFile });
    const timer = window.setTimeout(async () => {
      if (!draft.destination || !draft.filter) { preview = ""; previewError = ""; return; }
      try {
        const value = await previewDestination(structuredClone(draft), sampleFile || "20261022.pdf");
        if (JSON.stringify({ rule: draft, sampleFile }) === snapshot) { preview = value; previewError = ""; }
      } catch (error) {
        if (JSON.stringify({ rule: draft, sampleFile }) === snapshot) { preview = ""; previewError = String(error); }
      }
    }, 150);
    return () => window.clearTimeout(timer);
  });

  $effect(() => { JSON.stringify(draft); hasTested = false; });

  onMount(() => { dialog.showModal(); void loadOptions(); });
</script>

<dialog class="wizard" bind:this={dialog} aria-labelledby="wizard-title" oncancel={(event) => { event.preventDefault(); onClose(); }}>
  <div class="wizard-shell">
    <header><div><p>{editing ? ptBR.wizardEditTitle : ptBR.wizardCreateTitle}</p><h2 id="wizard-title">{steps[step]}</h2></div><button type="button" aria-label={ptBR.closeWizard} onclick={onClose}>×</button></header>
    <nav aria-label={ptBR.wizardCreateTitle}>{#each steps as label, index}<button type="button" class:active={step === index} class:done={index < step} disabled={!editing && index > maxVisited} onclick={() => goTo(index)}><span>{index + 1}</span>{label}</button>{/each}</nav>
    <section class="wizard-body">
      {#if step === 0}<StepSource bind:draft {templates} bind:customMarkers bind:sampleFile bind:selectedTypes bind:selectedTemplate errors={sourceErrors} {editing} />
      {:else if step === 1}<StepDestination bind:draft {markers} {customMarkers} {sampleFile} {preview} {previewError} errors={destinationErrors} warnings={validation.warnings} />
      {:else if step === 2}<StepBehavior bind:draft errors={behaviorErrors} />
      {:else}<WizardReview rows={testRows} {testing} {testError} onTest={runTest} />{/if}
    </section>
    <footer>
      {#if step > 0}<button class="secondary" type="button" onclick={() => goTo(step - 1)}>{ptBR.back}</button>{:else}<span></span>{/if}
      <div>
        {#if step < 3}<button class="primary" type="button" onclick={() => goTo(step + 1)}>{ptBR.next}</button>
        {:else}<button class="secondary" type="button" disabled={!canSave} onclick={() => save(false)}>{ptBR.save}</button><button class="primary" type="button" disabled={!canSave} onclick={() => save(true)}>{ptBR.saveAndProcess}</button>{/if}
      </div>
    </footer>
  </div>
</dialog>

<style>
  dialog { width: min(820px, calc(100% - 2rem)); max-height: calc(100vh - 2rem); padding: 0; border: 0; border-radius: 1rem; color: var(--text); background: var(--surface); box-shadow: var(--shadow-lg); }
  dialog::backdrop { background: rgb(10 20 15 / .58); backdrop-filter: blur(3px); }
  .wizard-shell { display: grid; grid-template-rows: auto auto minmax(260px, 1fr) auto; max-height: calc(100vh - 2rem); }
  header { display: flex; align-items: center; justify-content: space-between; padding: 1rem 1.25rem .8rem; border-bottom: 1px solid var(--border); }
  header p { margin: 0 0 .15rem; color: var(--text-soft); font-size: .75rem; font-weight: 700; text-transform: uppercase; } h2 { margin: 0; font-size: 1.25rem; }
  header button { width: 40px; height: 40px; border: 0; border-radius: .55rem; color: var(--text-soft); background: transparent; font-size: 1.35rem; cursor: pointer; }
  nav { display: grid; grid-template-columns: repeat(4, 1fr); gap: .35rem; padding: .65rem 1.25rem; border-bottom: 1px solid var(--border); }
  nav button { display: flex; align-items: center; gap: .4rem; min-height: 40px; padding: .4rem; border: 0; border-radius: .5rem; color: var(--text-faint); background: transparent; font: inherit; font-size: .76rem; text-align: left; cursor: pointer; }
  nav button span { display: grid; place-items: center; width: 24px; height: 24px; flex: 0 0 auto; border: 1px solid var(--border); border-radius: 50%; }
  nav button.active { color: var(--accent-strong); background: var(--accent-soft); font-weight: 700; } nav button.done span { border-color: var(--accent); color: white; background: var(--accent); } nav button:disabled { cursor: not-allowed; opacity: .45; }
  .wizard-body { overflow: auto; padding: 1.1rem 1.25rem 1.4rem; }
  footer { display: flex; align-items: center; justify-content: space-between; gap: .75rem; padding: .8rem 1.25rem; border-top: 1px solid var(--border); background: var(--surface); } footer > div { display: flex; gap: .55rem; }
  footer button { min-height: 42px; padding: .6rem .9rem; border: 1px solid var(--border); border-radius: .6rem; font: inherit; font-weight: 700; cursor: pointer; }
  footer .secondary { color: var(--text); background: var(--surface-raised); } footer .primary { border-color: var(--accent); color: white; background: var(--accent); } footer button:disabled { cursor: not-allowed; opacity: .45; }
  :global(.wizard .step-stack) { display: grid; gap: 1rem; }
  :global(.wizard .field) { display: grid; gap: .35rem; font-weight: 650; }
  :global(.wizard .field input), :global(.wizard .inline-input input), :global(.wizard .marker-controls input) { width: 100%; min-height: 42px; padding: .55rem .65rem; border: 1px solid var(--border); border-radius: .55rem; color: var(--text); background: var(--surface); font: inherit; }
  :global(.wizard .code-input) { font-family: ui-monospace, SFMono-Regular, Consolas, monospace !important; }
  :global(.wizard .field-group) { min-width: 0; margin: 0; padding: 0; border: 0; }
  :global(.wizard .field-group legend) { margin-bottom: .55rem; font-weight: 750; }
  :global(.wizard .picker), :global(.wizard .inline-input > button) { min-height: 42px; padding: .55rem .8rem; border: 1px solid var(--border); border-radius: .55rem; color: var(--text); background: var(--surface-raised); font: inherit; font-weight: 700; cursor: pointer; }
  :global(.wizard .picker:disabled) { cursor: not-allowed; opacity: .45; }
  :global(.wizard .path) { margin: .45rem 0 0; padding: .55rem .65rem; border-radius: .5rem; color: var(--text-soft); background: var(--surface-raised); font-family: ui-monospace, SFMono-Regular, Consolas, monospace; font-size: .78rem; overflow-wrap: anywhere; }
  :global(.wizard .path.empty), :global(.wizard .muted), :global(.wizard .help) { color: var(--text-faint); }
  :global(.wizard .help) { margin: 0 0 .55rem; font-size: .82rem; line-height: 1.45; }
  :global(.wizard .field-error) { display: block; color: var(--danger); font-size: .8rem; line-height: 1.4; }
  :global(.wizard .choice-grid) { display: grid; grid-template-columns: repeat(2, 1fr); gap: .6rem; }
  :global(.wizard .choice-grid.three) { grid-template-columns: repeat(3, 1fr); }
  :global(.wizard .choice-grid > label) { display: flex; align-items: center; min-height: 56px; gap: .55rem; padding: .65rem .75rem; border: 1px solid var(--border); border-radius: .65rem; cursor: pointer; }
  :global(.wizard .choice-grid > label.selected) { border-color: var(--accent); background: var(--accent-soft); }
  @media (max-width: 620px) { dialog { width: calc(100% - 1rem); max-height: calc(100vh - 1rem); } .wizard-shell { max-height: calc(100vh - 1rem); } nav button { justify-content: center; font-size: 0; } nav button span { font-size: .76rem; } footer { align-items: stretch; } footer > div { flex: 1; flex-wrap: wrap; justify-content: flex-end; } :global(.wizard .choice-grid), :global(.wizard .choice-grid.three) { grid-template-columns: 1fr; } }
</style>
