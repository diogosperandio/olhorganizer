<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { exportRules, getSettings, importRules, saveSettings } from "$lib/api";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { ImportMode, Rule, Settings } from "$lib/types";
  import { checkForUpdatesNow } from "$lib/updater";

  let { onRulesImported, onNotice }: { onRulesImported: (rules: Rule[]) => void; onNotice: (message: string) => void } = $props();
  let settings = $state<Settings | null>(null);
  let loadingError = $state("");
  let version = $state("—");
  let pendingImport = $state("");
  let importing = $state(false);
  let importDialog = $state<HTMLDialogElement>();
  let saveQueue = Promise.resolve();
  let settingsRevision = 0;
  let checkingUpdates = $state(false);

  function updateSetting<K extends keyof Settings>(key: K, value: Settings[K]) {
    if (!settings) return;
    settings = { ...settings, [key]: value };
    const snapshot = { ...settings };
    const revision = ++settingsRevision;
    saveQueue = saveQueue.then(async () => {
      try {
        const saved = await saveSettings(snapshot);
        if (revision === settingsRevision) settings = saved;
        onNotice(ptBR.saved);
      } catch (error) {
        onNotice(`${ptBR.actionError} ${String(error)}`);
        if (revision === settingsRevision) {
          try { settings = await getSettings(); } catch { /* The original error is already visible. */ }
        }
      }
    });
  }

  async function exportCurrentRules() {
    try {
      const path = await save({ defaultPath: ptBR.exportFileName, filters: [{ name: ptBR.jsonFiles, extensions: ["json"] }] });
      if (!path) return;
      await exportRules(path);
      onNotice(ptBR.exportedRules);
    } catch (error) { onNotice(`${ptBR.actionError} ${String(error)}`); }
  }

  async function chooseImport() {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: ptBR.jsonFiles, extensions: ["json"] }] });
      if (typeof path === "string") pendingImport = path;
    } catch (error) { onNotice(`${ptBR.actionError} ${String(error)}`); }
  }

  async function runImport(mode: ImportMode) {
    const path = pendingImport;
    if (!path) return;
    importing = true;
    try {
      const rules = await importRules(path, mode);
      pendingImport = "";
      onRulesImported(rules);
      onNotice(ptBR.importedRules);
    } catch (error) { onNotice(`${ptBR.actionError} ${String(error)}`); }
    finally { importing = false; }
  }

  async function checkNow() {
    if (checkingUpdates) return;
    checkingUpdates = true;
    try { await checkForUpdatesNow(() => onNotice(ptBR.alreadyLatest)); }
    finally { checkingUpdates = false; }
  }

  onMount(() => {
    void Promise.all([getSettings(), getVersion()]).then(([loaded, appVersion]) => { settings = loaded; version = appVersion; }).catch((error) => (loadingError = `${ptBR.settingsLoadError} ${String(error)}`));
  });

  $effect(() => {
    if (pendingImport && importDialog && !importDialog.open) importDialog.showModal();
  });
</script>

<section class="view-heading"><div><h1>{ptBR.settingsTitle}</h1><p>{ptBR.settingsSummary}</p></div></section>

