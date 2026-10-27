<script lang="ts">
  import { t } from "$lib/i18n";
  import Modal from "./Modal.svelte";

  let {
    open,
    title,
    message,
    confirmLabel,
    danger = false,
    onconfirm,
    oncancel,
  }: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel: string;
    danger?: boolean;
    onconfirm: () => void;
    oncancel: () => void;
  } = $props();
</script>

<Modal {open} {title} onclose={oncancel}>
  <p>{message}</p>
  {#snippet actions()}
    <!-- The safe choice gets focus, so Enter never deletes by accident. -->
    <button data-autofocus onclick={oncancel}>{t("dialog.cancel")}</button>
    <button class={danger ? "danger" : "primary"} onclick={onconfirm}>{confirmLabel}</button>
  {/snippet}
</Modal>

<style>
  p {
    margin: 0;
  }
</style>
