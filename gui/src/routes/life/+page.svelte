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
   * ## Three calls, and why not four hundred
   *
   * `activityByMonth` over the whole life, `activitySummary` over the whole
   * life, and — when you pick a year — `activitySummary` over that year. The
   * year's figures are a **request**, never a fold of its months: item 42 says
   * `books` is distinct over a period and therefore cannot be summed out of the
   * periods inside it, and `minutes: null` collapses to `0` on the first
   * `reduce`. A summary per *month* was the option item 42 rejected as sixty
   * round trips for five years; a summary per *year* is five, and each answers
   * exactly the question its heading asks.
   *
   * The years themselves come from the months that came back, which is a view of
   * the data rather than a derivation of a rule — there is no request that says
   * which years a library has, and asking for a fixed five would be this page
   * deciding when a reading life began.
   */
  import type { ActivitySummaryDto, MonthActivityDto } from '$lib/api/bindings';
  import { client } from '$lib/api/client';
  import Months from '$lib/life/Months.svelte';
  import Summary from '$lib/life/Summary.svelte';
  import { wholeLife, yearRange, yearsOf } from '$lib/life/period';

  let months = $state<MonthActivityDto[]>([]);
  let summary = $state<ActivitySummaryDto | null>(null);
  /** Three states, not a nullable: not asked, asked, answered (item 27's finding). */
  let loaded = $state(false);
  let failure = $state<string | null>(null);
  /** `null` is the whole life, which is what the page opens on. */
  let year = $state<number | null>(null);

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
  }
</script>

<svelte:head><title>Reading life — readingbuddy</title></svelte:head>

<a class="back" href="/">← Library</a>

<h1>Reading life</h1>

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
    refilled is legitimately empty here, and that is ambiguous between "nothing
    happened" and "nobody has built it yet". Naming the move is what stops it
    being a dead end, and it is the same sentence `rb activity` prints.
  -->
  <p class="note">Nothing recorded here yet.</p>
  <p class="hint">
    <code>rb activity --refill</code> builds the log from what you already have — your highlights,
    your notes and the days your reads began and ended.
  </p>
{:else}
  {#if summary}
    <Summary {summary} />
  {/if}

  {#if years.length > 1}
    <!-- A year filtered out of the whole, which the vision names. Not a filter
         over what is *left*: every year here is a year that happened. -->
    <nav aria-label="Which years">
      <button type="button" aria-pressed={year === null} onclick={() => pick(null)}>All</button>
      {#each years as y (y.year)}
        <button type="button" aria-pressed={year === y.year} onclick={() => pick(y.year)}>
          {y.year}
        </button>
      {/each}
    </nav>
  {/if}

  {#each shown as y (y.year)}
    <section class="band">
      <h2 class="band-title">{y.year}</h2>
      <!-- No figure beside the year heading. A year's books cannot be summed out
           of its months (they are distinct over each), and a heading carrying a
           wrong number is worse than one carrying none — the year's own totals
           are what the switch above fetches. -->
      <Months months={y.months} />
    </section>
  {/each}
{/if}

<style>
  .back {
    color: var(--ink-dim);
    font-size: 0.85rem;
    display: inline-block;
    margin-bottom: 1.1rem;
  }
  h1 {
    font-size: 1.05rem;
    color: var(--ink-dim);
    font-weight: 500;
    margin-bottom: 1.4rem;
  }
  /* The shelf's segmented switch, in the one other place a small set of
     alternatives is picked between. Deliberately not promoted to `app.css`:
     `ShelfSwitch` is a component with its own behaviour and this is four
     buttons, so sharing the *look* here is cheaper than sharing the widget. */
  nav {
    display: flex;
    gap: 0.3rem;
    flex-wrap: wrap;
    margin: 0.4rem 0 1.6rem;
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
  nav button[aria-pressed='true'] {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
  }
  section.band {
    margin-top: 1.8rem;
  }
  section.band h2 {
    margin-bottom: 0.4rem;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
</style>
