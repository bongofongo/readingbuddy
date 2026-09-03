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
   *
   * ## It is `.choices` now, not a segmented control
   *
   * It used to be a bordered strip on `--bg-raised` with the selected segment
   * filled brass — a small, tidy widget, and the loudest thing on a page whose
   * whole content is jackets. The minimal pass gave the app one way of saying
   * *this one of these* (`app.css`'s `.choice`), which the shell's nav row
   * already spoke, and this is now the same three words in the same treatment.
   * The library keeps the control: quantity is this page's purpose and how the
   * quantity is arranged is the one question it asks.
   */
  import { ARRANGEMENTS, type ArrangementId } from './arrangements';

  let {
    current,
    onpick,
  }: { current: ArrangementId; onpick: (id: ArrangementId) => void } = $props();
</script>

<!-- Callback prop, never `createEventDispatcher` (Svelte 5). -->
<div class="choices" role="group" aria-label="Shelf arrangement">
  {#each ARRANGEMENTS as a (a.id)}
    <button class="choice" type="button" aria-pressed={a.id === current} onclick={() => onpick(a.id)}>
      {a.label}
    </button>
  {/each}
</div>

<style>
  /* Tighter than the shell's row: three one-word arrangements read as a set at
     this spacing, and the nav's `--s-4` between six multi-word places would make
     them look like six. Everything else — the size, the colour, the rule under
     the current one — is `app.css`'s. */
  .choices {
    gap: var(--s-3);
  }
</style>
