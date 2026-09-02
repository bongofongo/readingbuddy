<script lang="ts">
  /**
   * The months of one year, written as sentences.
   *
   * **Only months carrying an event appear**, which is the engine's rule and not
   * this component's shortcut: an empty month is the client's to draw, and
   * drawing it as a row of zeros would be the same lie as folding `null` into
   * `0` — a month you read nothing in and a month nothing was recorded about are
   * different things, and only one of them has a row.
   *
   * Nothing here sums, averages or compares two months. A "best month" is a
   * threshold discovered after the fact, which is a leaderboard with one
   * competitor, and it is the nearest wrong turn on this screen.
   *
   * ## Sentences rather than chips, and what that changes
   *
   * The figures used to be a four-column grid of chips. A stat block invites
   * reading *down* a column, and a column of comparable numbers is a scoreboard
   * one glance away; a sentence is read across and says what happened. The
   * figures are still the engine's, unchanged, and are still bold — what went is
   * the grid that made them a series.
   *
   * **A month in which nothing was finished says nothing about finishing.** Not
   * "Finished nothing." — that is a deficit sentence wearing a fact, and it is
   * the exact failure this app is built to avoid. The page states what happened
   * and is silent about what did not.
   *
   * **An absence gets its own line and says what is absent.** Never a zero: a
   * month with no device data returns absent minutes, not zero minutes, and zero
   * is a claim.
   */
  import type { MonthActivityDto } from '$lib/api/bindings';
  import type { StoredBook } from '$lib/api/client';
  import { client } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import {
    countLabel,
    deviceFigures,
    joinList,
    monthLabel,
    NOT_MEASURED,
    titleLabel,
  } from '$lib/phrasing';

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
     * simply has no finishing sentence.
     */
    finished: Map<string, StoredBook[]>;
  } = $props();
</script>

<ul>
  {#each months as m (m.month)}
    {@const device = deviceFigures(m.minutes, m.pages)}
    {@const done = finished.get(m.month) ?? []}
    <li>
      <span class="month">{monthLabel(m.month)}</span>
      <div class="said">
        {#if done.length > 0}
          <p class="what">
            Finished {joinList(done.map((b) => titleLabel(b.title)))}.
          </p>
        {/if}
        <p class="figures">
          <!-- Distinct books **over the whole month**, which is exactly why this
               cannot be folded out of days: the same two books opened on twelve
               days are two books, not twenty-four. -->
          <strong>{countLabel(m.books, 'book')}</strong> on
          <strong>{countLabel(m.activity_days, 'day')}</strong>.
        </p>
        {#if device.length === 1}
          <!-- The named absence, on its own line, in its own voice: dimmed
               **and** italic, because italics alone was the only thing
               separating a measured `0 min` from an unmeasured one — one weak
               signal carrying the distinction this whole page exists for. -->
          <p class="absent">
            No device data — minutes and pages come from a reader you have connected.
          </p>
        {:else}
          <p class="figures">
            <!-- The separator is CSS and not a text node between the spans:
                 Svelte trims the whitespace around markup, so ` · ` written
                 inline arrives as `10 h 20 min·410 pages`. A `::before` cannot
                 be trimmed. -->
            {#each device as figure (figure)}<span class:absent={figure.includes(NOT_MEASURED)}
                >{figure}</span
              >{/each}
          </p>
        {/if}
        {#if done.length > 0}
          <!-- The jackets of what closed, small. Typography is suppressed at this
               size inside `Jacket`'s plate by the size alone — a title rendered
               at 40px wide is illegible and reads as a rendering fault, so what
               is here is the artwork and the name is in the sentence above. -->
          <div class="jackets">
            {#each done as b (b.id)}
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
      </div>
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: calc(var(--column) + 7rem);
  }
  li {
    display: grid;
    grid-template-columns: 7rem minmax(0, 1fr);
    gap: 0 1rem;
    padding: 0.8rem 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  @media (max-width: 520px) {
    li {
      grid-template-columns: minmax(0, 1fr);
      gap: 0.3rem 0;
    }
  }
  .month {
    font-size: var(--t-fine);
    color: var(--ink-dim);
  }
  .said {
    min-width: 0;
  }
  p {
    margin: 0 0 0.2rem;
    font-size: var(--t-fine);
    overflow-wrap: anywhere;
  }
  .what {
    /* What was read is the sentence that matters; the figures under it are the
       measurement of it. Ink against dim carries that without a second size. */
    color: var(--ink);
  }
  .figures {
    color: var(--ink-dim);
    font-size: var(--t-fine);
  }
  .figures span + span::before {
    content: ' · ';
    color: var(--ink-dim);
    font-style: normal;
  }
  .figures strong {
    color: var(--ink);
    font-weight: 600;
  }
  .absent {
    font-style: italic;
    color: var(--ink-dim);
    font-size: var(--t-fine);
  }
  .jackets {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }
  .art {
    display: block;
    width: 40px;
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border-radius: 2px;
    overflow: hidden;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent);
  }
</style>
