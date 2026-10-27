<script lang="ts">
  // "Configurações": every change is saved at once (with a "Salvo" toast).
  import { onMount } from "svelte";
  import * as api from "$lib/api";
  import { t, type MessageKey } from "$lib/i18n";
  import { GITHUB_URL } from "$lib/links";
  import type { ImportMode, Settings } from "$lib/types";
  import Modal from "./Modal.svelte";
  import PageHeader from "./PageHeader.svelte";
  import Toast from "./Toast.svelte";

  type Toggle = "startWithSystem" | "notifications" | "checkUpdatesOnStart";

  const TOGGLES: { key: Toggle; label: MessageKey; help: MessageKey }[] = [
    { key: "startWithSystem", label: "settings.startWithSystem", help: "settings.startWithSystemHelp" },
    { key: "notifications", label: "settings.notifications", help: "settings.notificationsHelp" },
    { key: "checkUpdatesOnStart", label: "settings.updates", help: "settings.updatesHelp" },
  ];

  let settings = $state<Settings | null>(null);
  let version = $state("");
  let toast = $state<string | null>(null);
  let problem = $state<string | null>(null);

  // Import: the checked file waiting for "Substituir" / "Adicionar às atuais".
  let pending = $state<{ path: string; names: string[] } | null>(null);
  let importing = $state(false);

  onMount(() => {
    api.getSettings().then((s) => (settings = s), (e) => (problem = String(e)));
    api.appVersion().then((v) => (version = v), () => {});
  });

  async function toggle(key: Toggle) {
    if (!settings) return;
    const previous = settings;
    const next = { ...settings, [key]: !settings[key] };
    settings = next;
    try {
      settings = await api.saveSettings(next);
      toast = t("toast.saved");
    } catch (e) {
      settings = previous;
      problem = String(e);
    }
  }

  async function exportRules() {
    try {
      const path = await api.pickSaveJson(t("settings.exportTitle"), t("settings.exportFile"));
      if (!path) return;
      const count = await api.exportRules(path);
      toast = t("settings.exported", { count });
    } catch (e) {
      problem = String(e);
    }
  }

  async function chooseImport() {
    try {
      const path = await api.pickJsonFile(t("settings.importTitle"));
      if (!path) return;
      // Checked first: an invalid file is reported and nothing changes.
      const summary = await api.inspectImport(path);
      pending = { path, names: summary.ruleNames };
    } catch (e) {
      problem = String(e);
    }
  }

  async function runImport(mode: ImportMode) {
    if (!pending) return;
    importing = true;
    try {
      const warnings = await api.importRules(pending.path, mode);
      pending = null;
      toast = t("settings.imported");
      if (warnings.length > 0) problem = warnings.join(" ");
    } catch (e) {
      pending = null;
      problem = String(e);
    } finally {
      importing = false;
    }
  }

  async function openGithub(e: MouseEvent) {
    e.preventDefault();
    try {
      await api.openWebPage(GITHUB_URL);
    } catch (err) {
      problem = String(err);
    }
  }
</script>

<PageHeader title={t("settings.title")} />

<main>
  <section>
    <h2>{t("settings.general")}</h2>
    {#if settings}
      {#each TOGGLES as item (item.key)}
        <div class="row">
          <div class="text">
            <span class="label" id="label-{item.key}">{t(item.label)}</span>
            <p class="help" id="help-{item.key}">{t(item.help)}</p>
          </div>
          <button
            class="switch"
            role="switch"
            aria-checked={settings[item.key]}
            aria-labelledby="label-{item.key}"
            aria-describedby="help-{item.key}"
            onclick={() => toggle(item.key)}
          >
            <span class="track"><span class="thumb"></span></span>
          </button>
        </div>
        {#if item.key === "checkUpdatesOnStart"}
          <div class="row sub">
            <button onclick={() => (toast = t("settings.checkNowSoon"))}>{t("settings.checkNow")}</button>
          </div>
        {/if}
      {/each}
    {/if}
  </section>

  <section>
    <h2>{t("settings.rules")}</h2>
    <div class="row">
      <div class="text">
        <span class="label">{t("settings.export")}</span>
        <p class="help">{t("settings.exportHelp")}</p>
      </div>
      <button onclick={exportRules}>{t("settings.export")}</button>
    </div>
    <div class="row">
      <div class="text">
        <span class="label">{t("settings.import")}</span>
        <p class="help">{t("settings.importHelp")}</p>
      </div>
      <button onclick={chooseImport}>{t("settings.import")}</button>
    </div>
  </section>

  <section>
    <h2>{t("settings.about")}</h2>
    <p>{t("app.name")} — {version ? t("settings.version", { version }) : ""}</p>
    <p><a href={GITHUB_URL} onclick={openGithub}>{t("settings.github")}</a></p>
  </section>
</main>

<Modal open={pending !== null} title={t("settings.import")} onclose={() => (pending = null)}>
  {#if pending}
    <p>{t("settings.importAsk", { count: pending.names.length })}</p>
    <ul>
      {#each pending.names as name, i (i)}<li>{name}</li>{/each}
    </ul>
    <p>{t("settings.importQuestion")}</p>
  {/if}
  {#snippet actions()}
    <button data-autofocus onclick={() => (pending = null)} disabled={importing}>{t("dialog.cancel")}</button>
    <button onclick={() => runImport("replace")} disabled={importing}>{t("settings.importReplace")}</button>
    <button class="primary" onclick={() => runImport("add")} disabled={importing}>{t("settings.importAdd")}</button>
  {/snippet}
</Modal>

<Toast message={toast} onclose={() => (toast = null)} />
<Toast message={problem} error onclose={() => (problem = null)} />

<style>
  main {
    max-width: 760px;
    margin: 0 auto;
    padding: 16px 24px 80px;
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  section {
    padding: 16px 20px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  h2 {
    margin: 0 0 8px;
    font-size: 1.1rem;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 0;
  }
  .row + .row {
    border-top: 1px solid var(--border);
  }
  .row.sub {
    border-top: none;
    padding-top: 0;
  }
  .label {
    font-weight: 600;
  }
  section p {
    margin: 4px 0;
  }
  a {
    color: var(--accent);
  }
  ul {
    margin: 0 0 12px;
    padding-left: 20px;
  }
  .switch {
    flex: none;
    min-width: 56px;
    padding: 0 6px;
    border: none;
    background: transparent;
  }
  .switch:hover {
    background: var(--hover);
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
