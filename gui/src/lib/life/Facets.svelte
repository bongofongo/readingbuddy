<script lang="ts">
  /**
   * *Everything* — the reading-life page's second tab, and the full disclosure
   * of what a period's data can say.
   *
   * **This tab is the permission.** `docs/decisions.md` entry 58 lifted the ban
   * on ranking and on comparing one stretch of time against another, and it did
   * so on one condition: that the material lives somewhere a reader **goes**,
   * not somewhere they are met. The page opens on the timeline; this is the
   * other tab. Draw any of it on arrival and the permission is gone.
   *
   * Three groups, and the grouping is the argument:
   *
   * - **What you read** — ranked. Authors, subjects, the longest books.
   * - **Measures** — the distributions and the sums, on their own axes.
   * - **Over time** — the self-comparison. The trend, the busiest month, the
   *   longest run of days.
   *
   * ## What still may not appear here
   *
   * The axiom is *"the app tells you what you did; it never tells you what you
   * have left"*, and entry 58 did not touch it. So: no goal, no target, no pace,
   * no *on track*, no *behind*, nothing counting what is undone, and **no
   * figure on the tab label itself** — entry 51 rules that a number on a control
   * is one decision from a badge.
   *
   * The run panel carries the sharpest edge. `longestRunOf` refuses any run
   * touching today, so what is drawn is always over — a run still going is a
   * thing a reader can be made to feel they must protect, which is the streak
   * this app is built without.
   *
   * Every figure comes from `facets.ts` and inherits its two caveats: bounded by
   * the caller's row ceiling, and about closed readings only. None is presented
   * as a total of the library.
   */
  import type { DayActivityDto, MonthActivityDto } from '$lib/api/bindings';
  import type { Bar } from '$lib/life/graphs';
  import type { ReadingRow } from '$lib/api/client';
  import { countLabel, monthLabel, titleLabel } from '$lib/phrasing';

  import Bars from './charts/Bars.svelte';
  import Calendar from './charts/Calendar.svelte';
  import Columns from './charts/Columns.svelte';
  import Frame from './charts/Frame.svelte';
  import Scatter from './charts/Scatter.svelte';
  import type { Tally } from './facets';
  import {
    authorsOf,
    busiestOf,
    decadesOf,
    longestOf,
    longestRunOf,
    meanRatingOf,
    pagesOf,
    passageOf,
    ratingsOf,
    rereadsOf,
    subjectsOf,
  } from './facets';
  import {
    calendar,
    cumulative,
    durations,
    lengths,
    lengthVsRating,
    measured,
    perMonth,
    pubVsRead,
    seasonality,
  } from './graphs';

  let {
    rows,
    months,
    days,
    today,
  }: {
    rows: ReadingRow[];
    /** The months of the **whole life**, so the trend is a trend and not a slice. */
    months: MonthActivityDto[];
    days: DayActivityDto[];
    /** `YYYY-MM-DD`, passed in rather than read here so the run rule is testable. */
    today: string;
  } = $props();

  const passage = $derived(passageOf(rows));
  const authors = $derived(authorsOf(rows));
  const subjects = $derived(subjectsOf(rows));
  const longest = $derived(longestOf(rows));
  const rereads = $derived(rereadsOf(rows));
  const ratings = $derived(ratingsOf(rows));
  const mean = $derived(meanRatingOf(rows));
  const decades = $derived(decadesOf(rows));
  const pages = $derived(pagesOf(rows));
  const busiest = $derived(busiestOf(months));
  const run = $derived(longestRunOf(days, today));

  // The plots. Each is a shape; the numbers behind it live in its own table.
  const finishedPerMonth = $derived(perMonth(months));
  const running = $derived(cumulative(rows));
  const season = $derived(seasonality(months));
  const minutesPerMonth = $derived(measured(months, 'minutes'));
  const pagesPerMonth = $derived(measured(months, 'pages'));
  const lengthBins = $derived(lengths(rows));
  const durationBins = $derived(durations(rows));
  const ageVsRead = $derived(pubVsRead(rows));
  const sizeVsRating = $derived(lengthVsRating(rows));
  const cal = $derived(calendar(days));

  /** A month key as a word, for a chart axis where `2025-03` is noise. */
  function shortMonth(key: string): string {
    const m = /^(\d{4})-(\d{2})$/.exec(key);
    if (m === null) return key;
    const names = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
    return `${names[Number(m[2]) - 1]} ${m[1]!.slice(2)}`;
  }

  /** A unix second as a year, for the scatter's own axis. */
  function yearOf(seconds: number): string {
    return new Date(seconds * 1000).toISOString().slice(0, 4);
  }

  /** A month-keyed series with its axis labels put into words. */
  function named(bars: Bar[]): Bar[] {
    return bars.map((b) => ({ ...b, label: shortMonth(b.label) }));
  }

  /** A `Tally` is `{key, count}`; a chart wants `{key, label, value}`. */
  function asBars(tallies: Tally[], label?: (key: string) => string): Bar[] {
    return tallies.map((t) => ({ key: t.key, label: label ? label(t.key) : t.key, value: t.count }));
  }

