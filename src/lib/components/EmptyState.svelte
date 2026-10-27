<script lang="ts">
  import { t } from "$lib/i18n";
  import Icon from "./Icon.svelte";

  let { oncreate }: { oncreate: () => void } = $props();
</script>

<section class="empty">
  <div class="art"><Icon name="folder" size={48} /></div>
  <p>{t("rules.empty")}</p>
  <button class="primary big" onclick={oncreate}><Icon name="plus" />{t("rules.new")}</button>
</section>

<style>
  .empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 16px;
    padding: 48px 24px;
    text-align: center;
    border: 2px dashed var(--border);
    border-radius: var(--radius);
    background: var(--surface);
  }
  .art {
    color: var(--accent);
  }
  p {
    max-width: 420px;
    margin: 0;
    font-size: 1.1rem;
    color: var(--muted);
  }
  .big {
    min-height: 48px;
    padding: 0 24px;
    font-size: 1.05rem;
  }
</style>
