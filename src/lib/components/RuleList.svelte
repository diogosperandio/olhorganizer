<script lang="ts">
  import type { Rule } from "$lib/types";
  import RuleCard, { type CardAction, type RuleActivity } from "./RuleCard.svelte";
  let { rules, activities, busyIds, onToggle, onResume, onAction, onReorder }: {
    rules: Rule[]; activities: Record<string, RuleActivity>; busyIds: Set<string>;
    onToggle: (rule: Rule, enabled: boolean) => void; onResume: (rule: Rule) => void;
    onAction: (rule: Rule, action: CardAction) => void; onReorder: (ids: string[]) => void;
  } = $props();
  let draggedId = $state<string | null>(null); let overId = $state<string | null>(null);
  function reset() { draggedId = null; overId = null; }
  function drop(targetId: string) {
    if (!draggedId || draggedId === targetId) return reset();
    const ids = rules.map((rule) => rule.id); const from = ids.indexOf(draggedId); const to = ids.indexOf(targetId);
    if (from < 0 || to < 0) return reset();
    const moved = ids.splice(from, 1)[0]; ids.splice(to, 0, moved); onReorder(ids); reset();
  }
</script>

<div class="list" role="list">{#each rules as rule (rule.id)}
  <div class="item" role="listitem" class:dragging={draggedId === rule.id} class:over={overId === rule.id && draggedId !== rule.id} draggable="true"
    ondragstart={(event) => { draggedId = rule.id; event.dataTransfer?.setData("text/plain", rule.id); }}
    ondragover={(event) => { event.preventDefault(); overId = rule.id; }} ondrop={(event) => { event.preventDefault(); drop(rule.id); }} ondragend={reset}>
    <RuleCard {rule} activity={activities[rule.id] || { status: rule.enabled ? "ok" : "paused" }} busy={busyIds.has(rule.id)}
      onToggle={(enabled) => onToggle(rule, enabled)} onResume={() => onResume(rule)} onAction={(action) => onAction(rule, action)} />
  </div>
{/each}</div>

<style>
  .list { display: grid; gap: .75rem; } .item { border-radius: .95rem; transition: 120ms ease; } .item.dragging { opacity: .42; }
  .item.over { transform: translateY(4px); outline: 2px solid var(--accent); outline-offset: 2px; }
</style>