</script>

{#if rows.length > 0}
  <!--
    Fourteen plots, in three bands.

    **Pictures first.** Every panel that was a list of figures is a chart now,
    and the figures did not go anywhere — each chart's `Frame` carries a
    `<details>` holding the same numbers as a real table. The tooltip enhances;
    the table is what stops it gating.

    Every chart is a **single series**, so each is one hue and none carries a
    legend: with one colour on the plot, a legend box would only restate the
    title. Nothing here is dual-axis — two measures of different scale are two
    charts, which is why minutes and pages are drawn separately rather than
    together.
  -->
  <h2 class="group">What you read</h2>
  <div class="facets">
    {#if authors.length > 0}
      <Frame
        title="Who you read"
        note="By how many of theirs you finished. Ties are alphabetical."
        columns={['Author', 'Books']}
        rows={authors.map((t) => [t.key, t.count])}
      >
        {#snippet plot()}
          <Bars bars={asBars(authors.slice(0, 12))} />
        {/snippet}
      </Frame>
    {/if}

    {#if subjects.length > 0}
      <Frame
        title="What it was about"
        note="Counted per book, not per mention."
        columns={['Subject', 'Books']}
        rows={subjects.map((t) => [t.key, t.count])}
      >
        {#snippet plot()}
          <Bars bars={asBars(subjects.slice(0, 12))} />
        {/snippet}
      </Frame>
    {/if}

    {#if longest.length > 0}
      <Frame
        title="The longest of them"
        note="Pages, where the book states a length."
        columns={['Book', 'Pages']}
        rows={longest.map((x) => [titleLabel(x.book.title), x.pages])}
      >
        {#snippet plot()}
          <Bars
            bars={longest
              .slice(0, 10)
              .map((x, i) => ({ key: `${x.book.id}-${i}`, label: titleLabel(x.book.title), value: x.pages }))}
            link={(b) => `/book/${b.key.slice(0, b.key.lastIndexOf('-'))}`}
          />
        {/snippet}
      </Frame>
    {/if}

    {#if rereads.length > 0}
      <section class="panel">
        <h3 class="band-title">Books you went back to</h3>
        <ul class="rereads">
          {#each rereads as r, i (`${r.book.id}-${r.readNumber}-${i}`)}
            <li>
              <a href={`/book/${r.book.id}`}>{titleLabel(r.book.title)}</a>
              <!-- The engine's own numbering (item 41). Never
                   `indexOf(id) + 1`, which re-derives an ordering the wire
                   does not state. -->
              <span class="which">read {r.readNumber} of {r.ofReads}</span>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  </div>

  <h2 class="group">Measures</h2>
  <div class="facets">
    {#if passage}
      <section class="panel">
        <h3 class="band-title">A passage you kept</h3>
        <blockquote>{passage.passage.text}</blockquote>
        <p class="whence">
          <a href={`/book/${passage.book.id}`}>{titleLabel(passage.book.title)}</a>
        </p>
      </section>
    {/if}

    {#if ratings.length > 0}
      <Frame
        title="Ratings you gave"
        note={mean ? `${mean.mean.toFixed(1)} on average, over ${countLabel(mean.of, 'rating')}.` : undefined}
        columns={['Rating', 'Readings']}
        rows={ratings.map((t) => [t.key, t.count])}
      >
        {#snippet plot()}
          <!-- An ordered scale, so the axis is the scale itself and the shape of
               the distribution is the thing being read. Unrated readings are
               absent rather than a bar at zero. -->
          <Columns bars={asBars(ratings)} every={1} />
        {/snippet}
      </Frame>
    {/if}

    {#if decades.length > 0}
      <Frame
        title="When the books were written"
        note="A fact about the shelf, not about the reader."
        columns={['Decade', 'Books']}
        rows={decades.map((t) => [t.key, t.count])}
      >
        {#snippet plot()}
          <Columns bars={asBars(decades, (k) => k.replace('0s', '0'))} every={2} />
        {/snippet}
      </Frame>
    {/if}

    {#if lengthBins.length > 0}
      <Frame
        title="How long the books were"
        note="Pages, in fixed bins. A book with no stated length is absent."
        columns={['Pages', 'Books']}
        rows={lengthBins.map((b) => [b.label, b.count])}
      >
        {#snippet plot()}
          <Columns
            bars={lengthBins.map((b) => ({ key: b.label, label: b.label, value: b.count }))}
            every={1}
          />
        {/snippet}
      </Frame>
    {/if}

    {#if durationBins.length > 0}
      <Frame
        title="How long a read was open"
        note="Days, where both ends were recorded. There is no average — an average here is a pace."
        columns={['Days', 'Reads']}
        rows={durationBins.map((b) => [b.label, b.count])}
      >
        {#snippet plot()}
          <Columns
            bars={durationBins.map((b) => ({ key: b.label, label: b.label, value: b.count }))}
            every={1}
          />
        {/snippet}
      </Frame>
    {/if}

    {#if pages.stated > 0}
      <section class="panel">
        <h3 class="band-title">Pages</h3>
        <p class="figure">{pages.pages.toLocaleString()}</p>
        <!-- The sum alone would be a lie by omission: `page_count` is nullable,
             so the denominator travels with it — **when there is one**. Where
             every book states a length there is nothing to qualify. Never
             phrased as a shortfall either way: it says what was counted, not
             what was missing. -->
        {#if pages.stated === pages.total}
          <p class="more">over {countLabel(pages.total, 'book')}.</p>
        {:else}
          <p class="more">
            over the {countLabel(pages.stated, 'book')} of {pages.total} that state a length.
          </p>
        {/if}
      </section>
    {/if}

    {#if sizeVsRating.length > 1}
      <Frame
        title="Length against what you gave it"
        note="Both axes are facts about one book. No trend line — a regression through what you read is a verdict."
        columns={['Book', 'Pages', 'Rating']}
        rows={sizeVsRating.map((p) => [titleLabel(p.book.title), p.x, p.y])}
      >
        {#snippet plot()}
          <Scatter
            points={sizeVsRating}
            xLabel="pages"
            yLabel="rating"
            xText={(v) => `${v} pages`}
            yText={(v) => `rated ${v}`}
          />
        {/snippet}
      </Frame>
    {/if}
  </div>

  <!--
    Over time — the self-comparison entry 58 permits, and the band that would
    have been refused outright before it.

    Every panel here is past tense and describes something that has finished. It
    is the ordering of the tabs that makes them allowable: a reader arrives at
    the timeline and comes here on purpose.
  -->
  <h2 class="group">Over time</h2>
  <div class="facets">
    {#if finishedPerMonth.length > 1}
      <Frame
        title="Month by month"
        note="The months as they were. Nothing is averaged — an average over time is a pace."
        columns={['Month', 'Books']}
        rows={finishedPerMonth.map((b) => [b.label, b.value])}
      >
        {#snippet plot()}
          <Columns bars={named(finishedPerMonth)} unit="books" />
        {/snippet}
      </Frame>
    {/if}

    {#if running.length > 1}
      <Frame
        title="The shelf filling"
        note="A running total of what closed. It only ever goes up, which is why no month on it can look like a failure."
        columns={['Month', 'Books so far']}
        rows={running.map((b) => [b.label, b.value])}
      >
        {#snippet plot()}
          <Columns bars={named(running)} unit="books so far" />
        {/snippet}
      </Frame>
    {/if}

    {#if season.some((b) => b.value > 0)}
      <Frame
        title="Time of year"
        note="Days with something on them, by calendar month, across every year in the span."
        columns={['Month', 'Days']}
        rows={season.map((b) => [b.label, b.value])}
      >
        {#snippet plot()}
          <!-- Days and not books: two Januaries can hold the same book, and
               `books` is distinct over a period and cannot be summed out of the
               periods inside it (item 42). Days can. -->
          <Columns bars={season} unit="days" every={1} />
        {/snippet}
      </Frame>
    {/if}

    {#if cal.weeks.length > 0}
      <Frame
        title="The days themselves"
        note="Only days something was recorded on. An empty square is a day with nothing recorded, never a zero."
        columns={['Day', 'Books']}
        rows={cal.weeks.flat().flatMap((c) => (c === null ? [] : [[c.day, c.books]]))}
      >
        {#snippet plot()}
          <Calendar weeks={cal.weeks} peak={cal.peak} />
        {/snippet}
      </Frame>
    {/if}

    {#if minutesPerMonth.length > 1}
      <Frame
        title="Minutes, where a device measured them"
        note="Months the device never measured are absent, never drawn as zero."
        columns={['Month', 'Minutes']}
        rows={minutesPerMonth.map((b) => [b.label, b.value])}
      >
        {#snippet plot()}
          <Columns bars={named(minutesPerMonth)} unit="min" />
        {/snippet}
      </Frame>
    {/if}

    {#if pagesPerMonth.length > 1}
      <Frame
        title="Pages turned, where a device counted them"
        note="Its own chart rather than a second axis beside the minutes — two scales on one plot invent a relationship."
        columns={['Month', 'Pages']}
        rows={pagesPerMonth.map((b) => [b.label, b.value])}
      >
        {#snippet plot()}
          <Columns bars={named(pagesPerMonth)} unit="pages" />
        {/snippet}
      </Frame>
    {/if}

    {#if ageVsRead.length > 1}
      <Frame
        title="When it was written against when you read it"
        note="A column at the right is a season of new books; a band along the bottom is a year in one century."
        columns={['Book', 'Published', 'Finished']}
        rows={ageVsRead.map((p) => [titleLabel(p.book.title), p.y, yearOf(p.x)])}
      >
        {#snippet plot()}
          <Scatter
            points={ageVsRead}
            xLabel="when you read it"
            yLabel="when it was written"
            xText={(v) => `read ${yearOf(v)}`}
            yText={(v) => `written ${v}`}
          />
        {/snippet}
      </Frame>
    {/if}

    {#if busiest}
      <section class="panel">
        <h3 class="band-title">The busiest month</h3>
        <!-- *Busiest*, never *best*: the first says what happened, the second
             grades it. Ties go to the earlier month so the answer is stable. -->
        <p class="figure">{monthLabel(busiest.month)}</p>
        <p class="more">
          {countLabel(busiest.books, 'book')} on {countLabel(busiest.activity_days, 'day')}.
        </p>
      </section>
    {/if}

    {#if run}
      <section class="panel">
        <h3 class="band-title">The longest run of days</h3>
        <!-- Only ever a run that has **ended**. `longestRunOf` refuses one
             touching today, which is the condition entry 23 attached to
             speaking a run at all — a run still going is something a reader can
             be made to feel they must protect. -->
        <p class="figure">{countLabel(run.days, 'day')}</p>
        <p class="more">{run.from} to {run.to}.</p>
      </section>
    {/if}
  </div>
{/if}

<style>
  /*
   * A wrapping column of panels, and the wrapping is the point.
   *
   * `auto-fit` with a floor, so the periphery is one column beside a narrow
   * window and several across a wide one — which is the "more information when
   * the screen allows it" this was built for, without a breakpoint per panel.
   */
  /*
   * A group heading over each band of panels.
   *
   * `.band-title` is the panel's own heading, so the group above it needs to be
   * a step up rather than the same token twice — ink against dim, at lead size,
   * with the panels' own headings staying `.band-title`. Three of these is what
   * turns a wall of eleven panels into three answers.
   */
  .group {
    font-size: var(--t-lead);
    margin: 2rem 0 var(--s-3);
  }
  .group:first-of-type {
    margin-top: 0;
  }
  /*
   * Wider than the 15rem the prose panels wanted.
   *
   * A chart needs room its label needs: twelve month names across a 15rem card
   * is 26px a tick, and `May` does not fit in 26px. The floor is what a dense
   * axis can actually be read at, and everything else follows it.
   */
  .facets {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(21rem, 100%), 1fr));
    gap: var(--s-4);
    align-items: start;
  }
  .panel {
    min-width: 0;
    padding: var(--s-3);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .panel h3 {
    margin: 0 0 var(--s-2);
  }
    blockquote {
    margin: 0;
    font-size: var(--t-fine);
    line-height: 1.55;
    font-style: italic;
    overflow-wrap: anywhere;
  }
  blockquote::before {
    content: '“';
  }
  blockquote::after {
    content: '”';
  }
  .whence {
    margin: var(--s-2) 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  .whence a:hover {
    color: var(--accent-text);
  }

      .more {
    margin: var(--s-2) 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }

  .rereads {
    list-style: none;
    margin: 0;
    padding: 0;
    font-size: var(--t-fine);
  }
  .rereads li {
    padding: 0.15rem 0;
    overflow-wrap: anywhere;
  }
  .rereads a:hover {
    color: var(--accent-text);
  }
  .which {
    margin-left: 0.4rem;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }

    .figure {
    margin: 0;
    font-size: var(--t-title);
    line-height: 1.15;
    letter-spacing: -0.01em;
    color: var(--ink);
  }
</style>
