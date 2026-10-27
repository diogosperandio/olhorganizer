<script lang="ts">
  // The 3-step rule wizard plus "Testar e salvar". Holds the draft rule;
  // nothing is written to disk until "Salvar". Validation and the destination
  // example are refreshed shortly after each change.
  import { onMount, untrack } from "svelte";
  import { goto } from "$app/navigation";
  import * as api from "$lib/api";
  import { t, type MessageKey } from "$lib/i18n";
  import { hasDate, newDraft, splitName } from "$lib/rule-draft";
  import type { FilterTemplate, MarkerDef, Rule, RuleField, ValidationResult } from "$lib/types";
  import StepDestination, { type Preview } from "./StepDestination.svelte";
  import StepReview from "./StepReview.svelte";
  import StepSchedule from "./StepSchedule.svelte";
  import StepSource from "./StepSource.svelte";

  let { initial }: { initial: Rule | null } = $props();

  const editing = untrack(() => initial !== null);
  let draft = $state<Rule>(untrack(() => (initial ? structuredClone(initial) : newDraft())));

  const STEPS: MessageKey[] = ["wizard.step1", "wizard.step2", "wizard.step3", "wizard.step4"];
  const STEP_OF: Record<RuleField, number> = {
    name: 1,
    sourceDir: 1,
    filter: 1,
    destination: 2,
    dateSource: 2,
    checkMode: 3,
  };
  let step = $state(1);
  let maxStep = $state(editing ? STEPS.length : 1);

  let templates = $state<FilterTemplate[]>([]);
  let markers = $state<MarkerDef[]>([]);
  let validation = $state<ValidationResult>({ errors: [], warnings: [] });
  let files = $state<string[]>([]);
  let sample = $state<string | null>(null);
  let preview = $state<Preview | null>(null);
  let dateTouched = $state(editing);
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  onMount(() => {
    api.listFilterTemplates().then((list) => (templates = list)).catch(() => {});
    api.listMarkers().then((list) => (markers = list)).catch(() => {});
  });

  const DEBOUNCE_MS = 150;

  // Until the user picks one, the date comes from the name when it has one.
  $effect(() => {
    const filter = draft.filter;
    if (!untrack(() => dateTouched)) draft.dateSource = hasDate(filter) ? "file_name" : "modified";
  });

  // Files of the folder, for the name-part picker.
  $effect(() => {
    const dir = draft.sourceDir;
    const sub = draft.includeSubfolders;
    if (!dir) {
      files = [];
      return;
    }
    const timer = setTimeout(() => {
      api.listFolderFiles(dir, sub, undefined, 200).then((f) => (files = f), () => (files = []));
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  // A real file the filter accepts, for the example.
  $effect(() => {
    const dir = draft.sourceDir;
    const sub = draft.includeSubfolders;
    const filter = draft.filter;
    if (!dir) {
      sample = null;
      return;
    }
    const timer = setTimeout(() => {
      api.listFolderFiles(dir, sub, filter, 1).then((f) => (sample = f[0] ?? null), () => (sample = null));
    }, DEBOUNCE_MS);
    return () => clearTimeout(timer);
  });

  // Validation and example, after each change.
  $effect(() => {
    const snapshot = $state.snapshot(draft);
    const name = sample ?? "20261022.pdf";
    let stale = false;
    const timer = setTimeout(async () => {
      const [checked, example] = await Promise.allSettled([
        api.validateRule(snapshot),
        snapshot.destination ? api.previewDestination(snapshot, name) : Promise.resolve(null),
      ]);
      if (stale) return;
      if (checked.status === "fulfilled") validation = checked.value;
      if (example.status === "rejected") {
        preview = { sample: name, error: String(example.reason) };
      } else if (example.value === null) {
        preview = null;
      } else {
        const file = splitName(name);
        const fileName = file.ext ? `${file.stem}.${file.ext}` : file.stem;
        preview = { sample: name, folder: example.value.slice(0, -fileName.length) };
      }
    }, DEBOUNCE_MS);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  });

  function errors(field: RuleField): string[] {
    return validation.errors.filter((m) => m.field === field).map((m) => m.message);
  }

  function warnings(field: RuleField): string[] {
    return validation.warnings.filter((m) => m.field === field).map((m) => m.message);
  }

  const stepsWithErrors = $derived(new Set(validation.errors.map((m) => STEP_OF[m.field])));

  function go(target: number) {
    step = target;
    maxStep = Math.max(maxStep, target);
    saveError = null;
  }

  async function save(runNow: boolean) {
    if (validation.errors.length > 0) {
      saveError = t("wizard.fixFirst");
      go(Math.min(...stepsWithErrors));
      return;
    }
    saving = true;
    saveError = null;
    const snapshot = $state.snapshot(draft);
    try {
      const result = await api.saveRule(snapshot);
      // Restarting the rule scans its folder right away.
      if (runNow && result.rule.id) await api.resumeRule(result.rule.id);
      await goto("/");
    } catch (e) {
      saveError = t("wizard.saveFailed", { error: String(e) });
      try {
        validation = await api.validateRule(snapshot);
        if (stepsWithErrors.size > 0) step = Math.min(...stepsWithErrors);
      } catch {
        // The save error above already tells the user.
      }
    } finally {
      saving = false;
    }
  }
</script>

<header class="top">
  <h1>{editing ? t("wizard.titleEdit") : t("wizard.titleNew")}</h1>
  <nav aria-label={t("wizard.steps")}>
    <ol>
      {#each STEPS as key, i (key)}
        {@const n = i + 1}
        <li>
          <button
            class="step-link"
            class:current={step === n}
            aria-current={step === n ? "step" : undefined}
            disabled={n > maxStep}
            onclick={() => go(n)}
          >
            <span class="num" class:bad={stepsWithErrors.has(n)}>{n}</span>
            {t(key)}
            {#if stepsWithErrors.has(n)}<span class="visually-hidden">{t("wizard.stepHasProblem")}</span>{/if}
          </button>
        </li>
      {/each}
    </ol>
  </nav>
</header>

<main>
  <h2>{t(STEPS[step - 1])}</h2>
  {#if step === 1}
    <StepSource bind:draft {templates} {files} {errors} />
  {:else if step === 2}
    <StepDestination bind:draft bind:dateTouched {markers} {preview} {errors} {warnings} />
  {:else if step === 3}
    <StepSchedule bind:draft {errors} />
  {:else}
    <StepReview {draft} />
  {/if}
</main>

<footer>
  <button onclick={() => goto("/")}>{t("wizard.cancel")}</button>
  <div class="spacer"></div>
  {#if saveError}<p class="save-error" role="alert">{saveError}</p>{/if}
  {#if step > 1}
    <button onclick={() => go(step - 1)}>{t("wizard.back")}</button>
  {/if}
  {#if step < STEPS.length}
    <button class="primary" onclick={() => go(step + 1)}>{t("wizard.next")}</button>
  {:else}
    <button onclick={() => save(false)} disabled={saving}>
      {saving ? t("wizard.saving") : t("wizard.save")}
    </button>
    <button class="primary" onclick={() => save(true)} disabled={saving}>
      {t("wizard.saveAndRun")}
    </button>
  {/if}
</footer>

<style>
  .top {
    position: sticky;
    top: 0;
    z-index: 20;
    padding: 12px 24px 8px;
    background: var(--surface);
    border-bottom: 1px solid var(--border);
  }
  h1 {
    margin: 0 0 8px;
    font-size: 1.35rem;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .step-link {
    border: none;
    background: transparent;
    padding: 0 10px 0 4px;
    color: var(--muted);
  }
  .step-link.current {
    color: var(--text);
    font-weight: 700;
  }
  .step-link:disabled {
    opacity: 0.5;
  }
  .num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: 50%;
    background: var(--badge-bg);
    font-size: 0.9rem;
  }
  .current .num {
    background: var(--accent);
    color: var(--accent-text);
  }
  .num.bad {
    background: var(--danger);
    color: #fff;
  }
  main {
    max-width: 860px;
    margin: 0 auto;
    padding: 20px 24px 120px;
  }
  h2 {
    margin: 0 0 16px;
    font-size: 1.25rem;
  }
  footer {
    position: fixed;
    left: 0;
    right: 0;
    bottom: 0;
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 12px 24px;
    background: var(--surface);
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .save-error {
    margin: 0;
    padding: 6px 10px;
    border-radius: 8px;
    background: var(--danger-bg);
    color: var(--danger-text);
  }
</style>
