<script lang="ts">
  /**
   * The reading life — the one place in this app where counts are allowed.
   *
   * They are allowed **because it is a place you chose to go**. That is the
   * whole distinction the home-surface rule was sharpened to carry: a number on
   * the shelf describes one book or it does not belong there; a number here
   * describes your reading, in the past tense, on a page you opened on purpose.
   *
   * Everything on it is past tense. There is no goal, no target, no streak, no
   * badge, and nothing framed as remaining, pending or due. `activity_days` is a
   * count of days inside a range you asked about — it is **not** a streak, and a
   * "current streak" rendered from it would be a threshold announced in advance
   * in a costume. It is the nearest wrong turn on this screen and it is not
   * taken here or in either component below.
   *
   * ## Two columns now: the years, and what happened in them
   *
   * A sticky year rail on the left, months as sentences on the right. The rail's
   * years come from the **months that came back**, not from `readingYears` — and
   * that is deliberate, because the two answer different questions.
   * `readingYears` is *which years a reading closed in*, which is exactly right
   * for the wall of cards and wrong here: a year in which you wrote nine notes
   * and finished nothing is a year of your reading life, and a rail built from
   * closed readings would have no way to reach it.
   *
   * ## The calls, and why not four hundred
   *
   * `activityByMonth` over the whole life, `activitySummary` over the whole life,
   * and — when you pick a year — `activitySummary` over that year plus one
   * `listReadingRows` for the readings that closed in it. The year's figures are
   * a **request**, never a fold of its months: item 42 says `books` is distinct
   * over a period and therefore cannot be summed out of the periods inside it,
   * and `minutes: null` collapses to `0` on the first `reduce`. A summary per
   * *month* was the option item 42 rejected as sixty round trips for five years.
   *
   * The finished-books sentence has the same shape of answer: **one** request per
   * year, whose rows carry their own `finished_at`, placed into the month that
   * date names. That is placing a date, not deriving a measure — the rule item 42
   * states is that `activityByDay` must not be bucketed into months in
   * TypeScript, because a *count* over a period is not the sum of counts over its
   * parts. A reading has exactly one finish day and belongs to exactly one month.
   */
  import type { ActivitySummaryDto, MonthActivityDto, ReadingFilterDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import Months from '$lib/life/Months.svelte';
  import Summary from '$lib/life/Summary.svelte';
  import { wholeLife, yearRange, yearsOf } from '$lib/life/period';

  /** A ceiling on one year's closed readings; a year past it is short, not wrong. */
  const READINGS_PAGE = 500;

  let months = $state<MonthActivityDto[]>([]);
  let summary = $state<ActivitySummaryDto | null>(null);
  /** Three states, not a nullable: not asked, asked, answered (item 27's finding). */
  let loaded = $state(false);
  let failure = $state<string | null>(null);
  /** `null` is the whole life, which is what the page opens on. */
  let year = $state<number | null>(null);
  /** What closed in each month, keyed `YYYY-MM`. Empty until a span is asked for. */
  let finished = $state<Map<string, StoredBook[]>>(new Map());

  const years = $derived(yearsOf(months));
  const shown = $derived(year === null ? years : years.filter((y) => y.year === year));

  $effect(() => {
    // Read once per run rather than in each helper, so the two spans below
    // cannot straddle midnight and disagree about what "today" was.
    const today = new Date();
    const life = wholeLife(today);
    const api = client();
    loaded = false;
    Promise.all([api.activityByMonth(life.from, life.to), api.activitySummary(life.from, life.to)])
      .then(([ms, s]) => {
        months = ms;
        summary = s;
      })
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)))
      .finally(() => (loaded = true));
    // The books, over the same span. Its failure is **not** this page's: the
    // months and their figures are the page, and the finishing sentence is what
    // it says on top of them.
    loadFinished(life).catch(() => (finished = new Map()));
  });

  async function pick(next: number | null) {
    year = next;
    const today = new Date();
    const span = next === null ? wholeLife(today) : yearRange(next, today);
    try {
      summary = await client().activitySummary(span.from, span.to);
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    }
    await loadFinished(span).catch(() => (finished = new Map()));
  }

  /**
   * Which books closed in each month of a span.
   *
   * `finished_in` is the filter item 43 put on the request for exactly this
   * question, and the month key comes off the reading's own `finished_at` in
   * UTC — the engine's day convention, and the same one `dayLabel` phrases in,
   * so a book cannot appear under a month its own detail page denies.
   */
  async function loadFinished(span: { from: string; to: string }): Promise<void> {
    const filter: ReadingFilterDto = {
      book_id: null,
      status: null,
      open: false,
      finished_in: { from: span.from, to: span.to },
    };
    const rows = await client().listReadingRows({
      limit: READINGS_PAGE,
      offset: 0,
      sort: 'finished',
      filter,
    });
    const by = new Map<string, StoredBook[]>();
    for (const row of rows) {
      const at = row.reading.finished_at;
      if (at === null) continue;
      const key = new Date(at * 1000).toISOString().slice(0, 7);
      const bucket = by.get(key);
      if (bucket) bucket.push(row.book);
      else by.set(key, [row.book]);
    }
    finished = by;
  }
