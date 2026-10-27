<script lang="ts">
  import { onMount } from "svelte";

  import { ptBR } from "$lib/i18n/pt-BR";
  import type { UpdateDialogState } from "$lib/updater";

  let { update, onInstall, onLater, onSkip }: {
    update: UpdateDialogState;
    onInstall: () => void;
    onLater: () => void;
    onSkip: () => void;
  } = $props();
  let dialog = $state<HTMLDialogElement>();
  const percent = $derived(update.total && update.total > 0 ? Math.min(100, Math.round((update.downloaded / update.total) * 100)) : null);

  function formatBytes(bytes: number): string {
    if (bytes < 1024 * 1024) return `${Math.round(bytes / 1024)} KB`;
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  }

  onMount(() => dialog?.showModal());
</script>

<dialog bind:this={dialog} aria-labelledby="update-title" oncancel={(event) => {
  event.preventDefault();
  if (update.phase === "available") onLater();
}}>
  <div class="icon" aria-hidden="true">↑</div>
  <h2 id="update-title">{ptBR.updateAvailablePrefix} {update.version} {ptBR.updateAvailableSuffix}</h2>

  {#if update.phase === "available"}
    {#if update.notes}<div class="notes"><h3>{ptBR.releaseNotes}</h3><p>{update.notes}</p></div>
    {:else}<p class="summary">{ptBR.updateReady}</p>{/if}
    {#if update.installKind === "browser"}<p class="linux-note">{ptBR.updateExternalInstall}</p>{/if}
    <div class="actions">
      <button type="button" onclick={onSkip}>{ptBR.skipVersion}</button>
      <button type="button" onclick={onLater}>{ptBR.updateLater}</button>
      <button class="primary" type="button" onclick={onInstall}>{ptBR.updateNow}</button>
    </div>
  {:else}
    <p class="summary">{update.phase === "downloading" ? ptBR.downloadingUpdate : ptBR.installingUpdate}</p>
    {#if update.phase === "downloading"}
      {#if update.total}<progress max={update.total} value={update.downloaded}></progress>
      {:else}<progress></progress>{/if}
      <p class="progress-text">
        {formatBytes(update.downloaded)}{#if update.total} / {formatBytes(update.total)} ({percent}%){/if}
      </p>
    {:else}<div class="installing" aria-hidden="true"></div>{/if}
  {/if}
</dialog>

<style>
  dialog { width: min(540px, calc(100% - 2rem)); padding: 1.4rem; border: 1px solid var(--border); border-radius: 1rem; color: var(--text); background: var(--surface); box-shadow: var(--shadow-lg); }
  dialog::backdrop { background: rgb(10 20 15 / .62); backdrop-filter: blur(2px); }
  .icon { display: grid; place-items: center; width: 44px; height: 44px; margin-bottom: .85rem; border-radius: .75rem; color: white; background: var(--accent); font-size: 1.5rem; font-weight: 800; }
  h2 { margin: 0; font-size: 1.25rem; letter-spacing: -.02em; }
  h3 { margin: 0 0 .35rem; font-size: .88rem; }
  .summary, .linux-note, .progress-text { color: var(--text-soft); line-height: 1.5; }
  .notes { max-height: 220px; margin-top: 1rem; padding: .8rem; overflow: auto; border-radius: .7rem; background: var(--surface-raised); }
  .notes p { margin: 0; color: var(--text-soft); white-space: pre-wrap; line-height: 1.5; }
  .linux-note { padding: .65rem .75rem; border: 1px solid var(--border); border-radius: .65rem; background: var(--surface-raised); font-size: .86rem; }
  .actions { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: .55rem; margin-top: 1.25rem; }
  button { min-height: 42px; padding: .6rem .8rem; border: 1px solid var(--border); border-radius: .6rem; color: var(--text); background: var(--surface-raised); font: inherit; font-weight: 700; cursor: pointer; }
  button.primary { border-color: var(--accent); color: white; background: var(--accent); }
  progress { width: 100%; height: 12px; margin-top: .7rem; accent-color: var(--accent); }
  .progress-text { margin: .35rem 0 0; font-size: .82rem; text-align: right; }
  .installing { width: 30px; height: 30px; margin: 1rem auto 0; border: 4px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin .8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
</style>
