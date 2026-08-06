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
  import { countLabel, deviceFigures, monthLabel } from '$lib/phrasing';

  let { months }: { months: MonthActivityDto[] } = $props();
</script>

<ul>
  {#each months as m (m.month)}
    <li>
      <span class="month">{monthLabel(m.month)}</span>
      <span class="figures">
        <!-- Distinct books **over the whole month**, which is exactly why this
             cannot be folded out of days: the same two books opened on twelve
             days are two books, not twenty-four. -->
        <span>{countLabel(m.books, 'book')}</span>
        <span>{countLabel(m.activity_days, 'day')}</span>
        {#each deviceFigures(m.minutes, m.pages) as figure (figure)}
          <!-- An absence keeps its own voice. `deviceFigures` decides whether
               that is one chip or two; this only styles what it decided. -->
          <span class:absent={figure.includes('not measured') || figure === 'no device data'}>
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
  .figures {
    display: flex;
    gap: 0.35rem 1rem;
    flex-wrap: wrap;
    justify-content: flex-end;
    font-size: 0.82rem;
    color: var(--ink-dim);
  }
  /*
   * The chips line up down the page rather than each row starting where its own
   * text happens to.
   *
   * Every row carries the same figures in the same order, so a floor plus a
   * right edge gives them columns without making this a `<table>` — which at
   * 320px would either scroll sideways or crush. A month whose two device
   * figures collapsed into one chip is wider than the floor and simply takes the
   * room, which is the absence being allowed to look like what it is.
   */
  .figures > span {
    min-width: 4.2rem;
    text-align: right;
  }
  /* Italic, the same voice every other absence in this app has. It must not read
     as a value — that is the whole of what this page gets right. */
  .absent {
    font-style: italic;
    opacity: 0.85;
  }
</style>
