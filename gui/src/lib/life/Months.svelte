<script lang="ts">
  /**
   * The months of one year, as they came off the wire.
   *
   * **Only months carrying an event appear**, which is the engine's rule and not
   * this component's shortcut: an empty month is the client's to draw, and
   * drawing it as a row of zeros would be the same lie as folding `null` into
   * `0` — a month you read nothing in and a month nothing was recorded about are
   * different things, and only one of them has a row.
   *
   * Nothing here sums, averages or compares two months. A "best month" is a
   * threshold discovered after the fact, which is a leaderboard with one
   * competitor, and it is the second-nearest wrong turn on this screen.
   *
   * Not a `<table>`: at 320px a four-column table either scrolls sideways or
   * crushes its columns, and every one of these figures is a chip that reads on
   * its own. The month name is the row's heading and the chips wrap under it.
   */
  import type { MonthActivityDto } from '$lib/api/bindings';
  import { countLabel, deviceFigures, monthLabel, NO_DEVICE_DATA, NOT_MEASURED } from '$lib/phrasing';

  let { months }: { months: MonthActivityDto[] } = $props();
</script>

<ul>
  {#each months as m (m.month)}
    {@const device = deviceFigures(m.minutes, m.pages)}
    <li>
      <span class="month">{monthLabel(m.month)}</span>
      <span class="figures">
        <!-- Distinct books **over the whole month**, which is exactly why this
             cannot be folded out of days: the same two books opened on twelve
             days are two books, not twenty-four. -->
        <span>{countLabel(m.books, 'book')}</span>
        <span>{countLabel(m.activity_days, 'day')}</span>
        {#each device as figure (figure)}
          <!-- An absence keeps its own voice, and now two of them: dimmed **and**
               italic. Italics alone was the only thing separating a measured
               `0 min` from `minutes not measured`, which is one weak signal
               carrying the distinction this whole page exists for. -->
          <span
            class:absent={figure === NO_DEVICE_DATA || figure.includes(NOT_MEASURED)}
            class:wide={device.length === 1}
          >
            {figure}
          </span>
        {/each}
      </span>
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: calc(var(--measure) + 6rem);
  }
  li {
    display: flex;
    gap: 0.4rem 1rem;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    padding: 0.55rem 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  .month {
    font-size: 0.92rem;
    /* Wide enough that a column of month names lines up, and allowed to shrink
       rather than push the chips off the row. */
    flex: 0 1 9rem;
  }
  /*
   * A fixed template, so the figures line up down the page.
   *
   * A floor per chip was the first attempt and it did not work: *minutes not
   * measured* is far wider than any number, so it blew out its own slot and
   * shoved everything to its left, leaving April's columns 65px off January's.
   * A page of monthly figures whose whole purpose is reading **down** a column
   * has to have columns.
   *
   * Still not a `<table>`: at 320px a four-column table either scrolls sideways
   * or crushes, and the template below reflows to two columns instead. What is
   * borrowed from a table is the shared geometry, not the element.
   */
  .figures {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.3rem 1rem;
    justify-items: end;
    font-size: 0.82rem;
    /* `--ink`, not `--ink-dim`. The month name was three times the contrast of
       the figures beside it, which inverts the hierarchy of a page whose
       subject *is* the figures — the stat block above already gets this right
       with a dark number and a muted label. */
    color: var(--ink);
    flex: 1 1 14rem;
  }
  @media (min-width: 34rem) {
    .figures {
      /* The third column is sized for the widest thing that can land in it,
         which is the named absence rather than any number. */
      grid-template-columns: 4.5rem 4.5rem 9rem 6rem;
      flex: 0 0 auto;
    }
  }
  /* One chip standing for both device figures takes both slots rather than
     leaving a hole where the second would have been. */
  .wide {
    grid-column: span 2;
  }
  /*
   * Italic **and** dimmed — two signals, and the second one is not decoration.
   *
   * With the figures now at `--ink`, italics alone was the only thing between a
   * measured `0 min` and `minutes not measured`, and the distinction those two
   * carry is the reason this page exists. `--ink-dim` is also what fixes the
   * contrast: `opacity: 0.85` over `--ink-dim` measured **3.88:1** on the light
   * theme — under AA, on the one string that must be legible *as* an absence.
   */
  .absent {
    font-style: italic;
    color: var(--ink-dim);
  }
</style>
