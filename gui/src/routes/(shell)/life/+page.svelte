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
   * ## Two tabs, and the order of them is the argument
   *
   * *Timeline* is what the page opens on: the figures, then the months as the
   * covers that closed in them. *Everything* is the full disclosure — rankings,
   * distributions and comparisons across time — and a reader **goes** there.
   *
   * `docs/decisions.md` entry 58 is the settled account. The short version: the
   * ban this page used to keep on ranking and on self-comparison was lifted, and
   * what makes that safe is exactly this ordering. The same material drawn on
   * arrival would be the app telling you how you are doing before you asked.
   * What did not move is the axiom itself — no goal, no target, no pace, nothing
   * counting what is undone, and no figure on a control.
   *
   * **Two selection dialects share this page, on purpose.** The tabs are
   * `.choices` — ink with an accent rule under the current one, the same control
   * the shell's nav and the book page's selector use — and the year rail keeps
   * its left inset, which entry 57 listed as deliberately unchanged. They are
   * not inconsistent, they are different questions: the horizontal row is *which
   * surface*, the vertical rail is *which period*, and the rail applies to both
   * surfaces. Giving them one treatment would say they were alternatives to each
   * other.
   *
   * ## The rail, and what happened in the years
   *
   * A sticky year rail on the left. The rail's
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
  import type {
    ActivitySummaryDto,
    DayActivityDto,
    MonthActivityDto,
    ReadingFilterDto,
  } from '$lib/api/bindings';
  import { client, type ReadingRow, type StoredBook } from '$lib/api/client';
  import Facets from '$lib/life/Facets.svelte';
  import Months from '$lib/life/Months.svelte';
  import { dayOf, wholeLife, yearRange, yearsOf } from '$lib/life/period';
  import Summary from '$lib/life/Summary.svelte';
  import { type View, VIEWS } from '$lib/life/views';

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
  /**
   * The same readings, unbucketed — what the periphery is built from.
   *
   * The request was already being made for the months' finishing sentence and
   * its rows were being reduced to `book` and thrown away; `ReadingRow` also
   * carries `read_number`, `ko_rating`, `passage` and the whole `BookDto`, which
   * is every panel in `Facets`. So the periphery costs **no new call** — it
   * costs keeping what was already on the wire.
   *
   * It is deliberately a second variable rather than something derived back out
   * of `finished`: that map has lost the reading, and a reread would be
   * indistinguishable from a book listed twice.
   */
  let rows = $state<ReadingRow[]>([]);
  /**
   * The days of the whole life, for the run panel — and only for it.
   *
   * Fetched once over the whole life rather than per picked year, because a run
   * is a property of the days and not of the period a reader happens to be
   * looking at; re-fetching per year would also make a run that straddles New
   * Year vanish from both of them.
   */
  let days = $state<DayActivityDto[]>([]);
  /**
   * Today, as `YYYY-MM-DD`, read once when the page loads.
   *
   * Passed down rather than read inside `longestRunOf` so the rule that a run
   * touching today is not a run is testable without mocking a clock.
   */
  let today = $state(dayOf(new Date()));

  /**
   * Which surface is up. Component state and not the URL, which is the call the
   * book page makes for its four places: the **period** is this route's subject
   * and the view is not.
   */
  let view = $state<View>('timeline');

  const years = $derived(yearsOf(months));
  const shown = $derived(year === null ? years : years.filter((y) => y.year === year));
  /**
   * The months of whatever period is picked, flat — what the trend is drawn from.
   *
   * Taken from `shown` rather than from `months`, so picking a year narrows the
   * trend with everything else. Flattened out of the year groups rather than
   * re-filtered, so there is one definition of *which months are on show*.
   */
  const shownMonths = $derived(
    // Sorted, and this is not belt-and-braces: `yearsOf` returns years
    // **newest first** with months ascending inside each, so a bare flatten
    // gives 2026's months followed by 2025's — an axis that runs forwards,
    // then jumps backwards, drawn as though it were a line through time.
    // `YYYY-MM` sorts lexically, which is the same property the fake's month
    // filter rests on.
    [...shown.flatMap((y) => y.months)].sort((a, b) => a.month.localeCompare(b.month)),
  );

  $effect(() => {
    // Read once per run rather than in each helper, so the two spans below
    // cannot straddle midnight and disagree about what "today" was.
    const today_ = new Date();
    const life = wholeLife(today_);
    const api = client();
    loaded = false;
    today = dayOf(today_);
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
    loadFinished(life).catch(() => forgetFinished());
    // The days, for the run panel alone. Its failure is not the page's — a
    // missing run is one absent panel, not a broken record.
    api
      .activityByDay(life.from, life.to)
      .then((ds) => (days = ds))
      .catch(() => (days = []));
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
    await loadFinished(span).catch(() => forgetFinished());
  }

  /**
   * Drop both views of the readings together.
   *
   * They are two shapes of one answer, so letting one survive a failure of the
   * other would leave the periphery describing a period the months no longer
   * claim.
   */
  function forgetFinished(): void {
    finished = new Map();
    rows = [];
  }

  /**
   * Which books closed in each month of a span, and the rows behind them.
   *
   * `finished_in` is the filter item 43 put on the request for exactly this
   * question, and the month key comes off the reading's own `finished_at` in
   * UTC — the engine's day convention, and the same one `dayLabel` phrases in,
   * so a book cannot appear under a month its own detail page denies.
   *
   * The rows are kept whole as well as bucketed: `rows` is what the periphery
   * reads, and bucketing loses the reading.
   */
  async function loadFinished(span: { from: string; to: string }): Promise<void> {
    const filter: ReadingFilterDto = {
      book_id: null,
      status: null,
      open: false,
      finished_in: { from: span.from, to: span.to },
    };
    const found = await client().listReadingRows({
      limit: READINGS_PAGE,
      offset: 0,
      sort: 'finished',
      filter,
    });
    rows = found;
    const by = new Map<string, StoredBook[]>();
    for (const row of found) {
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
      <nav class="years" aria-label="Which years">
        <button type="button" aria-pressed={year === null} onclick={() => pick(null)}>All</button>
        {#each years as y (y.year)}
          <button type="button" aria-pressed={year === y.year} onclick={() => pick(y.year)}>
            {y.year}
          </button>
        {/each}
      </nav>
    {:else}
      <!-- A library with one year still needs the column filled, or the record
           slides left under the rail's grid area. -->
      <div class="no-years"></div>
    {/if}

    <div class="record">
      <!--
        The two surfaces, as `.choices` — the same control the shell's nav and
        the book page's selector use, so *where you are* is said the same way
        everywhere. A `nav` because that is what it is.

        No count on either label. `docs/decisions.md` entry 51: a figure on a
        control is one decision from the badge the axiom bans, and entry 58 did
        not touch that.
      -->
      <nav class="views" aria-label="What to show">
        <div class="choices">
          {#each VIEWS as [what, label] (what)}
            <button
              class="choice"
              type="button"
              aria-pressed={view === what}
              onclick={() => (view = what)}
            >
              {label}
            </button>
          {/each}
        </div>
      </nav>

      {#if summary}
        <Summary {summary} />
      {/if}

      {#if view === 'timeline'}
        {#each shown as y (y.year)}
          <section class="band">
            <h2 class="band-title">{y.year}</h2>
            <!-- No figure beside the year heading. A year's books cannot be
                 summed out of its months (they are distinct over each), and a
                 heading carrying a wrong number is worse than one carrying
                 none — the year's own totals are what the rail above fetches. -->
            <Months months={y.months} {finished} />
          </section>
        {/each}
      {/if}
    </div>

    <!--
      *Everything*, across the full width.

      It takes the whole row rather than a column beside the record because it is
      the surface, not an annotation of one — and because the width a wide window
      has spare is what its three bands of panels are for.
    -->
  </div>

  <!--
    *Everything*, outside the grid rather than as a row of it.

    It used to span both columns, which put the sticky year rail's own column
    underneath it — and a sticky item does not reliably stop at the end of its
    grid row, so the rail painted over the panels as soon as the page scrolled.
    Out here it has the width to itself and nothing overlaps.
  -->
  {#if view === 'everything'}
    <div class="periphery">
      <Facets {rows} months={shownMonths} {days} {today} />
    </div>
  {/if}
{/if}

<style>
  /*
   * The rail, the record, and the periphery under both — centred in the window.
   *
   * The record is capped at a measure it can actually be read at and the whole
   * block is centred with `margin-inline: auto`, so a wide window gives equal
   * margins rather than the dead right-hand third the page used to have. The
   * rail keeps its column, so centring moves the record without unpinning the
   * years from it.
   *
   * `--shell` is the app's own outer bound; this cap is narrower on purpose,
   * because the thing being centred is prose-shaped and the periphery below it
   * is the part that wants the width.
   */
  .life {
    display: grid;
    grid-template-columns: 9rem minmax(0, 46rem);
    grid-template-areas: 'years record';
    gap: 0 2.5rem;
    align-items: start;
    justify-content: center;
    max-width: var(--shell);
    margin-inline: auto;
  }
  .years,
  .no-years {
    grid-area: years;
  }
  .record {
    grid-area: record;
  }
  /* Its own block under the grid, centred on the same axis and given the width
     the three bands of panels are for. */
  .periphery {
    max-width: var(--shell);
    margin: 2.2rem auto 0;
  }
  /*
   * The tabs sit above the figures, inside the record column, so they line up
   * with the thing they switch rather than with the year rail beside it.
   *
   * Every year-rail rule above is scoped to `.years` rather than to `nav`,
   * because this is the page's second `<nav>` and unscoped `nav button` was
   * repainting these tabs in the rail's inset dialect. The two dialects are
   * deliberate; CSS does not know that.
   */
  .views {
    margin-bottom: 1.4rem;
  }
  .years {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 0.1rem;
    position: sticky;
    top: 1.5rem;
  }
  .years button {
    font: inherit;
    font-size: var(--t-fine);
    text-align: start;
    padding: 0.25rem 0.5rem;
    /* Stated on both states, so selecting a year does not move the column. */
    border: 0;
    border-left: 2px solid transparent;
    background: none;
    color: var(--ink-dim);
    cursor: pointer;
  }
  .years button:hover {
    color: var(--ink);
  }
  /* The selected year: an accent inset and the raised ground behind it — the
     same pair the book page's rail uses, and the same caveat applies. The inset
     is the cue; `--bg-raised` measures Lc 0.0 against `--bg` and is nearly free. */
  .years button[aria-pressed='true'] {
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
      /* The areas have to be restated with the columns: a two-name row against
         a one-column grid is an invalid template and the whole declaration is
         dropped, which silently un-places every child. */
      grid-template-areas:
        'years'
        'record';
    }
    .years {
      position: static;
      flex-direction: row;
      flex-wrap: wrap;
      gap: 0.3rem;
      margin-bottom: 1.4rem;
    }
    .years button {
      border-left: 0;
      border-bottom: 2px solid transparent;
    }
    .years button[aria-pressed='true'] {
      border-left: 0;
      border-bottom-color: var(--accent);
    }
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
