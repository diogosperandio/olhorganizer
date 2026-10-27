<script lang="ts">
  // Short confirmation at the bottom of the screen ("Salvo"); hides itself.
  import { t } from "$lib/i18n";

  let {
    message,
    onclose,
    error = false,
  }: { message: string | null; onclose: () => void; error?: boolean } = $props();

  $effect(() => {
    if (!message) return;
    // Errors stay longer, so they can be read.
    const timer = setTimeout(onclose, error ? 8000 : 2000);
    return () => clearTimeout(timer);
  });
</script>

<div class="region" role={error ? "alert" : "status"} aria-live={error ? "assertive" : "polite"}>
  {#if message}
    <div class="toast" class:error>
      <span>{message}</span>
      {#if error}
        <button aria-label={t("toast.close")} onclick={onclose}>×</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .region {
    position: fixed;
    left: 50%;
    bottom: 24px;
    transform: translateX(-50%);
    z-index: 50;
    max-width: calc(100vw - 32px);
  }
  .toast {
    display: flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    padding: 0 18px;
    border-radius: 999px;
    background: var(--text);
    color: var(--bg);
    font-weight: 600;
    box-shadow: var(--shadow-lifted);
  }
  .toast.error {
    border-radius: 12px;
    padding: 8px 8px 8px 16px;
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  button {
    width: 40px;
    padding: 0;
    border: none;
    background: transparent;
    color: inherit;
    font-size: 1.3rem;
  }
</style>
