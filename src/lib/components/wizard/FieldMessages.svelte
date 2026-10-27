<script lang="ts">
  // Validation messages under a field: errors in red (block saving),
  // warnings in yellow (do not). `id` lets the field point here with
  // aria-describedby.
  let { id, errors, warnings = [] }: { id?: string; errors: string[]; warnings?: string[] } =
    $props();
</script>

{#if errors.length > 0 || warnings.length > 0}
  <div {id} class="messages" aria-live="polite">
    {#each errors as message (message)}
      <p class="error">{message}</p>
    {/each}
    {#each warnings as message (message)}
      <p class="warning">{message}</p>
    {/each}
  </div>
{/if}

<style>
  .messages {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  p {
    margin: 0;
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 0.95rem;
  }
  .error {
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .warning {
    background: var(--warn-bg);
    color: var(--warn-text);
  }
</style>
