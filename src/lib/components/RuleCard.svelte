<script lang="ts">
  import type { Snippet } from "svelte";
  import { destinationParts, fileTypes, folderName, relativeTime } from "$lib/format";
  import { t } from "$lib/i18n";
  import type { Rule, RuleStatus } from "$lib/types";
  import Icon from "./Icon.svelte";
  import StatusDot, { type DotState } from "./StatusDot.svelte";

  let {
    rule,
    status,
    now,
    handle,
    ontoggle,
    onedit,
    ontest,
    onduplicate,
    ondelete,
    onresume,
  }: {
    rule: Rule;
    status: RuleStatus | undefined;
    now: number;
    /** The drag handle, rendered by the list. */
    handle: Snippet;
    ontoggle: (enabled: boolean) => void;
    onedit: () => void;
    ontest: () => void;
    onduplicate: () => void;
    ondelete: () => void;
    onresume: () => void;
  } = $props();

  const dot: DotState = $derived(rule.enabled ? (status?.state ?? "ok") : "off");
  const message = $derived(rule.enabled && dot !== "ok" ? status?.messagePt : null);
  const dotLabel = $derived(
    { ok: t("status.ok"), off: t("status.off"), paused: t("status.paused"), error: t("status.error") }[
      dot
    ] + (message ? `: ${message}` : ""),
  );
  const types = $derived(fileTypes(rule.filter));
  const parts = $derived(destinationParts(rule.destination));

  // Menu (⋯): opens below its button; Esc or a click outside closes it.
  let menuOpen = $state(false);
  let menuButton: HTMLButtonElement;
  let menu = $state<HTMLElement>();
  const menuId = $props.id();

  function openMenu() {
    menuOpen = true;
    queueMicrotask(() => menu?.querySelector<HTMLElement>("[role=menuitem]")?.focus());
  }

  function closeMenu(returnFocus = true) {
    menuOpen = false;
    if (returnFocus) menuButton.focus();
  }

  function choose(action: () => void) {
    closeMenu(false);
    action();
  }

  function menuKeys(e: KeyboardEvent) {
    const items = [...(menu?.querySelectorAll<HTMLElement>("[role=menuitem]") ?? [])];
    const index = items.indexOf(document.activeElement as HTMLElement);
    if (e.key === "Escape") {
      e.preventDefault();
      closeMenu();
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      items[(index + 1) % items.length]?.focus();
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      items[(index - 1 + items.length) % items.length]?.focus();
    } else if (e.key === "Tab") {
      closeMenu(false);
    }
  }

  function outside(e: PointerEvent) {
    if (menuOpen && !menu?.contains(e.target as Node) && !menuButton.contains(e.target as Node)) {
      closeMenu(false);
    }
  }
</script>

<svelte:window onpointerdown={outside} />

