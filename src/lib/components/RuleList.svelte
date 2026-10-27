<script lang="ts">
  // Rule cards in priority order. Reorder by dragging the handle (pointer
  // events, so it works the same in every WebView) or with the arrow keys on
  // the focused handle. Moves are tracked on the window rather than with
  // pointer capture, because reordering moves the dragged node in the DOM.
  import { flip } from "svelte/animate";
  import { t } from "$lib/i18n";
  import type { Rule, RuleStatus, Uuid } from "$lib/types";
  import Icon from "./Icon.svelte";
  import RuleCard from "./RuleCard.svelte";

  let {
    rules,
    statuses,
    now,
    onreorder,
    ontoggle,
    onedit,
    ontest,
    onduplicate,
    ondelete,
    onresume,
  }: {
    rules: Rule[];
    statuses: Record<Uuid, RuleStatus>;
    now: number;
    /** Called with every rule id in the new order. */
    onreorder: (ids: Uuid[]) => void;
    ontoggle: (rule: Rule, enabled: boolean) => void;
    onedit: (rule: Rule) => void;
    ontest: (rule: Rule) => void;
    onduplicate: (rule: Rule) => void;
    ondelete: (rule: Rule) => void;
    onresume: (rule: Rule) => void;
  } = $props();

  let dragId = $state<Uuid | null>(null);
  let order = $state<Rule[]>([]);
  const nodes: Record<Uuid, HTMLElement> = {};

  // While dragging, the list shows the preview order; otherwise the props.
  const items = $derived(dragId === null ? rules : order);

  function idOf(rule: Rule): Uuid {
    return rule.id ?? "";
  }

  function startDrag(e: PointerEvent, rule: Rule) {
    if (e.button !== 0) return;
    e.preventDefault();
    order = [...rules];
    dragId = idOf(rule);
  }

  function moveDrag(e: PointerEvent) {
    if (dragId === null) return;
    const from = order.findIndex((r) => idOf(r) === dragId);
    const to = order.filter((r) => {
      if (idOf(r) === dragId) return false;
      const box = nodes[idOf(r)]?.getBoundingClientRect();
      return box !== undefined && box.top + box.height / 2 < e.clientY;
    }).length;
    if (from !== to) {
      const next = [...order];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      order = next;
    }
  }

  function endDrag() {
    if (dragId === null) return;
    const ids = order.map(idOf);
    const changed = ids.join() !== rules.map(idOf).join();
    // Report before leaving drag mode, so the parent's new order shows at once.
    if (changed) onreorder(ids);
    dragId = null;
  }

  function keyMove(e: KeyboardEvent, rule: Rule) {
    const step = e.key === "ArrowUp" ? -1 : e.key === "ArrowDown" ? 1 : 0;
    if (step === 0) return;
    e.preventDefault();
    const from = rules.indexOf(rule);
    const to = from + step;
    if (to < 0 || to >= rules.length) return;
    const ids = rules.map(idOf);
    [ids[from], ids[to]] = [ids[to], ids[from]];
    onreorder(ids);
  }
</script>

<svelte:window onpointermove={moveDrag} onpointerup={endDrag} onpointercancel={endDrag} />

<ul aria-label={t("rules.listLabel")} class:sorting={dragId !== null}>
  {#each items as rule (idOf(rule))}
    <li
      bind:this={nodes[idOf(rule)]}
      class:dragging={dragId === idOf(rule)}
      animate:flip={{ duration: 150 }}
    >
      <RuleCard
        {rule}
        status={statuses[idOf(rule)]}
        {now}
        ontoggle={(enabled) => ontoggle(rule, enabled)}
        onedit={() => onedit(rule)}
        ontest={() => ontest(rule)}
        onduplicate={() => onduplicate(rule)}
        ondelete={() => ondelete(rule)}
        onresume={() => onresume(rule)}
      >
        {#snippet handle()}
          <button
            class="handle"
            aria-label={t("card.dragHandle", { name: rule.name })}
            onpointerdown={(e) => startDrag(e, rule)}
            onkeydown={(e) => keyMove(e, rule)}
          >
            <Icon name="grip" />
          </button>
        {/snippet}
      </RuleCard>
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  ul.sorting {
    user-select: none;
  }
  li.dragging {
    position: relative;
    z-index: 5;
    opacity: 0.85;
  }
  li.dragging :global(.card) {
    box-shadow: var(--shadow-lifted);
  }
  .handle {
    width: 32px;
    min-width: 32px;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--muted);
    cursor: grab;
    touch-action: none;
  }
  .handle:hover {
    background: var(--hover);
  }
  li.dragging .handle {
    cursor: grabbing;
  }
</style>
