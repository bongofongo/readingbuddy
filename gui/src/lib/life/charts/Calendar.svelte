<script lang="ts">
  /**
   * The days with something on them, as a calendar of weeks.
   *
   * ## Read the caveat before keeping this chart
   *
   * This is the one panel on the page whose *form* argues against the app. A
   * grid of days shaded by activity is the contribution-graph idiom, and that
   * idiom's whole social function is to make an unbroken row feel owed —
   * which is the streak `docs/decisions.md` refuses by name. Nothing here
   * computes a run, labels one, or draws today; but the shape carries a
   * suggestion the numbers do not, and that is a reason to cut it rather than a
   * reason to soften it.
   *
   * Three things hold the line as far as a form like this can:
   *
   * - **Today is not on the grid** and no cell is marked *now*. The calendar
   *   stops at the last day the engine returned, so there is no empty square at
   *   the end waiting to be filled.
   * - **An unrecorded day is empty surface, not a zero.** Only days the engine
   *   returned exist; everything else is the page showing through, which is what
   *   *nothing was recorded* looks like.
   * - **No total, no percentage, no "days covered".** The picture is the whole
   *   claim.
   *
   * ## The ramp
   *
   * Sequential: one hue, light to dark, four steps. Never a rainbow, and never a
   * second hue at the top — a heat scale that changes hue makes the reader ask
   * what the new colour *means*, and it means nothing but *more*.
   */
  import type { Cell } from '$lib/life/graphs';

  let { weeks, peak }: { weeks: (Cell | null)[][]; peak: number } = $props();

  let active = $state<Cell | null>(null);

  /**
   * Which of four steps a day sits on.
   *
   * Four rather than more because past about seven classes adjacent shades stop
   * being separable — and this scale is read at 10px, well before that.
   */
  function step(books: number): number {
    if (peak <= 1) return 4;
    return Math.max(1, Math.ceil((books / peak) * 4));
  }
</script>

<div class="cal">
  <div class="grid" role="img" aria-label="Days with something on them">
    {#each weeks as week, w (w)}
      <div class="week">
        {#each week as cell, d (d)}
          {#if cell === null}
            <span class="cell empty"></span>
          {:else}
            <span
              class="cell s{step(cell.books)}"
              role="button"
              tabindex="0"
              aria-label={`${cell.day}: ${cell.books} ${cell.books === 1 ? 'book' : 'books'}`}
              onmouseenter={() => (active = cell)}
              onmouseleave={() => (active = null)}
              onfocus={() => (active = cell)}
              onblur={() => (active = null)}
            ></span>
          {/if}
        {/each}
      </div>
    {/each}
  </div>

  <p class="readout" aria-live="polite">
    {#if active}
      <strong>{active.books} {active.books === 1 ? 'book' : 'books'}</strong>
      <span>{active.day}</span>
    {/if}
  </p>
</div>

<style>
  /*
   * Weeks run left to right and the block scrolls sideways when there are more
   * of them than fit.
   *
   * It used to wrap, which turned three years into a tall column of week-strips
   * that read as a texture rather than as a calendar — a year is a horizontal
   * thing and wrapping it breaks the only axis it has. Wide content scrolls
   * inside its own container; it does not reflow into a different shape.
   */
  .grid {
    display: flex;
    /* The 2px surface gap doing the separating, in both directions. */
    gap: 2px;
    flex-wrap: nowrap;
    overflow-x: auto;
    padding-bottom: 0.3rem;
  }
  .week {
    flex: none;
    display: grid;
    grid-template-rows: repeat(7, 10px);
    gap: 2px;
  }
  .cell {
    display: block;
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  /* Empty surface, not a zero: nothing was recorded on this day. */
  .empty {
    background: var(--line);
    opacity: 0.35;
  }
  /* One hue, four steps, light to dark. */
  .s1 {
    background: color-mix(in srgb, var(--accent) 28%, var(--bg));
  }
  .s2 {
    background: color-mix(in srgb, var(--accent) 52%, var(--bg));
  }
  .s3 {
    background: color-mix(in srgb, var(--accent) 76%, var(--bg));
  }
  .s4 {
    background: var(--accent);
  }
  .cell:hover,
  .cell:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .readout {
    min-height: 1.35rem;
    margin: 0.45rem 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  .readout strong {
    color: var(--ink);
    font-weight: 600;
    margin-right: 0.4rem;
  }
</style>
