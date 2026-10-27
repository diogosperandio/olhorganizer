<script lang="ts">
  import { open } from "@tauri-apps/plugin-dialog";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { FilterTemplate, Rule } from "$lib/types";
  import { customMarkersFromFilter, fileNameFromPath, filterWithTypes, folderNameFromPath, knownFileTypes, type CustomMarker, type CustomMarkerKind, typesFromFilter } from "./types";

  let { draft = $bindable(), templates, customMarkers = $bindable(), sampleFile = $bindable(), selectedTypes = $bindable(), selectedTemplate = $bindable(), errors, editing }: {
    draft: Rule; templates: FilterTemplate[]; customMarkers: CustomMarker[]; sampleFile: string;
    selectedTypes: string[]; selectedTemplate: string; errors: string[]; editing: boolean;
  } = $props();

  let otherType = $state("");
  let selectedText = $state("");
  let selectionStart = $state(0);
  let selectionEnd = $state(0);
  let markerName = $state("");
  let markerKind = $state<CustomMarkerKind>("TEXTO");
  let markerError = $state("");
  let sampleInput = $state<HTMLInputElement>();

  async function chooseSource() {
    const selected = await open({ directory: true, multiple: false, defaultPath: draft.sourceDir || undefined });
    if (typeof selected !== "string") return;
    const shouldPrefill = !editing && (!draft.name.trim() || draft.name === folderNameFromPath(draft.sourceDir));
    draft.sourceDir = selected;
    if (shouldPrefill) draft.name = folderNameFromPath(selected);
  }

  async function chooseSample() {
    const selected = await open({ directory: false, multiple: false, defaultPath: draft.sourceDir || undefined });
    if (typeof selected === "string") {
      sampleFile = fileNameFromPath(selected);
      selectedText = "";
    }
  }

  function chooseTemplate(template: FilterTemplate) {
    selectedTemplate = template.filter;
    draft.filter = template.filter;
    selectedTypes = typesFromFilter(template.filter);
    if (template.filter.includes("{YYYY}") && template.filter.includes("{MM}") && template.filter.includes("{DD}")) draft.dateSource = "file_name";
  }

  function toggleType(type: string) {
    selectedTypes = selectedTypes.includes(type) ? selectedTypes.filter((value) => value !== type) : [...selectedTypes, type];
    draft.filter = filterWithTypes(draft.filter, selectedTypes);
  }

  function addOtherType() {
    const type = otherType.trim().replace(/^\./, "").toLowerCase().replace(/[^a-z0-9]/g, "");
    if (!type) return;
    if (!selectedTypes.includes(type)) selectedTypes = [...selectedTypes, type];
    draft.filter = filterWithTypes(draft.filter, selectedTypes);
    otherType = "";
  }

  function updateSelection() {
    selectionStart = sampleInput?.selectionStart ?? 0;
    selectionEnd = sampleInput?.selectionEnd ?? 0;
    selectedText = sampleFile.slice(selectionStart, selectionEnd);
    if (/^\d+$/.test(selectedText)) markerKind = "NUM";
  }

  function structure(value: string): string {
    return value
      .replace(/\d{8}/g, "\uE000")
      .replace(/[\p{L}\p{N}]+/gu, "*")
      .replace(/\*+/g, "*")
      .replace(/\uE000/g, "{YYYY}{MM}{DD}");
  }

  function addCustomMarker() {
    markerError = "";
    const stem = sampleFile.replace(/\.[^.]+$/, "");
    if (!selectedText || selectionEnd > stem.length) { markerError = ptBR.invalidMarkerSelection; return; }
    if (!/^[a-zA-Z][a-zA-Z0-9_-]*$/.test(markerName)) { markerError = ptBR.invalidMarkerName; return; }
    const token = `{${markerKind}:${markerName}}`;
    const prefix = structure(stem.slice(0, selectionStart));
    const suffix = structure(stem.slice(selectionEnd));
    draft.filter = filterWithTypes(`${prefix}${token}${suffix}`, selectedTypes.length ? selectedTypes : typesFromFilter(draft.filter));
    customMarkers = [...customMarkers.filter((marker) => marker.name !== markerName), { name: markerName, kind: markerKind, token: `{${markerName}}` }];
    markerName = "";
    selectedText = "";
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
    <button class="picker" type="button" disabled={!draft.sourceDir} onclick={chooseSample}>{ptBR.chooseSample}</button>
    {#if sampleFile}<label class="field"><span>{ptBR.sampleFile}</span><input class="sample" bind:this={sampleInput} value={sampleFile} readonly onselect={updateSelection} onmouseup={updateSelection} onkeyup={updateSelection} /></label>
      <div class="selection-row"><span>{ptBR.selection}:</span><strong>{selectedText || "—"}</strong></div>
      <div class="marker-controls"><label><span>{ptBR.markerNameLabel}</span><input bind:value={markerName} placeholder={ptBR.markerNamePlaceholder} /></label>
        <div class="segmented"><button type="button" class:active={markerKind === "TEXTO"} onclick={() => (markerKind = "TEXTO")}>{ptBR.markerAsText}</button><button type="button" class:active={markerKind === "NUM"} onclick={() => (markerKind = "NUM")}>{ptBR.markerAsNumber}</button></div>
        <button class="picker" type="button" onclick={addCustomMarker}>{ptBR.addMarker}</button>
      </div>{#if markerError}<small class="field-error">{markerError}</small>{/if}
    {/if}
  </details>

  <label class="switch-row"><button type="button" role="switch" aria-label={ptBR.includeSubfoldersLabel} aria-checked={draft.includeSubfolders} class:on={draft.includeSubfolders} onclick={() => (draft.includeSubfolders = !draft.includeSubfolders)}><span></span></button><span><strong>{ptBR.includeSubfoldersLabel}</strong><small>{ptBR.includeSubfoldersHelp}</small></span></label>

  <details class="advanced"><summary>{ptBR.advancedUsers}</summary><label class="field"><span>{ptBR.rawFilter}</span><input class="code-input" bind:value={draft.filter} oninput={() => { selectedTemplate = ""; selectedTypes = typesFromFilter(draft.filter); customMarkers = customMarkersFromFilter(draft.filter); }} /></label></details>
</div>

<style>
  .template-grid { display: grid; grid-template-columns: repeat(2, 1fr); gap: .65rem; }
  .radio-card { display: grid; min-height: 74px; padding: .8rem; border: 1px solid var(--border); border-radius: .7rem; cursor: pointer; }
  .radio-card.selected { border-color: var(--accent); background: var(--accent-soft); } .radio-card input { position: absolute; opacity: 0; } .radio-card code { margin-top: .25rem; color: var(--text-soft); }
  .chips { display: flex; flex-wrap: wrap; gap: .45rem; } .chips button { min-height: 40px; padding: .45rem .7rem; border: 1px solid var(--border); border-radius: 999px; color: var(--text); background: var(--surface-raised); font-weight: 700; cursor: pointer; }
  .chips button.active { border-color: var(--accent); color: var(--accent-strong); background: var(--accent-soft); }
  .inline-input, .marker-controls { display: flex; align-items: end; gap: .55rem; margin-top: .7rem; } .inline-input label, .marker-controls label { display: grid; flex: 1; gap: .3rem; }
  .marker-builder, .advanced { padding: .75rem; border: 1px solid var(--border); border-radius: .7rem; } summary { min-height: 40px; padding-top: .55rem; font-weight: 700; cursor: pointer; }
  .sample { user-select: text; } .selection-row { display: flex; gap: .5rem; margin-top: .5rem; color: var(--text-soft); } .selection-row strong { color: var(--accent-strong); }
  .segmented { display: flex; } .segmented button { min-height: 40px; border: 1px solid var(--border); color: var(--text); background: var(--surface); cursor: pointer; } .segmented button:first-child { border-radius: .55rem 0 0 .55rem; } .segmented button:last-child { border-radius: 0 .55rem .55rem 0; } .segmented button.active { color: white; background: var(--accent); }
  .switch-row { display: flex; align-items: center; gap: .7rem; } .switch-row > button { position: relative; width: 46px; min-width: 46px; height: 40px; border: 0; border-radius: 999px; background: var(--switch-off); cursor: pointer; } .switch-row > button span { position: absolute; top: 9px; left: 7px; width: 22px; height: 22px; border-radius: 50%; background: white; transition: 150ms; } .switch-row > button.on { background: var(--accent); } .switch-row > button.on span { transform: translateX(10px); } .switch-row > span { display: grid; gap: .2rem; } .switch-row small { color: var(--text-soft); }
  @media (max-width: 620px) { .template-grid { grid-template-columns: 1fr; } .marker-controls { align-items: stretch; flex-direction: column; } }
</style>
