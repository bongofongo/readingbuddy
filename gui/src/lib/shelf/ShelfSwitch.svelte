<script lang="ts">
  /**
   * Picks how the wall is arranged.
   *
   * It offers whatever is in [`ARRANGEMENTS`] — it does not name them itself, so
   * the registry stays the only file that knows what exists.
   *
   * A **preference**, not a task. It shows which arrangement is on and offers
   * the others; it counts nothing, and there is nothing here to finish. What it
   * used to offer was a *layout* — covers or rows — and the difference is the
   * point: the wall's shape is settled, and where its groups fall is not.
   */
  import { ARRANGEMENTS, type ArrangementId } from './arrangements';

  let {
    current,
    onpick,
  }: { current: ArrangementId; onpick: (id: ArrangementId) => void } = $props();
</script>

<!-- Callback prop, never `createEventDispatcher` (Svelte 5). -->
<div class="switch" role="group" aria-label="Shelf arrangement">
  {#each ARRANGEMENTS as a (a.id)}
    <button
      type="button"
      class:on={a.id === current}
      aria-pressed={a.id === current}
      onclick={() => onpick(a.id)}
    >
      {a.label}
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
     The selected point is a **surface**, so it takes `--accent` with an
     `--accent-on` label: white on brass measured 2.95:1 while the *unselected*
     segment measured 5.61:1, which put the one thing this control exists to
     show on the harder side to read. */
  button.on {
    color: var(--accent-on);
    background: var(--accent);
    font-weight: 600;
  }
</style>
