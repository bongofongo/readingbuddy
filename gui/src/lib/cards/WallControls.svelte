<script lang="ts">
  /**
   * Which cards, and in what order — the wall's two switches.
   *
   * `/life`'s year picker and the shelf's arrangement switch, in the one place
   * that wants both. Like them it is a **preference and not a task**: it shows
   * what is on and offers the alternatives, it counts nothing, and there is
   * nothing here to finish.
   *
   * ## The years are the wall's own now (item 51)
   *
   * They used to be derived from `ActivityByMonth`, and that was a **proxy**:
   * the activity log is filled by `rb activity --refill` and by nothing
   * automatically, so a library that had never refilled offered no years at all
   * while plainly having finished books, and a year could be offered because a
   * note was written in it while no read ended. `ReadingYears` answers the
   * actual question — which years a matching reading *closed* in — under the
   * wall's own filter, so an offered year has cards behind it by construction.
   *
   * ## *Still reading* is a chip and is not a year
   *
   * An open reading has no `finished_at`, so it belongs to no year, and a wall
   * that offered only years would leave those cards reachable from *All* and
   * from nowhere else — a reader visiting every year in turn would never see the
   * book they are in the middle of. `ReadingYearsDto.open` says whether the chip
   * exists; **nothing here says how many**, which is the same refusal the wire
   * makes by carrying a bool. A number of books-in-progress on a control is one
   * decision from the badge the axiom bans, and it would read as work
   * outstanding rather than as a place to go.
   */
  import type { ReadingSortDto } from '$lib/api/bindings';
  import { SORTS, type WallScope } from './wall';
  import { readingSortLabel } from '$lib/phrasing';

  let {
    years,
    anyOpen,
    scope,
    sort,
    onscope,
    onsort,
  }: {
    /** Newest first, and every one of them has cards. */
    years: number[];
    /** Whether any reading is still open. Never a count of them. */
    anyOpen: boolean;
    scope: WallScope;
    sort: ReadingSortDto;
    onscope: (s: WallScope) => void;
    onsort: (s: ReadingSortDto) => void;
  } = $props();
</script>

<!-- Callback props, never `createEventDispatcher` (Svelte 5). -->
<div class="controls">
  {#if years.length > 0 || anyOpen}
    <nav aria-label="Which cards">
      <!--
        **A visible label here too, and item 51 is what earned it.**

        This group used to be `All / 2025 / 2024`, and the note on the Order
        group below argued — correctly, then — that a row of years is
        self-evidently a filter while three bare event-nouns are not. *Still
        reading* retires that premise: the group now ends in a text chip that
        names a state, and a review found the result reproducing the exact
        defect the last wave fixed — a lit `Still reading` beside a lit
        `Finished`, two brass pills in identical treatment saying opposite
        things, and at phone width stacked 40px apart with only the second group
        labelled. The asymmetry was the ambiguity; so is its absence now.
      -->
      <span class="what">Show</span>
      <button
        type="button"
        aria-pressed={scope.kind === 'all'}
        onclick={() => onscope({ kind: 'all' })}
      >
        All
      </button>
      {#each years as y (y)}
        <button
          type="button"
          aria-pressed={scope.kind === 'year' && scope.year === y}
          onclick={() => onscope({ kind: 'year', year: y })}
        >
          {y}
        </button>
      {/each}
      {#if anyOpen}
        <!-- Last, after the years, because it is not one of them and because it
             is the present tense on a row of past ones. Worded as the state a
             reader is in rather than as *unfinished*, which would be the same
             fact framed as something owed. -->
        <button
          type="button"
          aria-pressed={scope.kind === 'open'}
          onclick={() => onscope({ kind: 'open' })}
        >
          Still reading
        </button>
      {/if}
    </nav>
  {/if}

  <nav class="order" aria-label="Order">
    <!--
      **A visible label — and since item 51, both groups carry one.**

      The first review found *All* and *Finished* lit in identical pills with the
      only thing naming the second an order being a screen-reader `aria-label` —
      so a lit *Finished* read as *show me filtered to finished reads*, and the
      card two rows below said `Reading  p. 100 of 300`. The screen disproved its
      own control. The years were left unlabelled on the argument that
      *All / 2025 / 2024* is self-evidently a filter, which held exactly as long
      as that group was years; see the note above the group itself.
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
