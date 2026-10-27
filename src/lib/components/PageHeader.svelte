<script lang="ts">
  // Header of secondary screens: "Voltar", title and optional actions.
  import type { Snippet } from "svelte";
  import { t } from "$lib/i18n";
  import Icon from "./Icon.svelte";

  let { title, actions }: { title: string; actions?: Snippet } = $props();
</script>

<header class="top">
  <a class="button" href="/"><Icon name="back" />{t("nav.back")}</a>
  <h1>{title}</h1>
  <div class="actions">{@render actions?.()}</div>
</header>

<style>
  .top {
    position: sticky;
    top: 0;
    z-index: 20;
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 12px 24px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  h1 {
    flex: 1;
    margin: 0;
    font-size: 1.35rem;
  }
  .actions {
    display: flex;
    gap: 8px;
  }
</style>
