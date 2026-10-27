<script lang="ts">
  // "Usar parte do nome como marcador": the user selects pieces of a sample
  // file name with the mouse; each becomes {TEXTO:x}, {NUM:x} or date markers,
  // and the rest of the name is generalized so similar files match too.
  import { untrack } from "svelte";
  import { t } from "$lib/i18n";
  import {
    dateMarkers,
    joinFilter,
    patternFromSample,
    splitFilter,
    splitName,
    validMarkerName,
    type NamePiece,
    type PieceKind,
  } from "$lib/rule-draft";

  let {
    files,
    filter,
    onapply,
  }: {
    /** Sample file names from the folder (relative paths). */
    files: string[];
    /** The current filter, whose file types are kept. */
    filter: string;
    onapply: (filter: string) => void;
  } = $props();

  let chosen = $state("");
  let typed = $state("");
  const sample = $derived(chosen || typed);
  const stem = $derived(splitName(sample).stem);

  let pieces = $state<NamePiece[]>([]);
  let selStart = $state(0);
  let selEnd = $state(0);
  let kind = $state<PieceKind>("texto");
  let name = $state("");
  let problem = $state<string | null>(null);
  let applied = $state(false);

  // A new sample starts over.
  $effect(() => {
    void sample;
    untrack(() => {
      pieces = [];
      selStart = selEnd = 0;
      problem = null;
    });
  });

  const selected = $derived(stem.slice(selStart, selEnd));

  function readSelection(e: Event) {
    const input = e.currentTarget as HTMLInputElement;
    selStart = input.selectionStart ?? 0;
    selEnd = input.selectionEnd ?? 0;
    kind = dateMarkers(selected) ? "data" : /^\d+$/.test(selected) ? "num" : "texto";
    problem = null;
  }

  function add() {
    if (!selected) {
      problem = t("piece.selectFirst");
      return;
    }
    if (pieces.some((p) => selStart < p.end && selEnd > p.start)) {
      problem = t("piece.overlap");
      return;
    }
    const markerName = name.trim();
    if (kind !== "data" && !validMarkerName(markerName)) {
      problem = t("piece.badName");
      return;
    }
    pieces = [...pieces, { start: selStart, end: selEnd, kind, name: markerName }];
    name = "";
    problem = null;
    applied = false;
  }

  const types = $derived.by(() => {
    const ext = splitName(sample).ext.toLowerCase();
    return splitFilter(filter)?.types ?? (ext ? [ext] : []);
  });
  const result = $derived(joinFilter({ pattern: patternFromSample(stem, pieces), types }));

  function apply() {
    onapply(result);
    applied = true;
  }
</script>

<details>
  <summary>{t("piece.title")}</summary>
  <div class="picker">
    <p class="help">{t("piece.help")}</p>

    {#if files.length > 0}
      <div class="field">
        <label for="piece-sample">{t("piece.sample")}</label>
        <select id="piece-sample" bind:value={chosen}>
          <option value=""></option>
          {#each files as file (file)}
            <option value={file}>{file}</option>
          {/each}
        </select>
      </div>
    {:else}
      <p class="help">{t("piece.noFiles")}</p>
    {/if}
    {#if !chosen}
      <div class="field">
        <label for="piece-typed">{t("piece.sampleTyped")}</label>
        <input id="piece-typed" type="text" bind:value={typed} placeholder="Acme_20261022.pdf" />
      </div>
    {/if}

    {#if stem}
      <input
        class="stem"
        type="text"
        readonly
        value={stem}
        aria-label={t("piece.sample")}
        onselect={readSelection}
        onmouseup={readSelection}
        onkeyup={readSelection}
      />
      <p class="help">
        {selected ? t("piece.selection", { text: selected }) : t("piece.selectFirst")}
      </p>

      {#if selected}
        <fieldset>
          <legend>{t("piece.kind")}</legend>
          <div class="kinds">
            <label class="choice"><input type="radio" bind:group={kind} value="texto" />{t("piece.kindText")}</label>
            <label class="choice"><input type="radio" bind:group={kind} value="num" />{t("piece.kindNumber")}</label>
            {#if dateMarkers(selected)}
              <label class="choice"><input type="radio" bind:group={kind} value="data" />{t("piece.kindDate")}</label>
            {/if}
          </div>
        </fieldset>
        {#if kind !== "data"}
          <div class="field">
            <label for="piece-name">{t("piece.name")}</label>
            <input id="piece-name" type="text" bind:value={name} placeholder={t("piece.namePlaceholder")} />
          </div>
        {/if}
        <div><button onclick={add}>{t("piece.add")}</button></div>
      {/if}
      {#if problem}<p class="problem" role="alert">{problem}</p>{/if}

      {#if pieces.length > 0}
        <ul class="pieces">
          {#each pieces as piece (piece.start)}
            {@const label = piece.kind === "data" ? t("piece.kindDate") : piece.name}
            <li>
              <span>{stem.slice(piece.start, piece.end)} → <strong>{label}</strong></span>
              <button
                class="remove"
                aria-label={t("piece.remove", { name: label })}
                onclick={() => (pieces = pieces.filter((p) => p !== piece))}>×</button
              >
            </li>
          {/each}
        </ul>
        <p>{t("piece.result", { filter: result })}</p>
        <div class="apply">
          <button class="primary" onclick={apply}>{t("piece.apply")}</button>
          {#if applied}<span role="status">{t("piece.applied")}</span>{/if}
        </div>
      {/if}
    {/if}
  </div>
</details>

<style>
  details {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }
  summary {
    min-height: 44px;
    display: flex;
    align-items: center;
    padding: 0 14px;
    font-weight: 600;
    cursor: pointer;
  }
  .picker {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 0 14px 14px;
  }
  .stem {
    font-family: ui-monospace, "Cascadia Mono", "DejaVu Sans Mono", monospace;
    font-size: 1.1rem;
  }
  .kinds {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .problem {
    margin: 0;
    color: var(--danger-text);
    background: var(--danger-bg);
    padding: 8px 12px;
    border-radius: 8px;
  }
  .pieces {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .pieces li {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-left: 10px;
    border-radius: 999px;
    background: var(--chip-bg);
    color: var(--chip-text);
  }
  .remove {
    width: 40px;
    padding: 0;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: inherit;
    font-size: 1.2rem;
  }
  .apply {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  p {
    margin: 0;
    overflow-wrap: anywhere;
  }
</style>