{#if loadingError}<div class="view-state error">{loadingError}</div>
{:else if !settings}<div class="view-state">{ptBR.loadingSettings}</div>
{:else}<div class="settings-list">
  <section class="settings-card">
    <div class="setting-copy"><h2>{ptBR.startupSetting}</h2><p>{ptBR.startupHelp}</p></div>
    <button class="switch" class:on={settings.startWithSystem} type="button" role="switch" aria-checked={settings.startWithSystem} aria-label={ptBR.startupSetting} onclick={() => updateSetting("startWithSystem", !settings?.startWithSystem)}><span></span></button>
  </section>
  <section class="settings-card">
    <div class="setting-copy"><h2>{ptBR.notificationsSetting}</h2><p>{ptBR.notificationsHelp}</p></div>
    <button class="switch" class:on={settings.notifications} type="button" role="switch" aria-checked={settings.notifications} aria-label={ptBR.notificationsSetting} onclick={() => updateSetting("notifications", !settings?.notifications)}><span></span></button>
  </section>
  <section class="settings-card">
    <div class="setting-copy"><h2>{ptBR.updatesSetting}</h2><p>{ptBR.updatesHelp}</p></div>
    <div class="setting-actions"><button class="secondary" type="button" disabled={checkingUpdates} onclick={checkNow}>{checkingUpdates ? ptBR.checkingUpdates : ptBR.checkNow}</button><button class="switch" class:on={settings.checkUpdatesOnStart} type="button" role="switch" aria-checked={settings.checkUpdatesOnStart} aria-label={ptBR.updatesSetting} onclick={() => updateSetting("checkUpdatesOnStart", !settings?.checkUpdatesOnStart)}><span></span></button></div>
  </section>
  <section class="settings-card transfer">
    <div class="setting-copy"><h2>{ptBR.rulesTransfer}</h2><p>{ptBR.rulesTransferHelp}</p></div>
    <div class="setting-actions"><button class="secondary" type="button" onclick={exportCurrentRules}>{ptBR.exportRules}</button><button class="secondary" type="button" onclick={chooseImport}>{ptBR.importRules}</button></div>
  </section>
  <section class="settings-card about">
    <div class="setting-copy"><h2>{ptBR.about}</h2><p>{ptBR.aboutHelp}</p></div>
    <div class="about-details"><span>{ptBR.version} {version}</span><button type="button" onclick={() => openUrl("https://github.com/diogosperandio/olhorganizer")}>{ptBR.githubPage}</button></div>
  </section>
</div>{/if}

{#if pendingImport}<dialog class="import-dialog" bind:this={importDialog} aria-labelledby="import-title" oncancel={(event) => { event.preventDefault(); pendingImport = ""; }}>
  <h2 id="import-title">{ptBR.importQuestion}</h2><p>{ptBR.importQuestionHelp}</p>
  <div class="dialog-actions"><button type="button" disabled={importing} onclick={() => (pendingImport = "")}>{ptBR.cancel}</button><button type="button" disabled={importing} onclick={() => runImport("add")}>{ptBR.addRules}</button><button class="danger" type="button" disabled={importing} onclick={() => runImport("replace")}>{ptBR.replaceRules}</button></div>
</dialog>{/if}

<style>
  .view-heading { margin-bottom: 1.1rem; } h1 { margin: 0; font-size: clamp(1.35rem, 3vw, 1.75rem); } .view-heading p { margin: .3rem 0 0; color: var(--text-soft); font-size: .9rem; }
  .settings-list { display: grid; gap: .7rem; } .settings-card { display: flex; align-items: center; justify-content: space-between; gap: 1rem; min-height: 82px; padding: .9rem 1rem; border: 1px solid var(--border); border-radius: .8rem; background: var(--surface); box-shadow: var(--shadow-sm); }
  .setting-copy { min-width: 0; } .setting-copy h2 { margin: 0; font-size: .98rem; } .setting-copy p { margin: .28rem 0 0; color: var(--text-soft); font-size: .81rem; line-height: 1.4; }
  .switch { position: relative; width: 46px; min-width: 46px; height: 40px; border: 0; border-radius: 999px; background: var(--switch-off); cursor: pointer; } .switch span { position: absolute; top: 9px; left: 7px; width: 22px; height: 22px; border-radius: 50%; background: white; box-shadow: 0 1px 4px rgb(0 0 0 / .24); transition: 150ms; } .switch.on { background: var(--accent); } .switch.on span { transform: translateX(10px); }
  .setting-actions, .about-details { display: flex; align-items: center; justify-content: flex-end; flex-wrap: wrap; gap: .5rem; } .secondary, .about-details button { min-height: 40px; padding: .5rem .7rem; border: 1px solid var(--border); border-radius: .55rem; color: var(--text); background: var(--surface-raised); font: inherit; font-weight: 650; cursor: pointer; } .secondary:disabled { opacity: .55; cursor: wait; }
  .about-details { color: var(--text-soft); font-size: .82rem; } .about-details button { color: var(--accent-strong); }
  .view-state { display: grid; place-items: center; min-height: 220px; color: var(--text-soft); } .view-state.error { color: var(--danger); }
  .import-dialog { position: fixed; width: min(460px, calc(100% - 2rem)); padding: 1.3rem; border: 0; border-radius: .9rem; color: var(--text); background: var(--surface); box-shadow: var(--shadow-lg); } .import-dialog::backdrop { background: rgb(10 20 15 / .58); } .import-dialog h2 { margin: 0; font-size: 1.1rem; } .import-dialog p { color: var(--text-soft); line-height: 1.5; }
  .dialog-actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: .5rem; margin-top: 1rem; } .dialog-actions button { min-height: 40px; padding: .55rem .75rem; border: 1px solid var(--border); border-radius: .55rem; color: var(--text); background: var(--surface-raised); font: inherit; font-weight: 700; cursor: pointer; } .dialog-actions .danger { border-color: var(--danger); color: white; background: var(--danger); } .dialog-actions button:disabled { opacity: .5; }
  @media (max-width: 620px) { .settings-card { align-items: flex-start; } .settings-card.transfer, .settings-card.about { flex-direction: column; } .setting-actions, .about-details { justify-content: flex-start; } }
</style>
