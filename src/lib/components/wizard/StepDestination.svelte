<script lang="ts" module>
  export interface Preview {
    sample: string;
    /** Destination folder, ending with a separator. */
    folder?: string;
    error?: string;
  }
</script>

<script lang="ts">
  // Step 2 "Para onde?": base folder, marker buttons, live example, date source.
  import { tick } from "svelte";
  import * as api from "$lib/api";
  import { t, type MessageKey } from "$lib/i18n";
  import { customMarkers, destinationTail, separatorOf } from "$lib/rule-draft";
  import type { DateSource, MarkerDef, Rule } from "$lib/types";
  import FieldMessages from "./FieldMessages.svelte";

  let {
    draft = $bindable(),
    dateTouched = $bindable(),
    markers,
    preview,
    errors,
    warnings,
  }: {
    draft: Rule;
    dateTouched: boolean;
    markers: MarkerDef[];
    preview: Preview | null;
    errors: (field: "destination" | "dateSource") => string[];
    warnings: (field: "destination") => string[];
  } = $props();

  const MAIN = ["YYYY", "MM", "DD", "MES", "EXT"];
  let showMore = $state(false);
  const mainMarkers = $derived(markers.filter((m) => MAIN.includes(m.token)));
  const moreMarkers = $derived(markers.filter((m) => !MAIN.includes(m.token)));
  const custom = $derived(customMarkers(draft.filter));
  const sep = $derived(separatorOf(draft.destination || draft.sourceDir));

  let input: HTMLInputElement;
  // Where marker buttons insert; null means at the end.
  let caret: number | null = null;

  function rememberCaret() {
    caret = input.selectionStart;
  }

  async function insert(text: string) {
    const value = draft.destination;
    const at = caret ?? value.length;
    draft.destination = value.slice(0, at) + text + value.slice(at);
    caret = at + text.length;
    await tick();
    input.focus();
    input.setSelectionRange(caret, caret);
  }

  let pickError = $state<string | null>(null);

  async function pickBase() {
    pickError = null;
    try {
      const path = await api.pickFolder(t("dest.pickTitle"));
      if (!path) return;
      // Keep the markers already chosen, under the new base folder.
      const base = path.replace(/[\\/]+$/, "") + separatorOf(path);
      draft.destination = base + destinationTail(draft.destination);
      caret = null;
    } catch (e) {
      pickError = String(e);
    }
  }

  function markerLabel(token: string) {
    return t(`marker.${token}` as MessageKey);
  }

  const DATE_SOURCES: { value: DateSource; key: MessageKey }[] = [
    { value: "file_name", key: "dest.dateFileName" },
    { value: "modified", key: "dest.dateModified" },
    { value: "detected", key: "dest.dateDetected" },
  ];
</script>

<div class="step">
  <div class="field">
    <span class="field-label">{t("dest.base")}</span>
    <div><button onclick={pickBase}>{t("dest.pickBase")}</button></div>
    <FieldMessages errors={pickError ? [pickError] : []} />
  </div>

  <div class="field">
    <label for="destination">{t("dest.field")}</label>
    <input
      id="destination"
      type="text"
      spellcheck="false"
      bind:this={input}
      bind:value={draft.destination}
      onkeyup={rememberCaret}
      onclick={rememberCaret}
      onselect={rememberCaret}
      oninput={rememberCaret}
      aria-describedby="destination-msg"
    />
    <div class="markers" role="group" aria-label={t("dest.markers")}>
      {#each mainMarkers as m (m.token)}
        <button class="marker" title={m.descriptionPt} onclick={() => insert(`{${m.token}}`)}>
          {markerLabel(m.token)}
        </button>
      {/each}
      {#if showMore}
        {#each moreMarkers as m (m.token)}
          <button class="marker" title={m.descriptionPt} onclick={() => insert(`{${m.token}}`)}>
            {markerLabel(m.token)}
          </button>
        {/each}
      {/if}
      {#each custom as name (name)}
        <button class="marker custom" onclick={() => insert(`{${name}}`)}>{name}</button>
      {/each}
      <button class="slash" aria-label={t("dest.subfolder")} title={t("dest.subfolder")} onclick={() => insert(sep)}>
        {sep}
      </button>
      <button class="link" aria-expanded={showMore} onclick={() => (showMore = !showMore)}>
        {showMore ? t("dest.fewer") : t("dest.more")}
      </button>
    </div>

    <p class="preview" aria-live="polite">
      <span class="label">{t("dest.preview")}:</span>
      {#if !preview}
        <span class="muted">{t("dest.previewNone")}</span>
      {:else if preview.error}
        <span class="muted">{t("dest.previewError", { error: preview.error })}</span>
      {:else}
        <span>{preview.sample}</span> → <strong>{preview.folder}</strong>
      {/if}
    </p>
    <FieldMessages id="destination-msg" errors={errors("destination")} warnings={warnings("destination")} />
  </div>

  <fieldset>
    <legend>{t("dest.dateFrom")}</legend>
    <div class="choices">
      {#each DATE_SOURCES as source (source.value)}
        <label class="choice">
          <input
            type="radio"
            name="date-source"
            value={source.value}
            bind:group={draft.dateSource}
            onchange={() => (dateTouched = true)}
          />
          {t(source.key)}
        </label>
      {/each}
    </div>
    <FieldMessages errors={errors("dateSource")} />
  </fieldset>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  #destination {
    font-size: 1.05rem;
  }
  .markers {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .marker {
    border-radius: 999px;
    background: var(--chip-bg);
    color: var(--chip-text);
    border-color: transparent;
    font-weight: 600;
  }
  .marker.custom {
    border: 1px dashed var(--chip-text);
  }
  .slash {
    min-width: 48px;
    font-weight: 700;
  }
  .link {
    border: none;
    background: transparent;
    color: var(--accent);
    text-decoration: underline;
  }
  .preview {
    margin: 0;
    padding: 10px 12px;
    border-radius: 8px;
    background: var(--badge-bg);
    overflow-wrap: anywhere;
  }
  .label {
    font-weight: 600;
  }
  .muted {
    color: var(--muted);
  }
</style>
