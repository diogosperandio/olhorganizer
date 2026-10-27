<script lang="ts">
  import { tick } from "svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { MarkerDef, Rule } from "$lib/types";
  import type { CustomMarker } from "./types";

  let { draft = $bindable(), markers, customMarkers, sampleFile, preview, previewError, errors, warnings }: {
    draft: Rule; markers: MarkerDef[]; customMarkers: CustomMarker[]; sampleFile: string;
    preview: string; previewError: string; errors: string[]; warnings: string[];
  } = $props();
  let destinationInput: HTMLInputElement;
  let showMore = $state(false);

  const primaryKinds = new Set(["year4", "month2", "day2", "monthName", "ext"]);
  const primaryMarkers = $derived(markers.filter((marker) => primaryKinds.has(marker.kind)));
  const moreMarkers = $derived(markers.filter((marker) => !primaryKinds.has(marker.kind)));
  const markerLabels: Record<string, string> = { year4: ptBR.markerYear, year2: ptBR.markerYearShort, month2: ptBR.markerMonth, monthName: ptBR.markerMonthName, day2: ptBR.markerDay, name: ptBR.markerName, ext: ptBR.markerExtension };

  function separator() { return draft.destination.includes("\\") || draft.sourceDir.includes("\\") ? "\\" : "/"; }

  async function chooseBase() {
    const selected = await open({ directory: true, multiple: false });
    if (typeof selected !== "string") return;
    const markerIndex = draft.destination.indexOf("{");
    const suffix = markerIndex >= 0 ? draft.destination.slice(markerIndex) : "";
    const slash = selected.endsWith("/") || selected.endsWith("\\") ? "" : (selected.includes("\\") ? "\\" : "/");
    draft.destination = `${selected}${slash}${suffix}`;
  }

  async function insert(value: string) {
    const input = destinationInput;
    const start = input?.selectionStart ?? draft.destination.length;
    const end = input?.selectionEnd ?? start;
    draft.destination = `${draft.destination.slice(0, start)}${value}${draft.destination.slice(end)}`;
    await tick();
    input?.focus(); input?.setSelectionRange(start + value.length, start + value.length);
  }
</script>

<div class="step-stack">
  <fieldset class="field-group"><legend>{ptBR.destinationBase}</legend>
    <button class="picker" type="button" onclick={chooseBase}>{ptBR.chooseBaseFolder}</button>
    <p class="path" class:empty={!draft.destination}>{draft.destination || ptBR.noFolderChosen}</p>
  </fieldset>

  <label class="field"><span>{ptBR.destinationFormat}</span><input bind:this={destinationInput} class="code-input" bind:value={draft.destination} /></label>
  <div class="marker-buttons">
    {#each primaryMarkers as marker}<button type="button" title={marker.descriptionPt} onclick={() => insert(marker.token)}>[{markerLabels[marker.kind]}]</button>{/each}
    {#each customMarkers as marker}<button class="custom" type="button" onclick={() => insert(marker.token)}>[{marker.name}]</button>{/each}
    <button type="button" aria-label={ptBR.insertSubfolder} title={ptBR.insertSubfolder} onclick={() => insert(separator())}>/</button>
    {#if moreMarkers.length}<button type="button" aria-expanded={showMore} onclick={() => (showMore = !showMore)}>{ptBR.moreMarkers}</button>{/if}
  </div>
  {#if showMore}<div class="marker-buttons more">{#each moreMarkers as marker}<button type="button" title={marker.descriptionPt} onclick={() => insert(marker.token)}>[{markerLabels[marker.kind]}]</button>{/each}</div>{/if}

  {#if !draft.destination}<small class="field-error">{ptBR.requiredDestination}</small>{/if}
  {#each errors as error}<small class="field-error">{error}</small>{/each}
  {#each warnings as warning}<aside class="warning">{warning}</aside>{/each}

  <section class="preview" aria-live="polite"><strong>{ptBR.livePreview}</strong>
    {#if preview}<p><span>{sampleFile}</span><b aria-label={ptBR.previewArrow}>→</b><span>{preview}</span></p>
    {:else}<p class="muted">{previewError || ptBR.previewWaiting}</p>{/if}
  </section>

  <fieldset class="field-group"><legend>{ptBR.dateComesFrom}</legend><div class="choice-grid three">
    <label class:selected={draft.dateSource === "file_name"}><input type="radio" name="date-source" value="file_name" bind:group={draft.dateSource} />{ptBR.dateFileName}</label>
    <label class:selected={draft.dateSource === "modified"}><input type="radio" name="date-source" value="modified" bind:group={draft.dateSource} />{ptBR.dateModified}</label>
    <label class:selected={draft.dateSource === "detected"}><input type="radio" name="date-source" value="detected" bind:group={draft.dateSource} />{ptBR.dateDetected}</label>
  </div></fieldset>
</div>

<style>
  .marker-buttons { display: flex; flex-wrap: wrap; gap: .4rem; }
  .marker-buttons button { min-height: 40px; padding: .45rem .65rem; border: 1px solid var(--border); border-radius: .55rem; color: var(--accent-strong); background: var(--accent-soft); font: inherit; font-weight: 700; cursor: pointer; }
  .marker-buttons button.custom { color: var(--warning-text); background: var(--warning-soft); } .marker-buttons.more { margin-top: .45rem; }
  .preview { padding: .85rem 1rem; border: 1px solid var(--border); border-radius: .75rem; background: var(--surface-raised); }
  .preview p { display: flex; align-items: center; gap: .6rem; margin: .5rem 0 0; overflow-wrap: anywhere; } .preview b { color: var(--accent); font-size: 1.2rem; } .preview .muted { display: block; }
  .warning { padding: .7rem .8rem; border-radius: .6rem; color: var(--warning-text); background: var(--warning-soft); font-size: .86rem; line-height: 1.45; }
  @media (max-width: 620px) { .choice-grid.three { grid-template-columns: 1fr; } .preview p { align-items: flex-start; flex-direction: column; gap: .2rem; } }
</style>
