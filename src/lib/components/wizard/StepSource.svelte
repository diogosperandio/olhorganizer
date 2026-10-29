<script lang="ts">
  import { tick } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { validateRule } from "$lib/api";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { FilterTemplate, Rule } from "$lib/types";
  import { customMarkersFromFilter, filterWithCustomMarker, filterWithTypes, folderNameFromPath, knownFileTypes, type CustomMarker, type CustomMarkerKind, typesFromFilter } from "./types";

  let { draft = $bindable(), templates, customMarkers = $bindable(), selectedTypes = $bindable(), selectedTemplate = $bindable(), errors, editing }: {
    draft: Rule; templates: FilterTemplate[]; customMarkers: CustomMarker[];
    selectedTypes: string[]; selectedTemplate: string; errors: string[]; editing: boolean;
  } = $props();

  let otherType = $state("");
  let markerName = $state("");
  let markerKind = $state<CustomMarkerKind>("TEXTO");
  let markerError = $state("");
  let advancedOpen = $state(false);
  let filterInput = $state<HTMLInputElement>();
  let lastFilterCursor = $state<number | undefined>();

  async function chooseSource() {
    const selected = await open({ directory: true, multiple: false, defaultPath: draft.sourceDir || undefined });
    if (typeof selected !== "string") return;
    const shouldPrefill = !editing && (!draft.name.trim() || draft.name === folderNameFromPath(draft.sourceDir));
    draft.sourceDir = selected;
    if (shouldPrefill) draft.name = folderNameFromPath(selected);
  }

  function chooseTemplate(template: FilterTemplate) {
    selectedTemplate = template.filter;
    draft.filter = template.filter;
    selectedTypes = typesFromFilter(template.filter);
    customMarkers = customMarkersFromFilter(template.filter);
    lastFilterCursor = undefined;
    if (template.filter.includes("{YYYY}") && template.filter.includes("{MM}") && template.filter.includes("{DD}")) draft.dateSource = "file_name";
  }

  function toggleType(type: string) {
    selectedTypes = selectedTypes.includes(type) ? selectedTypes.filter((value) => value !== type) : [...selectedTypes, type];
    draft.filter = filterWithTypes(draft.filter, selectedTypes);
    lastFilterCursor = undefined;
  }

  function addOtherType() {
    const type = otherType.trim().replace(/^\./, "").toLowerCase().replace(/[^a-z0-9]/g, "");
    if (!type) return;
    if (!selectedTypes.includes(type)) selectedTypes = [...selectedTypes, type];
    draft.filter = filterWithTypes(draft.filter, selectedTypes);
    lastFilterCursor = undefined;
    otherType = "";
  }

  function suggestedMarkerName(value: string): string {
    return value
      .normalize("NFD")
      .replace(/[\u0300-\u036f]/g, "")
      .replace(/[^a-zA-Z0-9_]+/g, "_")
      .replace(/^_+|_+$/g, "");
  }

  function markerNameError(): string {
    if (!markerName) return "";
    if (/^[a-zA-Z0-9_]+$/.test(markerName)) return "";
    const suggestion = suggestedMarkerName(markerName);
    return suggestion
      ? `${ptBR.invalidMarkerName} ${ptBR.invalidMarkerNameSuggestion} ${suggestion}`
      : ptBR.invalidMarkerName;
  }

  async function addCustomMarker() {
    if (!markerName) return;
    markerError = "";
    const localError = markerNameError();
    if (localError) { markerError = localError; return; }
    const token = `{${markerKind}:${markerName}}`;
    const inserted = filterWithCustomMarker(draft.filter, token, lastFilterCursor);
    const candidate = { ...draft, filter: inserted.filter };
    try {
      const result = await validateRule(candidate);
      const compilerError = result.errors.find((error) =>
        error.includes(`'${markerName}'`) && /(repetido|conflito|inválido)/i.test(error),
      );
      if (compilerError) { markerError = compilerError; return; }
    } catch (error) {
      markerError = `${ptBR.actionError} ${String(error)}`;
      return;
    }
    draft.filter = inserted.filter;
    customMarkers = customMarkersFromFilter(draft.filter);
    selectedTemplate = "";
    lastFilterCursor = inserted.start + token.length;
    markerName = "";
    advancedOpen = true;
    await tick();
    filterInput?.focus();
    filterInput?.setSelectionRange(inserted.start, inserted.start + token.length);
    window.setTimeout(() => {
      if (filterInput?.selectionStart === inserted.start && filterInput.selectionEnd === inserted.start + token.length) {
        filterInput.setSelectionRange(inserted.start + token.length, inserted.start + token.length);
      }
    }, 1200);
  }

  function removeCustomMarker(marker: CustomMarker) {
    if (draft.destination.includes(marker.token)
      && !window.confirm(ptBR.removeMarkerUsedDestination.replace("{name}", marker.token))) return;
    const capture = new RegExp(`\\{(?:TEXTO|NUM):${marker.name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\}`);
    draft.filter = draft.filter.replace(capture, "");
    customMarkers = customMarkersFromFilter(draft.filter);
    selectedTemplate = "";
    lastFilterCursor = undefined;
  }

  function updateFilterCursor() {
    lastFilterCursor = filterInput?.selectionStart ?? undefined;
  }

  function updateRawFilter() {
    selectedTemplate = "";
    selectedTypes = typesFromFilter(draft.filter);
    customMarkers = customMarkersFromFilter(draft.filter);
    updateFilterCursor();
  }
