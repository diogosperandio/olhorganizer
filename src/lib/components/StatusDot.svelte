<script lang="ts" module>
  export type DotState = "ok" | "paused" | "error" | "off";
</script>

<script lang="ts">
  // Colored dot with a pt-BR tooltip; screen readers get the same text.
  let { state, label }: { state: DotState; label: string } = $props();
</script>

<span class="dot {state}" title={label} role="img" aria-label={label}></span>

<style>
  .dot {
    display: inline-block;
    flex: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--surface);
    box-shadow: 0 0 0 1px var(--border);
  }
  .ok {
    background: var(--ok);
  }
  .paused {
    background: var(--warn);
  }
  .error {
    background: var(--danger);
  }
  .off {
    background: var(--off);
  }
</style>
