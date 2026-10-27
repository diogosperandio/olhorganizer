<script lang="ts">
  // Native modal dialog: keeps focus inside, Esc closes (calls `onclose`), and
  // focus returns to where it was. The element marked data-autofocus, if any,
  // gets focus on open.
  import type { Snippet } from "svelte";

  let {
    open,
    title,
    onclose,
    children,
    actions,
    wide = false,
  }: {
    open: boolean;
    title: string;
    onclose: () => void;
    children: Snippet;
    actions: Snippet;
    wide?: boolean;
  } = $props();

  const titleId = $props.id();
  let dialog: HTMLDialogElement;

  $effect(() => {
    if (open && !dialog.open) {
      dialog.showModal();
      dialog.querySelector<HTMLElement>("[data-autofocus]")?.focus();
    } else if (!open && dialog.open) {
      dialog.close();
    }
  });
</script>

<dialog
  bind:this={dialog}
  class:wide
  aria-labelledby={titleId}
  oncancel={(e) => {
    e.preventDefault();
    onclose();
  }}
>
  <h2 id={titleId}>{title}</h2>
  <div class="body">{@render children()}</div>
  <div class="actions">{@render actions()}</div>
</dialog>

<style>
  dialog {
    width: min(460px, calc(100vw - 32px));
    max-height: calc(100vh - 48px);
    padding: 24px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    color: var(--text);
    box-shadow: var(--shadow-lifted);
  }
  dialog.wide {
    width: min(760px, calc(100vw - 32px));
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog::backdrop {
    background: rgb(10 14 22 / 45%);
  }
  h2 {
    margin: 0 0 12px;
    font-size: 1.25rem;
  }
  .body {
    overflow: auto;
    min-height: 0;
  }
  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
    margin-top: 20px;
  }
</style>
