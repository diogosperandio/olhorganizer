<script lang="ts">
  // Step 1 "De onde?": name, folder, which files, subfolders.
  import * as api from "$lib/api";
  import { folderName } from "$lib/format";
  import { t } from "$lib/i18n";
  import { joinFilter, splitFilter } from "$lib/rule-draft";
  import type { FilterTemplate, Rule } from "$lib/types";
  import FieldMessages from "./FieldMessages.svelte";
  import NamePartPicker from "./NamePartPicker.svelte";

  let {
    draft = $bindable(),
    templates,
    files,
    errors,
  }: {
    draft: Rule;
    templates: FilterTemplate[];
    /** Files in the chosen folder, for the name-part picker. */
    files: string[];
    errors: (field: "name" | "sourceDir" | "filter") => string[];
  } = $props();

  const BASE_TYPES = [
    { value: "pdf", label: "PDF" },
    { value: "jpg", label: "JPG/JPEG" },
    { value: "png", label: "PNG" },
    { value: "docx", label: "DOCX" },
    { value: "xlsx", label: "XLSX" },
    { value: "txt", label: "TXT" },
  ];

  let pickError = $state<string | null>(null);

  async function pickFolder() {
    pickError = null;
    try {
      const path = await api.pickFolder(t("source.pickTitle"), draft.sourceDir || undefined);
      if (!path) return;
      // Name follows the folder until the user writes their own.
      if (!draft.name.trim() || draft.name === folderName(draft.sourceDir)) {
        draft.name = folderName(path);
      }
      draft.sourceDir = path;
    } catch (e) {
      pickError = String(e);
    }
  }

  const parts = $derived(splitFilter(draft.filter));

  // A template is selected when it is exactly the filter or, for templates
  // without types (any file / date in name), when only the types differ.
  const selectedTemplate = $derived.by(() => {
    const exact = templates.find((tpl) => tpl.filter === draft.filter);
    if (exact || !parts) return exact?.filter;
    return templates.find((tpl) => {
      const tp = splitFilter(tpl.filter);
      return tp && tp.types.length === 0 && tp.pattern === parts.pattern;
    })?.filter;
  });

  const types = $derived(parts?.types ?? []);
  const typeChips = $derived([
    ...BASE_TYPES,
    ...types
      .filter((ty) => ty !== "jpeg" && !BASE_TYPES.some((b) => b.value === ty))
      .map((ty) => ({ value: ty, label: ty.toUpperCase() })),
  ]);

  function hasType(value: string) {
    return types.includes(value) || (value === "jpg" && types.includes("jpeg"));
  }

  function setTypes(next: string[]) {
    if (parts) draft.filter = joinFilter({ pattern: parts.pattern, types: next });
  }

  function toggleType(value: string) {
    setTypes(
      hasType(value) ? types.filter((ty) => ty !== value && !(value === "jpg" && ty === "jpeg")) : [...types, value],
    );
  }

  let otherType = $state("");
  function addOtherType() {
    const value = otherType.trim().replace(/^\.+/, "").toLowerCase();
    if (/^[a-z0-9]+$/.test(value) && !hasType(value)) setTypes([...types, value]);
    otherType = "";
  }
</script>

<div class="step">
  <div class="field">
    <label for="rule-name">{t("source.name")}</label>
    <input
      id="rule-name"
      type="text"
      bind:value={draft.name}
      placeholder={t("source.namePlaceholder")}
      aria-describedby="rule-name-msg"
    />
    <FieldMessages id="rule-name-msg" errors={errors("name")} />
  </div>

  <div class="field">
    <span class="field-label">{t("source.folder")}</span>
    <div class="folder">
      <button onclick={pickFolder}>{t("source.pickFolder")}</button>
      <span class="path" class:empty={!draft.sourceDir}>{draft.sourceDir || t("source.noFolder")}</span>
    </div>
    <FieldMessages errors={[...errors("sourceDir"), ...(pickError ? [pickError] : [])]} />
  </div>

  <fieldset>
    <legend>{t("source.which")}</legend>
    <div class="choices">
      {#each templates as tpl (tpl.filter)}
        <label class="choice">
          <input
            type="radio"
            name="template"
            checked={selectedTemplate === tpl.filter}
            onchange={() => (draft.filter = tpl.filter)}
          />
          {tpl.namePt}
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset>
    <legend>{t("source.types")}</legend>
    {#if parts}
      <div class="chips">
        {#each typeChips as chip (chip.value)}
          <label class="chip">
            <input type="checkbox" checked={hasType(chip.value)} onchange={() => toggleType(chip.value)} />
            {chip.label}
          </label>
        {/each}
      </div>
      <div class="other">
        <input
          type="text"
          bind:value={otherType}
          placeholder={t("source.otherTypePlaceholder")}
          aria-label={t("source.otherType")}
          onkeydown={(e) => e.key === "Enter" && addOtherType()}
        />
        <button onclick={addOtherType}>{t("source.addType")}</button>
      </div>
      <p class="help">{t("source.typesHint")}</p>
    {:else}
      <p class="help">{t("source.customFilter")}</p>
    {/if}
  </fieldset>

  <NamePartPicker {files} filter={draft.filter} onapply={(filter) => (draft.filter = filter)} />

  <details class="advanced" open={parts === null}>
    <summary>{t("source.advanced")}</summary>
    <div class="field">
      <label for="raw-filter">{t("source.rawFilter")}</label>
      <input id="raw-filter" type="text" bind:value={draft.filter} spellcheck="false" />
      <p class="help">{t("source.rawHint")}</p>
    </div>
  </details>
  <FieldMessages errors={errors("filter")} />

  <div class="subfolders">
    <button
      class="switch"
      role="switch"
      aria-checked={draft.includeSubfolders}
      aria-describedby="subfolders-help"
      onclick={() => (draft.includeSubfolders = !draft.includeSubfolders)}
    >
      <span class="track"><span class="thumb"></span></span>
      {t("source.subfolders")}
    </button>
    <p id="subfolders-help" class="help">{t("source.subfoldersHelp")}</p>
  </div>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .folder {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .path {
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .path.empty {
    font-weight: normal;
    color: var(--muted);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 40px;
    padding: 0 14px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface);
    font-weight: 600;
    cursor: pointer;
  }
  .chip:has(input:checked) {
    background: var(--chip-bg);
    color: var(--chip-text);
    border-color: var(--accent);
  }
  .chip input {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--accent);
  }
  .other {
    display: flex;
    gap: 8px;
    max-width: 320px;
  }
  .advanced {
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface);
  }
  .advanced summary {
    min-height: 44px;
    display: flex;
    align-items: center;
    padding: 0 14px;
    font-weight: 600;
    cursor: pointer;
  }
  .advanced .field {
    padding: 0 14px 14px;
  }
  .subfolders {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .switch {
    align-self: flex-start;
    gap: 12px;
    border: none;
    background: transparent;
    padding: 0 8px 0 0;
    font-weight: 600;
  }
  .track {
    position: relative;
    width: 44px;
    height: 24px;
    border-radius: 999px;
    background: var(--off);
    transition: background 0.15s;
  }
  .thumb {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.15s;
  }
  .switch[aria-checked="true"] .track {
    background: var(--ok);
  }
  .switch[aria-checked="true"] .thumb {
    transform: translateX(20px);
  }
</style>
