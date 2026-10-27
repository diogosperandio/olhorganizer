<script lang="ts">
  import { ptBR } from "$lib/i18n/pt-BR";
  type Status = "ok" | "paused" | "error";
  let { status, message }: { status: Status; message?: string } = $props();
  const fallback = $derived(status === "ok" ? ptBR.statusOk : status === "paused" ? ptBR.statusPaused : ptBR.statusError);
  const label = $derived(message || fallback);
</script>

<span class="status" class:ok={status === "ok"} class:paused={status === "paused"} class:error={status === "error"}>
  <button class="dot" type="button" aria-label={label}></button>
  <span class="tooltip" role="tooltip">{label}</span>
</span>

<style>
  .status { position: relative; display: inline-grid; place-items: center; width: 40px; height: 40px; flex: 0 0 auto; }
  .dot { display: grid; place-items: center; width: 40px; height: 40px; padding: 0; border: 0; background: transparent; cursor: help; }
  .dot::before { content: ""; width: 11px; height: 11px; border: 2px solid var(--surface); border-radius: 50%; background: var(--status-ok); box-shadow: 0 0 0 2px color-mix(in srgb, var(--status-ok) 24%, transparent); }
  .paused .dot::before { background: var(--status-paused); box-shadow: 0 0 0 2px color-mix(in srgb, var(--status-paused) 24%, transparent); }
  .error .dot::before { background: var(--status-error); box-shadow: 0 0 0 2px color-mix(in srgb, var(--status-error) 24%, transparent); }
  .tooltip { position: absolute; z-index: 20; top: calc(100% - 2px); right: 2px; width: max-content; max-width: 260px; padding: .45rem .65rem; border-radius: .45rem; color: var(--tooltip-text); background: var(--tooltip-bg); font-size: .78rem; line-height: 1.35; opacity: 0; pointer-events: none; transform: translateY(-3px); transition: 120ms ease; }
  .status:hover .tooltip, .status:focus-within .tooltip { opacity: 1; transform: translateY(0); }
</style>