</script>

<svelte:head><title>Reading life — readingbuddy</title></svelte:head>

<h1 class="sr-only">Reading life</h1>

{#if failure}
  <p class="note">The activity log did not open: {failure}</p>
  <p class="hint">
    <code>rb activity</code> reads the same log, and <code>rb activity --refill</code> rebuilds it.
  </p>
{:else if !loaded}
  <p class="hint">Reading the log…</p>
{:else if months.length === 0}
  <!--
    Idle is not blank, and this empty state is a real one rather than a rare one.
    The log is filled by **nothing automatically** — deliberately, so it is never
    a side effect of whichever importer ran last — so a library that has never
    refilled is legitimately empty here. Naming the move is what stops it being a
    dead end, and it is the same sentence `rb activity` prints.
  -->
  <p class="note">Nothing recorded here.</p>
  <p class="hint">
    <code>rb activity --refill</code> builds the log from what you already have — your highlights,
    your notes and the days your reads began and ended.
  </p>
{:else}
  <div class="life">
    {#if years.length > 1}
      <!-- A year filtered out of the whole, which the vision names. Not a filter
           over what is *left*: every year here is a year that happened, and none
           of them carries a figure — a column of years each with a number beside
           it is a scoreboard. -->
      <nav aria-label="Which years">
        <button type="button" aria-pressed={year === null} onclick={() => pick(null)}>All</button>
        {#each years as y (y.year)}
          <button type="button" aria-pressed={year === y.year} onclick={() => pick(y.year)}>
            {y.year}
          </button>
        {/each}
      </nav>
    {:else}
      <div></div>
    {/if}

    <div class="record">
      {#if summary}
        <Summary {summary} />
      {/if}

      {#each shown as y (y.year)}
        <section class="band">
          <h2 class="band-title">{y.year}</h2>
          <!-- No figure beside the year heading. A year's books cannot be summed
               out of its months (they are distinct over each), and a heading
               carrying a wrong number is worse than one carrying none — the
               year's own totals are what the rail above fetches. -->
          <Months months={y.months} {finished} />
        </section>
      {/each}
    </div>
  </div>
{/if}

<style>
  .life {
    display: grid;
    grid-template-columns: 9rem minmax(0, 1fr);
    gap: 0 2.5rem;
    align-items: start;
  }
  nav {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0.1rem;
    position: sticky;
    top: 1.5rem;
  }
  nav button {
    font: inherit;
    font-size: 0.85rem;
    text-align: start;
    padding: 0.25rem 0.5rem;
    /* Stated on both states, so selecting a year does not move the column. */
    border: 0;
    border-left: 2px solid transparent;
    background: none;
    color: var(--ink-dim);
    cursor: pointer;
  }
  nav button:hover {
    color: var(--ink);
  }
  /* The selected year: an accent inset and the raised ground behind it — the
     same pair the book page's rail uses, and the same caveat applies. The inset
     is the cue; `--bg-raised` measures Lc 0.0 against `--bg` and is nearly free. */
  nav button[aria-pressed='true'] {
    background: var(--bg-raised);
    border-left-color: var(--accent);
    color: var(--ink);
  }
  .record {
    min-width: 0;
  }
  section.band {
    margin-top: 1.8rem;
  }
  section.band h2 {
    margin-bottom: 0.4rem;
  }
  @media (max-width: 720px) {
    .life {
      grid-template-columns: minmax(0, 1fr);
    }
    nav {
      position: static;
      flex-direction: row;
      flex-wrap: wrap;
      gap: 0.3rem;
      margin-bottom: 1.4rem;
    }
    nav button {
      border-left: 0;
      border-bottom: 2px solid transparent;
    }
    nav button[aria-pressed='true'] {
      border-left: 0;
      border-bottom-color: var(--accent);
    }
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
