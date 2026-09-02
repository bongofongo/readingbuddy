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
   *
   * ## It is `.choices` now, and it lives on `/cards/history`
   *
   * Two things changed in the minimal pass. The group moved with the wall to
   * `/cards/history`, because quantity is that page's purpose and was never
   * `/cards`'s — the reasoning is on the route.
   *
   * And the pills became the shell's underline treatment. Brass-filled pills
   * were the loudest thing on the screen and there were **two lit at once**,
   * side by side, saying different kinds of thing: a lit `All` is a filter and a
   * lit `Finished` is an order. The label in front of each group is what tells
   * them apart, and a fill that shouts equally on both was working against that
   * label rather than with it. Underlined, the lit member reads as *this one of
   * these* without competing with the group beside it — and the one accent fill
   * this app allows per surface is spent on an action, which neither of these
   * is.
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
    <nav class="choices" aria-label="Which cards">
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
        class="choice"
        type="button"
        aria-pressed={scope.kind === 'all'}
        onclick={() => onscope({ kind: 'all' })}
      >
        All
      </button>
      {#each years as y (y)}
        <button
          class="choice"
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
          class="choice"
          type="button"
          aria-pressed={scope.kind === 'open'}
          onclick={() => onscope({ kind: 'open' })}
        >
          Still reading
        </button>
      {/if}
    </nav>
  {/if}

  <nav class="choices order" aria-label="Order">
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
      <button class="choice" type="button" aria-pressed={sort === s} onclick={() => onsort(s)}>
        {readingSortLabel(s)}
      </button>
    {/each}
  </nav>
</div>

<style>
  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: var(--s-3) var(--s-5);
    align-items: baseline;
    margin: 0 0 var(--s-5);
  }
  /* The group's own label, in front of its members. `--t-micro` is the label
     tier and this is the app's canonical use of it: a word naming what the
     things after it are, never something read at length. */
  .what {
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  /*
   * Pushed to the far side **only where there is a far side**.
   *
   * It is what stops two groups reading as one control with two members lit —
   * less load-bearing than it was now that neither group is filled, and kept
   * because the separation is still what the two labels rely on. But
   * `margin-left: auto` survives the wrap: at 390px the order group landed alone
   * on its own row, flush right, while the heading, the years, the prose and
   * every card edge were flush left — stranded rather than deliberate. Below the
   * width where the two fit side by side they are simply two left-aligned rows,
   * which the label in front of each already tells apart.
   */
  @media (min-width: 40rem) {
    nav.order {
      margin-left: auto;
    }
  }
</style>
