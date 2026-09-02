<script lang="ts">
  /**
   * The timeline: the months of one year, as the covers that closed in them.
   *
   * **Only months carrying an event appear**, which is the engine's rule and not
   * this component's shortcut: an empty month is the client's to draw, and
   * drawing it as a row of zeros would be the same lie as folding `null` into
   * `0` — a month you read nothing in and a month nothing was recorded about are
   * different things, and only one of them has a row.
   *
   * Nothing here sums, averages or compares two months. Comparison across a
   * period is the *Everything* tab's job now (`docs/decisions.md` entry 58) and
   * it is deliberately not done on the surface the page opens on.
   *
   * ## The covers are the row, and the sentence went
   *
   * This used to write each month as prose — *Finished Hollow Weather, Distant
   * Bell, Silent Letters…* — with a strip of 40px jackets beneath it as a
   * garnish. That was backwards. A title you finished eight months ago is a
   * string you have to read; its jacket is a thing you recognise without
   * reading, which is the entire reason a shelf works. So the covers grew, the
   * sentence went, and what is left on the row is **the month, the covers, and
   * the two figures**.
   *
   * Nothing was lost that was not said better: every jacket links to its book
   * and carries its title as a tooltip and an accessible name, so the names are
   * still there for anyone who needs them, and a screen reader gets a list of
   * titles rather than a sentence assembled by `joinList`.
   *
   * **A month with activity but nothing finished keeps its row.** It has its
   * name and its figures and simply no covers — which is true, and is not the
   * same claim as a month that is absent entirely.
   *
   * ## What the figures may and may not say
   *
   * **A month in which nothing was finished says nothing about finishing.** Not
   * "Finished nothing." — that is a deficit sentence wearing a fact, and it is
   * the exact failure this app is built to avoid. The page states what happened
   * and is silent about what did not.
   *
   * **An absence gets its own line and says what is absent.** Never a zero: a
   * month with no device data returns absent minutes, not zero minutes, and zero
   * is a claim. **But it says it only where it distinguishes something** — when
   * *no* month in the span has device data there is no measured month to be
   * confused with, and the summary above has already said `not measured` twice
   * and named `rb ko stats`. Repeated once per month it is wallpaper, which is
   * what entry 57 removed from the card wall in its other costume.
   */
  import type { MonthActivityDto } from '$lib/api/bindings';
  import type { StoredBook } from '$lib/api/client';
  import { client } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { countLabel, deviceFigures, monthLabel, NOT_MEASURED, titleLabel } from '$lib/phrasing';

  let {
    months,
    finished,
  }: {
    months: MonthActivityDto[];
    /**
     * The books whose reading **closed** in each month, keyed `YYYY-MM`.
     *
     * From `listReadingRows` with a `finished_in` span, which is the request
     * that answers this — not from the activity log, which records that
     * something happened on a day and not what ended. A month with no entry
     * simply has no covers.
     */
    finished: Map<string, StoredBook[]>;
  } = $props();

  /** Does any month in this span carry device figures? See the module doc. */
  const someMeasured = $derived(months.some((m) => m.minutes !== null || m.pages !== null));
</script>

<ul>
  {#each months as m (m.month)}
    {@const device = deviceFigures(m.minutes, m.pages)}
    {@const done = finished.get(m.month) ?? []}
    <li>
      <div class="head">
        <span class="month">{monthLabel(m.month)}</span>
        <span class="figures">
          <!-- Distinct books **over the whole month**, which is exactly why this
               cannot be folded out of days: the same two books opened on twelve
               days are two books, not twenty-four. -->
          <strong>{countLabel(m.books, 'book')}</strong> on
          <strong>{countLabel(m.activity_days, 'day')}</strong>
        </span>
      </div>

      {#if done.length > 0}
        <!-- The jackets of what closed. Typography is suppressed at this size by
             the size alone — a title rendered at 56px wide is illegible and
             reads as a rendering fault — so the artwork is what is here and the
             name travels as the link's accessible name. -->
        <div class="jackets">
          {#each done as b, i (`${b.id}-${i}`)}
            <a
              class="art"
              href={`/book/${b.id}`}
              title={titleLabel(b.title)}
              aria-label={titleLabel(b.title)}
            >
              <Jacket src={client().coverSrc(b)} accent={b.cover_accent} />
            </a>
          {/each}
        </div>
      {/if}

      {#if device.length === 1}
        {#if someMeasured}
          <!-- The named absence, in its own voice: dimmed **and** italic,
               because italics alone was the only thing separating a measured
               `0 min` from an unmeasured one. -->
          <p class="absent">
            No device data — minutes and pages come from a reader you have connected.
          </p>
        {/if}
      {:else}
        <p class="measured">
          <!-- The separator is CSS and not a text node between the spans:
               Svelte trims the whitespace around markup, so ` · ` written
               inline arrives as `10 h 20 min·410 pages`. A `::before` cannot
               be trimmed. -->
          {#each device as figure (figure)}<span class:absent={figure.includes(NOT_MEASURED)}
              >{figure}</span
            >{/each}
        </p>
      {/if}
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    padding: 0.9rem 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  /*
   * The month and its figures on one line, pushed apart.
   *
   * The month name is the anchor a reader scans down, so it keeps the left
   * edge; the figures go to the right rather than beside it, which stops a long
   * month name shifting them and gives the eye a second column to run down
   * without either being a heading over the other.
   */
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.55rem;
  }
  .month {
    font-size: var(--t-fine);
    color: var(--ink);
  }
  .figures,
  .measured {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    margin: 0;
  }
  /* Only `.figures` carries a `strong`; the device line is a run of spans. */
  .figures strong {
    color: var(--ink);
    font-weight: 600;
  }
  .measured span + span::before {
    content: ' · ';
    color: var(--ink-dim);
    font-style: normal;
  }
  .measured {
    margin-top: 0.5rem;
  }
  .absent {
    font-style: italic;
    color: var(--ink-dim);
    font-size: var(--t-micro);
    margin: 0.5rem 0 0;
  }
  .jackets {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem;
  }
  /*
   * 56px, up from the 40px this was when it garnished a sentence.
   *
   * The covers are the content of the row now, and 40px is below the size at
   * which a jacket is recognisable as a particular book rather than as a
   * coloured rectangle — which is the whole thing being traded for the prose
   * that went.
   */
  .art {
    display: block;
    width: 56px;
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border-radius: 2px;
    overflow: hidden;
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent),
      0 1px 2px rgb(0 0 0 / 0.18);
  }
  .art:hover {
    box-shadow:
      inset 0 0 0 1px var(--accent-text),
      0 1px 2px rgb(0 0 0 / 0.18);
  }
</style>
