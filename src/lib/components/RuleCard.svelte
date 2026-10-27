<script lang="ts">
  import { onMount } from "svelte";
  import { ptBR } from "$lib/i18n/pt-BR";
  import type { Rule } from "$lib/types";
  import StatusDot from "./StatusDot.svelte";

  export type RuleStatus = "ok" | "paused" | "error";
  export type CardAction = "edit" | "test" | "duplicate" | "delete";
  export interface RuleActivity { status: RuleStatus; message?: string; lastFile?: string; lastProcessedAt?: number; }

  let { rule, activity, busy = false, onToggle, onResume, onAction }: {
    rule: Rule; activity: RuleActivity; busy?: boolean;
    onToggle: (enabled: boolean) => void; onResume: () => void; onAction: (action: CardAction) => void;
  } = $props();
  let menuOpen = $state(false);
  let now = $state(Date.now());

  const labels: Record<string, string> = { YYYY: ptBR.markerYear, YY: ptBR.markerYearShort, MM: ptBR.markerMonth, MES: ptBR.markerMonthName, DD: ptBR.markerDay, NOME: ptBR.markerName, EXT: ptBR.markerExtension };
  const sourceName = $derived(lastPart(rule.sourceDir));
  const destinationParts = $derived(rule.destination.split(/(\{[^}]+\})/g).filter(Boolean).map((part) => {
    const marker = /^\{(.+)\}$/.exec(part)?.[1];
    return marker ? { text: labels[marker] || marker, marker: true } : { text: part, marker: false };
  }));
  const fileTypes = $derived(extractTypes(rule.filter));
  const status = $derived(rule.enabled ? activity.status : "paused");
  const statusMessage = $derived(rule.enabled ? activity.message : ptBR.statusDisabled);
  const relativeTime = $derived(formatTime(activity.lastProcessedAt));
  const canResume = $derived(activity.message?.includes(ptBR.circuitBreakerHint) ?? false);

  onMount(() => {
    const timer = window.setInterval(() => (now = Date.now()), 30_000);
    return () => window.clearInterval(timer);
  });

  function lastPart(path: string) { return path.split(/[\\/]/).filter(Boolean).at(-1) || path; }
  function extractTypes(filter: string): string[] {
    const group = /\.\{([^}]+)\}$/.exec(filter)?.[1];
    const single = /\.([a-zA-Z0-9]+)$/.exec(filter)?.[1];
    const extensions = group ? group.split(",") : single ? [single] : [];
    return [...new Set((extensions.length ? extensions : [ptBR.noSpecificType]).map((extension) => {
      const upper = extension.trim().toUpperCase(); return upper === "JPEG" ? "JPG" : upper;
    }))];
  }
  function formatTime(timestamp?: number) {
    if (!timestamp) return "";
    const minutes = Math.floor(Math.max(0, now - timestamp) / 60_000);
    if (minutes < 1) return ptBR.justNow;
    if (minutes < 60) return `${ptBR.timeAgo} ${minutes} ${minutes === 1 ? ptBR.minute : ptBR.minutes}`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${ptBR.timeAgo} ${hours} ${hours === 1 ? ptBR.hour : ptBR.hours}`;
    const days = Math.floor(hours / 24);
    return `${ptBR.timeAgo} ${days} ${days === 1 ? ptBR.day : ptBR.days}`;
  }
  function choose(action: CardAction) { menuOpen = false; onAction(action); }
</script>

<svelte:window onclick={() => (menuOpen = false)} onkeydown={(event) => { if (event.key === "Escape") menuOpen = false; }} />

<article class:disabled={!rule.enabled} aria-busy={busy}>
  <div class="drag-handle" title={ptBR.dragRule} aria-hidden="true">⋮⋮</div>
  <div class="card-main">
    <div class="title-row">
      <StatusDot {status} message={statusMessage} />
      <div class="title-copy">
        <h2>{rule.name}</h2>
        <div class="route" aria-label={ptBR.sourceToDestination}>
          <strong>{sourceName}</strong><span class="arrow" aria-hidden="true">→</span>
          <span class="destination">{#each destinationParts as part}{#if part.marker}<span class="marker">{part.text}</span>{:else}<span>{part.text}</span>{/if}{/each}</span>
        </div>
      </div>
      <button class="switch" class:on={rule.enabled} type="button" role="switch" aria-checked={rule.enabled} aria-label={rule.enabled ? ptBR.disableRule : ptBR.enableRule} disabled={busy} onclick={() => onToggle(!rule.enabled)}><span></span></button>
      <div class="menu-wrap">
        <button class="menu-button" type="button" aria-label={ptBR.moreOptions} aria-haspopup="menu" aria-expanded={menuOpen} onclick={(event) => { event.stopPropagation(); menuOpen = !menuOpen; }}>•••</button>
        {#if menuOpen}<div class="menu" role="menu">
          <button type="button" role="menuitem" onclick={() => choose("edit")}>{ptBR.edit}</button>
          <button type="button" role="menuitem" onclick={() => choose("test")}>{ptBR.testRule}</button>
          <button type="button" role="menuitem" onclick={() => choose("duplicate")}>{ptBR.duplicate}</button>
          <button class="delete" type="button" role="menuitem" onclick={() => choose("delete")}>{ptBR.delete}</button>
        </div>{/if}
      </div>
    </div>
    <div class="meta-row">{#each fileTypes as type}<span class="badge">{type}</span>{/each}{#if rule.includeSubfolders}<span class="badge subfolders">{ptBR.includeSubfolders}</span>{/if}</div>
    {#if activity.status === "paused" && activity.message && rule.enabled}
      <div class="pause-message"><p>{activity.message}</p>{#if canResume}<button type="button" disabled={busy} onclick={onResume}>{ptBR.resume}</button>{/if}</div>
    {/if}
    <div class="last-file">
      <span>{ptBR.lastFile}</span>
      {#if activity.lastFile}<strong title={activity.lastFile}>{lastPart(activity.lastFile)}</strong><time datetime={new Date(activity.lastProcessedAt || 0).toISOString()}>{relativeTime}</time>
      {:else}<strong class="muted">{ptBR.noFilesYet}</strong>{/if}
    </div>
  </div>
</article>

<style>
  article { position: relative; display: flex; min-width: 0; border: 1px solid var(--border); border-radius: .95rem; background: var(--surface); box-shadow: var(--shadow-sm); transition: 150ms ease; }
  article:hover { border-color: var(--border-strong); box-shadow: var(--shadow-md); }
  article.disabled { opacity: .72; } article[aria-busy="true"] { cursor: progress; }
  .drag-handle { display: grid; place-items: center; width: 34px; flex: 0 0 auto; border-right: 1px solid var(--border); color: var(--text-faint); cursor: grab; font-size: .8rem; letter-spacing: -3px; user-select: none; }
  .card-main { min-width: 0; width: 100%; padding: 1rem 1rem .9rem; }
  .title-row { display: flex; align-items: flex-start; gap: .45rem; } .title-copy { min-width: 0; flex: 1; }
  h2 { margin: .43rem 0 .35rem; overflow: hidden; font-size: 1.02rem; line-height: 1.25; text-overflow: ellipsis; white-space: nowrap; }
  .route { display: flex; align-items: center; min-width: 0; gap: .45rem; color: var(--text-soft); font-size: .88rem; line-height: 1.65; }
  .route strong { max-width: 24%; overflow: hidden; color: var(--text); text-overflow: ellipsis; white-space: nowrap; } .arrow { color: var(--text-faint); }
  .destination { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .marker { display: inline-block; margin: 0 .1rem; padding: .05rem .35rem; border-radius: .35rem; color: var(--accent-strong); background: var(--accent-soft); font-size: .78rem; font-weight: 700; }
  .switch, .menu-button { min-width: 40px; min-height: 40px; border: 0; cursor: pointer; }
  .switch { position: relative; width: 46px; margin-top: .1rem; flex: 0 0 auto; border-radius: 999px; background: var(--switch-off); transition: 150ms ease; }
  .switch span { position: absolute; top: 9px; left: 7px; width: 22px; height: 22px; border-radius: 50%; background: white; box-shadow: 0 1px 4px rgb(0 0 0 / .24); transition: 150ms ease; }
  .switch.on { background: var(--accent); } .switch.on span { transform: translateX(10px); } .switch:disabled { cursor: wait; opacity: .55; }
  .menu-wrap { position: relative; } .menu-button { border-radius: .55rem; color: var(--text-soft); background: transparent; font-size: 1rem; letter-spacing: 1px; }
  .menu-button:hover, .menu-button:focus-visible { background: var(--surface-raised); }
  .menu { position: absolute; z-index: 15; top: 42px; right: 0; width: 160px; padding: .35rem; border: 1px solid var(--border); border-radius: .65rem; background: var(--surface); box-shadow: var(--shadow-lg); }
  .menu button { width: 100%; min-height: 40px; padding: .5rem .7rem; border: 0; border-radius: .4rem; color: var(--text); background: transparent; font: inherit; text-align: left; cursor: pointer; }
  .menu button:hover, .menu button:focus-visible { background: var(--surface-raised); } .menu .delete { color: var(--danger); }
  .meta-row { display: flex; flex-wrap: wrap; gap: .35rem; margin: .75rem 0 0 40px; }
  .badge { padding: .2rem .45rem; border: 1px solid var(--border); border-radius: 999px; color: var(--text-soft); background: var(--surface-raised); font-size: .68rem; font-weight: 750; letter-spacing: .035em; }
  .subfolders { color: var(--accent-strong); border-color: color-mix(in srgb, var(--accent) 28%, var(--border)); background: var(--accent-soft); letter-spacing: 0; }
  .pause-message { display: flex; align-items: center; gap: .75rem; margin: .8rem 0 0 40px; padding: .65rem .75rem; border-radius: .6rem; color: var(--warning-text); background: var(--warning-soft); }
  .pause-message p { flex: 1; margin: 0; font-size: .82rem; line-height: 1.4; }
  .pause-message button { min-height: 40px; padding: .5rem .75rem; border: 1px solid currentColor; border-radius: .55rem; color: inherit; background: transparent; font: inherit; font-weight: 700; cursor: pointer; }
  .last-file { display: flex; align-items: center; min-width: 0; gap: .45rem; margin: .8rem 0 0 40px; padding-top: .7rem; border-top: 1px solid var(--border); color: var(--text-faint); font-size: .77rem; }
  .last-file strong { min-width: 0; overflow: hidden; color: var(--text-soft); font-weight: 600; text-overflow: ellipsis; white-space: nowrap; } .last-file .muted { font-weight: 450; } time { margin-left: auto; flex: 0 0 auto; }
  @media (max-width: 620px) { .drag-handle { width: 26px; } .card-main { padding-inline: .7rem; } .route { display: block; } .route strong { max-width: 100%; } .arrow { margin: 0 .3rem; } .meta-row, .pause-message, .last-file { margin-left: 0; } .last-file > span { display: none; } }
</style>
