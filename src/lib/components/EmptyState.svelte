<script lang="ts">
  import { ptBR } from "$lib/i18n/pt-BR";
  let { onNew }: { onNew: () => void } = $props();
</script>

<section class="empty">
  <div class="folder" aria-hidden="true">⌁</div>
  <h2>{ptBR.emptyTitle}</h2>
  <p>{ptBR.emptyMessage}</p>
  <button type="button" onclick={onNew}>{ptBR.newRule}</button>
</section>

<style>
  .empty { display: grid; justify-items: center; padding: clamp(2.5rem, 10vh, 5rem) 1.5rem; border: 1px dashed var(--border-strong); border-radius: 1rem; text-align: center; background: color-mix(in srgb, var(--surface) 80%, transparent); }
  .folder { display: grid; place-items: center; width: 58px; height: 58px; margin-bottom: .9rem; border-radius: 1rem; color: var(--accent); background: var(--accent-soft); font-size: 2rem; font-weight: 700; }
  h2 { margin: 0; font-size: 1.25rem; }
  p { max-width: 430px; margin: .5rem 0 1.25rem; color: var(--text-soft); line-height: 1.5; }
  button { min-height: 44px; padding: .7rem 1.15rem; border: 0; border-radius: .7rem; color: white; background: var(--accent); font: inherit; font-weight: 700; cursor: pointer; }
</style>
