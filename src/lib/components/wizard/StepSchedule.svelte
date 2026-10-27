<script lang="ts">
  // Step 3 "Quando e como?": when to check, move or copy, name conflicts.
  import { t, type MessageKey } from "$lib/i18n";
  import type { Action, ConflictPolicy, Rule } from "$lib/types";
  import FieldMessages from "./FieldMessages.svelte";

  let {
    draft = $bindable(),
    errors,
  }: { draft: Rule; errors: (field: "checkMode") => string[] } = $props();

  // Remembered while "instant" is chosen, so switching back keeps the number.
  let minutes = $state(draft.checkMode.mode === "interval" ? draft.checkMode.minutes : 5);

  function setInstant() {
    draft.checkMode = { mode: "instant" };
  }

  function setInterval() {
    draft.checkMode = { mode: "interval", minutes };
  }

  function changeMinutes(value: number) {
    minutes = Number.isFinite(value) ? Math.max(0, Math.round(value)) : 0;
    draft.checkMode = { mode: "interval", minutes };
  }

  const ACTIONS: { value: Action; label: MessageKey; help: MessageKey }[] = [
    { value: "move", label: "how.move", help: "how.moveHelp" },
    { value: "copy", label: "how.copy", help: "how.copyHelp" },
  ];
  const CONFLICTS: { value: ConflictPolicy; label: MessageKey; help: MessageKey }[] = [
    { value: "rename", label: "conflict.rename", help: "conflict.renameHelp" },
    { value: "overwrite", label: "conflict.overwrite", help: "conflict.overwriteHelp" },
    { value: "skip", label: "conflict.skip", help: "conflict.skipHelp" },
  ];
</script>

<div class="step">
  <fieldset>
    <legend>{t("when.title")}</legend>
    <div class="choices">
      <label class="choice">
        <input type="radio" name="when" checked={draft.checkMode.mode === "instant"} onchange={setInstant} />
        {t("when.instant")}
      </label>
      <label class="choice interval">
        <input type="radio" name="when" checked={draft.checkMode.mode === "interval"} onchange={setInterval} />
        <span>
          {t("when.intervalBefore")}
          <input
            type="number"
            min="1"
            value={minutes}
            aria-label={t("when.minutes")}
            onfocus={setInterval}
            oninput={(e) => changeMinutes(e.currentTarget.valueAsNumber)}
          />
          {t("when.intervalAfter")}
        </span>
      </label>
    </div>
    <FieldMessages errors={errors("checkMode")} />
  </fieldset>

  <fieldset>
    <legend>{t("how.title")}</legend>
    <div class="choices">
      {#each ACTIONS as a (a.value)}
        <label class="choice">
          <input type="radio" name="action" value={a.value} bind:group={draft.action} />
          <span>{t(a.label)}<small>{t(a.help)}</small></span>
        </label>
      {/each}
    </div>
  </fieldset>

  <fieldset>
    <legend>{t("conflict.title")}</legend>
    <div class="choices">
      {#each CONFLICTS as c (c.value)}
        <label class="choice">
          <input type="radio" name="conflict" value={c.value} bind:group={draft.onConflict} />
          <span>{t(c.label)}<small>{t(c.help)}</small></span>
        </label>
      {/each}
    </div>
  </fieldset>
</div>

<style>
  .step {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .interval span {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
  }
  .interval input[type="number"] {
    width: 80px;
  }
</style>
