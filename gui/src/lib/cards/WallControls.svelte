<script lang="ts">
  /**
   * Which cards, and in what order — the wall's two switches.
   *
   * `/life`'s year picker and the shelf's arrangement switch, in the one place
   * that wants both. Like them it is a **preference and not a task**: it shows
   * what is on and offers the alternatives, it counts nothing, and there is
   * nothing here to finish.
   *
   * ## The years are borrowed, and that is a stopgap worth naming
   *
   * There is no request that answers *which years hold a finished reading*. The
   * caller derives these from `ActivityByMonth` — the same source `/life` derives
   * its years from, so the two pages offer one list rather than two — and that is
   * a **proxy**: the activity log is filled by `rb activity --refill` and by
   * nothing automatically, so a library that has never refilled offers no years
   * at all, and a year can be offered because a note was written in it while no
   * read ended. Both are visible rather than wrong: the switch is absent in the
   * first case, and the second is a real state the empty wall words honestly.
   * `docs/decisions.md` entry 47 records the engine item that would replace it.
   *
   * A year is offered even when it holds nothing, which is deliberate. Hiding
   * the empty ones would need a count per candidate year — one request each —
   * and it would also hide the sentence a reader most wants: that a year they
   * read in closed no book. That is not a failure and is not styled as one.
   */
  import type { ReadingSortDto } from '$lib/api/bindings';
  import { SORTS } from './wall';
  import { readingSortLabel } from '$lib/phrasing';

  let {
    years,
    year,
    sort,
    onyear,
    onsort,
  }: {
    years: number[];
    year: number | null;
    sort: ReadingSortDto;
    onyear: (y: number | null) => void;
    onsort: (s: ReadingSortDto) => void;
  } = $props();
</script>

<!-- Callback props, never `createEventDispatcher` (Svelte 5). -->
<div class="controls">
  {#if years.length > 0}
    <nav aria-label="Which years">
      <button type="button" aria-pressed={year === null} onclick={() => onyear(null)}>All</button>
      {#each years as y (y)}
        <button type="button" aria-pressed={year === y} onclick={() => onyear(y)}>{y}</button>
      {/each}
    </nav>
  {/if}

  <nav class="order" aria-label="Order">
    <!--
      **A visible label, and only on this group.**

      The first review found *All* and *Finished* lit in identical pills with the
      only thing naming the second an order being a screen-reader `aria-label` —
      so a lit *Finished* read as *show me finished reads*, and the card two rows
      below said `Reading  p. 100 of 300`. The screen disproved its own control.
      The years need no label because *All / 2025 / 2024* is self-evidently a
      filter; three bare event-nouns are self-evident only to somebody who
      already knows this is a sort. The asymmetry is the ambiguity, not
      sloppiness.
    -->
    <span class="what">Order</span>
    {#each SORTS as s (s)}
      <button type="button" aria-pressed={sort === s} onclick={() => onsort(s)}>
        {readingSortLabel(s)}
      </button>
    {/each}
  </nav>
</div>

<style>
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 1.6rem;
    align-items: center;
    margin: 0.4rem 0 1.6rem;
  }
  /* `/life`'s segmented switch, in the other place a small set of alternatives
     is picked between. Deliberately still not promoted to `app.css`: the shelf's
     `ShelfSwitch` is a component with its own behaviour, and these are buttons —
     sharing the look is cheaper here than sharing a widget. */
  nav {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
    align-items: center;
  }
  .what {
    font-size: 0.78rem;
    color: var(--ink-dim);
    margin-right: 0.15rem;
  }
  /*
   * Pushed to the far side **only where there is a far side**.
   *
   * The shelf's `band-head` arrangement, and it is what stops two pill groups
   * 1.6rem apart reading as one control with two segments lit. But `margin-left:
   * auto` survives the wrap: at 390px the order group landed alone on its own
   * row, flush right, while the heading, the year pills, the prose and every
   * card edge were flush left — stranded rather than deliberate, and with the
   * two gold pills now diagonally adjacent and *closer* than the desktop
   * reading relies on. Below the width where the two fit side by side they are
   * simply two left-aligned rows, which the label above already tells apart.
   */
  @media (min-width: 40rem) {
    nav.order {
      margin-left: auto;
    }
  }
  nav button {
    font: inherit;
    font-size: 0.82rem;
    padding: 0.25rem 0.7rem;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: transparent;
    color: var(--ink-dim);
    cursor: pointer;
  }
  nav button:hover {
    color: var(--ink);
  }
  /* The label is `--accent-on` rather than white: item 26 measured white on
     brass at 2.95:1, so the state whose only job is being visible was the harder
     one to read. A dark label on the fill inverts that. */
  nav button[aria-pressed='true'] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
    font-weight: 600;
  }
</style>
