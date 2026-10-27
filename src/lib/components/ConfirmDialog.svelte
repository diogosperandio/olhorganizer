<script lang="ts">
  import { onMount } from "svelte";
  import { ptBR } from "$lib/i18n/pt-BR";
  let { title, message, onConfirm, onCancel }: { title: string; message: string; onConfirm: () => void; onCancel: () => void } = $props();
  let dialog: HTMLDialogElement;
  onMount(() => dialog.showModal());
  function backdrop(event: MouseEvent) { if (event.target === dialog) onCancel(); }
</script>

<dialog bind:this={dialog} aria-labelledby="confirm-title" oncancel={(event) => { event.preventDefault(); onCancel(); }} onclick={backdrop}>
  <section>
    <h2 id="confirm-title">{title}</h2>
    <p>{message}</p>
    <div class="actions">
      <button class="secondary" type="button" onclick={onCancel}>{ptBR.cancel}</button>
      <button class="danger" type="button" onclick={onConfirm}>{ptBR.delete}</button>
    </div>
  </section>
</dialog>

<style>
  dialog { width: min(430px, calc(100% - 2rem)); padding: 0; border: 0; border-radius: 1rem; color: var(--text); background: var(--surface); box-shadow: var(--shadow-lg); }
  dialog::backdrop { background: rgb(15 23 42 / .52); backdrop-filter: blur(2px); }
  section { padding: 1.5rem; }
  h2 { margin: 0 0 .65rem; font-size: 1.15rem; }
  p { margin: 0; color: var(--text-soft); line-height: 1.55; }
  .actions { display: flex; justify-content: flex-end; gap: .65rem; margin-top: 1.5rem; }
  button { min-height: 40px; padding: .65rem 1rem; border: 1px solid var(--border); border-radius: .65rem; font: inherit; font-weight: 650; cursor: pointer; }
  .secondary { color: var(--text); background: var(--surface-raised); }
  .danger { border-color: var(--danger); color: white; background: var(--danger); }
</style>
