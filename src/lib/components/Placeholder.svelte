<script lang="ts">
  // Stand-in for screens that come in later steps.
  import { t } from "$lib/i18n";
  import Icon from "./Icon.svelte";

  let { title, message }: { title: string; message: string } = $props();
</script>

<main>
  <a class="button" href="/"><Icon name="back" />{t("nav.back")}</a>
  <h1>{title}</h1>
  <p>{message}</p>
</main>

<style>
  main {
    max-width: 860px;
    margin: 0 auto;
    padding: 24px;
  }
  h1 {
    margin: 24px 0 8px;
  }
  p {
    color: var(--muted);
  }
</style>