</script>

<div class="step-stack">
  <label class="field"><span>{ptBR.ruleName}</span><input bind:value={draft.name} placeholder={ptBR.ruleNamePlaceholder} />{#if !draft.name.trim()}<small class="field-error">{ptBR.requiredName}</small>{/if}</label>

  <fieldset class="field-group">
    <legend>{ptBR.sourceFolder}</legend>
    <button class="picker" type="button" onclick={chooseSource}>{ptBR.chooseFolder}</button>
    <p class="path" class:empty={!draft.sourceDir}>{draft.sourceDir || ptBR.noFolderChosen}</p>
    {#if !draft.sourceDir}<small class="field-error">{ptBR.requiredSource}</small>{/if}
    {#each errors as error}<small class="field-error">{error}</small>{/each}
  </fieldset>

  <fieldset class="field-group"><legend>{ptBR.whichFiles}</legend>
    {#if templates.length === 0}<p class="muted">{ptBR.loadingOptions}</p>{:else}<div class="template-grid">{#each templates as template}
      <label class="radio-card" class:selected={selectedTemplate === template.filter}><input type="radio" name="filter-template" checked={selectedTemplate === template.filter} onchange={() => chooseTemplate(template)} /><strong>{template.namePt}</strong><code>{template.filter}</code></label>
    {/each}</div>{/if}
  </fieldset>

  <fieldset class="field-group"><legend>{ptBR.fileTypes}</legend><p class="help">{ptBR.selectedTypesHelp}</p>
    <div class="chips">{#each knownFileTypes as type}<button type="button" class:active={selectedTypes.includes(type)} aria-pressed={selectedTypes.includes(type)} onclick={() => toggleType(type)}>{type === "jpg" ? "JPG/JPEG" : type.toUpperCase()}</button>{/each}
      {#each selectedTypes.filter((type) => !knownFileTypes.includes(type as never)) as type}<button type="button" class="active" aria-pressed="true" onclick={() => toggleType(type)}>{type.toUpperCase()}</button>{/each}
    </div>
    <div class="inline-input"><label><span>{ptBR.otherType}</span><input bind:value={otherType} onkeydown={(event) => { if (event.key === "Enter") { event.preventDefault(); addOtherType(); } }} /></label><button type="button" onclick={addOtherType}>{ptBR.addType}</button></div>
  </fieldset>

  <details class="marker-builder"><summary>{ptBR.useNameMarker}</summary><p class="help">{ptBR.markerHelp}</p>
    <div class="marker-controls"><label><span>{ptBR.markerNameLabel}</span><input bind:value={markerName} placeholder={ptBR.markerNamePlaceholder} aria-invalid={Boolean(markerError)} oninput={() => (markerError = markerNameError())} onkeydown={(event) => { if (event.key === "Enter") { event.preventDefault(); void addCustomMarker(); } }} />{#if markerError}<small class="field-error" aria-live="polite">{markerError}</small>{/if}</label>
      <div class="segmented" aria-label={ptBR.markerTypeLabel}><button type="button" class:active={markerKind === "TEXTO"} aria-pressed={markerKind === "TEXTO"} onclick={() => (markerKind = "TEXTO")}>{ptBR.markerAsText}</button><button type="button" class:active={markerKind === "NUM"} aria-pressed={markerKind === "NUM"} onclick={() => (markerKind = "NUM")}>{ptBR.markerAsNumber}</button></div>
      <button class="picker" type="button" disabled={!markerName} onclick={() => void addCustomMarker()}>{ptBR.addMarker}</button>
    </div>
    {#if customMarkers.length}<div class="marker-chips">{#each customMarkers as marker}<span>{marker.name} · {marker.kind === "TEXTO" ? ptBR.markerAsText.toLocaleLowerCase("pt-BR") : ptBR.markerAsNumber.toLocaleLowerCase("pt-BR")}<button type="button" aria-label={`${ptBR.removeMarker} ${marker.name}`} onclick={() => removeCustomMarker(marker)}>×</button></span>{/each}</div>{/if}
  </details>

  <label class="switch-row"><button type="button" role="switch" aria-label={ptBR.includeSubfoldersLabel} aria-checked={draft.includeSubfolders} class:on={draft.includeSubfolders} onclick={() => (draft.includeSubfolders = !draft.includeSubfolders)}><span></span></button><span><strong>{ptBR.includeSubfoldersLabel}</strong><small>{ptBR.includeSubfoldersHelp}</small></span></label>

  <details class="advanced" bind:open={advancedOpen}><summary>{ptBR.advancedUsers}</summary><label class="field"><span>{ptBR.rawFilter}</span><input bind:this={filterInput} class="code-input" bind:value={draft.filter} oninput={updateRawFilter} onclick={updateFilterCursor} onkeyup={updateFilterCursor} onselect={updateFilterCursor} /></label></details>
</div>

<style>
  .template-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: .65rem; }
  .radio-card { display: grid; min-height: 74px; padding: .8rem; border: 1px solid var(--border); border-radius: .7rem; cursor: pointer; }
  .radio-card.selected { border-color: var(--accent); background: var(--accent-soft); } .radio-card input { position: absolute; opacity: 0; } .radio-card code { margin-top: .25rem; color: var(--text-soft); }
  .chips { display: flex; flex-wrap: wrap; gap: .45rem; } .chips button { min-height: 40px; padding: .45rem .7rem; border: 1px solid var(--border); border-radius: 999px; color: var(--text); background: var(--surface-raised); font-weight: 700; cursor: pointer; }
  .chips button.active { border-color: var(--accent); color: var(--accent-strong); background: var(--accent-soft); }
  .inline-input, .marker-controls { display: flex; align-items: end; gap: .55rem; margin-top: .7rem; } .inline-input label, .marker-controls label { display: grid; flex: 1; gap: .3rem; }
  .marker-builder, .advanced { padding: .75rem; border: 1px solid var(--border); border-radius: .7rem; } summary { min-height: 40px; padding-top: .55rem; font-weight: 700; cursor: pointer; }
  .segmented { display: flex; } .segmented button { min-height: 40px; border: 1px solid var(--border); color: var(--text); background: var(--surface); cursor: pointer; } .segmented button:first-child { border-radius: .55rem 0 0 .55rem; } .segmented button:last-child { border-radius: 0 .55rem .55rem 0; } .segmented button.active { color: white; background: var(--accent); }
  .marker-chips { display: flex; flex-wrap: wrap; gap: .4rem; margin-top: .7rem; } .marker-chips span { display: inline-flex; align-items: center; gap: .3rem; min-height: 32px; padding: .25rem .35rem .25rem .6rem; border-radius: 999px; color: var(--accent-strong); background: var(--accent-soft); font-size: .8rem; font-weight: 700; } .marker-chips button { display: grid; place-items: center; width: 26px; height: 26px; padding: 0; border: 0; border-radius: 50%; color: inherit; background: transparent; font: inherit; font-size: 1rem; cursor: pointer; } .marker-chips button:hover, .marker-chips button:focus-visible { background: var(--surface); }
  .switch-row { display: flex; align-items: center; gap: .7rem; } .switch-row > button { position: relative; width: 46px; min-width: 46px; height: 40px; border: 0; border-radius: 999px; background: var(--switch-off); cursor: pointer; } .switch-row > button span { position: absolute; top: 9px; left: 7px; width: 22px; height: 22px; border-radius: 50%; background: white; transition: 150ms; } .switch-row > button.on { background: var(--accent); } .switch-row > button.on span { transform: translateX(10px); } .switch-row > span { display: grid; gap: .2rem; } .switch-row small { color: var(--text-soft); }
  @media (max-width: 620px) { .template-grid { grid-template-columns: 1fr; } .marker-controls { align-items: stretch; flex-direction: column; } }
</style>
