<script lang="ts">
  /**
   * Picks the shelf's arrangement.
   *
   * It offers whatever is in [`LAYOUTS`] — it does not name them itself, so
   * adding the spine shelf later adds a button here without editing this file.
   *
   * A **preference**, not a task. It shows which arrangement is on and offers
   * the others; it counts nothing, and there is nothing here to finish.
   */
  import { LAYOUTS, type ShelfLayoutId } from './layouts';

  let {
    current,
    onpick,
  }: { current: ShelfLayoutId; onpick: (id: ShelfLayoutId) => void } = $props();
</script>

<!-- Callback prop, never `createEventDispatcher` (Svelte 5). -->
<div class="switch" role="group" aria-label="Shelf arrangement">
  {#each LAYOUTS as l (l.id)}
    <button
      type="button"
      class:on={l.id === current}
      aria-pressed={l.id === current}
      onclick={() => onpick(l.id)}
    >
      {l.label}
    </button>
  {/each}
</div>

<style>
  .switch {
    display: inline-flex;
    gap: 1px;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg-raised);
  }
  button {
    font: inherit;
    font-size: 0.76rem;
    line-height: 1.4;
    color: var(--ink-dim);
    background: none;
    border: 0;
    border-radius: 2px;
    padding: 0.15rem 0.6rem;
    cursor: pointer;
  }
  button:hover {
    color: var(--ink);
  }
  /* State persists and is visible — the axiom's first clause, and the reason
     this is a segmented control rather than a cycling button with one label.
     The label is `--accent-on` rather than `--bg`: white on brass measured
     2.95:1 while the *unselected* segment measured 5.61:1, so the control whose
     only job is showing which state is on had the on state as the harder one to
     read. A dark label on the fill inverts that. */
  button.on {
    color: var(--accent-on);
    background: var(--accent);
    font-weight: 600;
  }
</style>