<article class="card" class:off={!rule.enabled}>
  <div class="grip">{@render handle()}</div>

  <div class="main">
    <header>
      <StatusDot state={dot} label={dotLabel} />
      <h2>{rule.name}</h2>
    </header>

    <p class="route">
      <span class="source"><Icon name="folder" size={18} />{folderName(rule.sourceDir)}</span>
      <span class="arrow"><Icon name="arrow" size={18} /><span class="visually-hidden">{t("card.to")}</span></span>
      <span class="destination">
        {#each parts as part, i (i)}
          {#if part.kind === "text"}{part.text}{:else}<span class="chip">{part.label}</span>{/if}
        {/each}
      </span>
    </p>

    <div class="badges">
      {#each types as type (type)}
        <span class="badge">{type}</span>
      {:else}
        <span class="badge">{t("card.anyType")}</span>
      {/each}
      {#if rule.includeSubfolders}
        <span class="badge sub">{t("card.subfolders")}</span>
      {/if}
    </div>

    {#if message}
      <div class="message {dot}" role="status">
        <span>{message}</span>
        {#if status?.canResume}
          <button onclick={onresume}>{t("card.resume")}</button>
        {/if}
      </div>
    {/if}

    <p class="last">
      {#if status?.lastFile && status.lastAt}
        {t("card.lastFile", { file: status.lastFile })} ·
        <time datetime={new Date(status.lastAt).toISOString()}>{relativeTime(status.lastAt, now)}</time>
      {:else}
        {t("card.lastFileNone")}
      {/if}
    </p>
  </div>

  <div class="controls">
    <button
      class="switch"
      role="switch"
      aria-checked={rule.enabled}
      aria-label={t("card.switchOn", { name: rule.name })}
      onclick={() => ontoggle(!rule.enabled)}
    >
      <span class="track"><span class="thumb"></span></span>
    </button>

    <div class="menu-wrap">
      <button
        class="icon"
        bind:this={menuButton}
        aria-label={t("card.menu", { name: rule.name })}
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        aria-controls={menuOpen ? menuId : undefined}
        onclick={() => (menuOpen ? closeMenu() : openMenu())}
      >
        <Icon name="more" />
      </button>
      {#if menuOpen}
        <div class="menu" role="menu" id={menuId} tabindex="-1" bind:this={menu} onkeydown={menuKeys}>
          <button role="menuitem" onclick={() => choose(onedit)}>{t("menu.edit")}</button>
          <button role="menuitem" onclick={() => choose(ontest)}>{t("menu.test")}</button>
          <button role="menuitem" onclick={() => choose(onduplicate)}>{t("menu.duplicate")}</button>
          <button role="menuitem" class="delete" onclick={() => choose(ondelete)}>
            {t("menu.delete")}
          </button>
        </div>
      {/if}
    </div>
  </div>
</article>

<style>
  .card {
    display: flex;
    gap: 12px;
    padding: 16px;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
  }
  .card.off .main {
    opacity: 0.65;
  }
  .grip {
    display: flex;
    align-items: flex-start;
  }
  .main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  h2 {
    margin: 0;
    font-size: 1.15rem;
    overflow-wrap: anywhere;
  }
  .route {
    margin: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    overflow-wrap: anywhere;
  }
  .source {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
  }
  .arrow {
    display: inline-flex;
    color: var(--muted);
  }
  .destination {
    min-width: 0;
  }
  .chip {
    display: inline-block;
    margin: 0 2px;
    padding: 1px 8px;
    border-radius: 999px;
    background: var(--chip-bg);
    color: var(--chip-text);
    font-size: 0.9rem;
    font-weight: 600;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .badge {
    padding: 2px 8px;
    border-radius: 6px;
    background: var(--badge-bg);
    color: var(--badge-text);
    font-size: 0.8rem;
    font-weight: 600;
    letter-spacing: 0.02em;
  }
  .badge.sub {
    background: var(--chip-bg);
    color: var(--chip-text);
  }
  .message {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 8px;
  }
  .message.paused {
    background: var(--warn-bg);
    color: var(--warn-text);
  }
  .message.error {
    background: var(--danger-bg);
    color: var(--danger-text);
  }
  .message button {
    flex: none;
  }
  .last {
    margin: 0;
    color: var(--muted);
    font-size: 0.92rem;
    overflow-wrap: anywhere;
  }
  .controls {
    display: flex;
    align-items: flex-start;
    gap: 4px;
  }

  .switch {
    min-width: 56px;
    padding: 0 6px;
    border: none;
    background: transparent;
  }
  .switch:hover {
    background: var(--hover);
  }
  .track {
    position: relative;
    width: 44px;
    height: 24px;
    border-radius: 999px;
    background: var(--off);
    transition: background 0.15s;
  }
  .thumb {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    transition: transform 0.15s;
  }
  .switch[aria-checked="true"] .track {
    background: var(--ok);
  }
  .switch[aria-checked="true"] .thumb {
    transform: translateX(20px);
  }

  .icon {
    width: 40px;
    padding: 0;
    border: none;
    background: transparent;
  }
  .icon:hover {
    background: var(--hover);
  }
  .menu-wrap {
    position: relative;
  }
  .menu {
    position: absolute;
    right: 0;
    top: calc(100% + 4px);
    z-index: 10;
    min-width: 180px;
    padding: 6px;
    display: flex;
    flex-direction: column;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: 10px;
    box-shadow: var(--shadow-lifted);
  }
  .menu button {
    justify-content: flex-start;
    border: none;
    background: transparent;
  }
  .menu button:hover,
  .menu button:focus-visible {
    background: var(--hover);
  }
  .menu .delete {
    color: var(--danger);
  }
</style>
