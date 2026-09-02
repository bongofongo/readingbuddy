<script lang="ts">
  /**
   * The chrome every chart on this page shares: a title, the plot, and the
   * numbers behind it.
   *
   * ## Why the table is not optional
   *
   * A chart that can only be read by hovering it is a chart a keyboard user, a
   * screen-reader user and anyone printing the page cannot read at all. So every
   * chart here ships a `<details>` holding the same numbers as a real `<table>`,
   * closed by default. The tooltip **enhances**; the table is what stops it
   * **gating**.
   *
   * It is a `<details>` rather than a toggle button because it needs no state,
   * survives with JavaScript broken, and is the one native element that already
   * means *there is more of this if you want it*.
   *
   * ## What this deliberately does not do
   *
   * No per-chart filters. The period is chosen once, in the year rail, and every
   * chart on the page answers for the same span — a control inside a chart card
   * would make two cards able to disagree about what they are describing.
   */
  import type { Snippet } from 'svelte';

  let {
    title,
    note,
    rows,
    columns,
    plot,
  }: {
    title: string;
    /** One quiet line under the title — the span, the unit, the caveat. */
    note?: string;
    /** The same numbers the plot draws, for the table. */
    rows: (string | number)[][];
    columns: string[];
    plot: Snippet;
  } = $props();
</script>

<section class="chart">
  <h3 class="band-title">{title}</h3>
  {#if note}<p class="note">{note}</p>{/if}

  <div class="plot">{@render plot()}</div>

  {#if rows.length > 0}
    <details>
      <summary>The numbers</summary>
      <div class="scroll">
        <table>
          <thead>
            <tr>
              {#each columns as c (c)}<th scope="col">{c}</th>{/each}
            </tr>
          </thead>
          <tbody>
            {#each rows as r, i (i)}
              <tr>
                {#each r as cell, j (j)}
                  {#if j === 0}<th scope="row">{cell}</th>{:else}<td>{cell}</td>{/if}
                {/each}
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </details>
  {/if}
</section>

<style>
  .chart {
    min-width: 0;
    padding: var(--s-3);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  .chart h3 {
    margin: 0;
  }
  .note {
    margin: 0.15rem 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  /* The plot sizes to its own content — a fixed height here is what crops an
     axis band and gives a card its own tiny scrollbar. */
  .plot {
    margin-top: var(--s-3);
  }
  details {
    margin-top: var(--s-3);
  }
  summary {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    cursor: pointer;
  }
  summary:hover {
    color: var(--ink);
  }
  /* A wide table scrolls inside its own card rather than pushing the page. */
  .scroll {
    overflow-x: auto;
    margin-top: var(--s-2);
  }
  table {
    border-collapse: collapse;
    font-size: var(--t-micro);
    width: 100%;
  }
  th,
  td {
    text-align: start;
    padding: 0.2rem 0.6rem 0.2rem 0;
    border-bottom: 1px solid var(--line);
    white-space: nowrap;
  }
  thead th {
    color: var(--ink-dim);
    font-weight: 400;
  }
  tbody th {
    font-weight: 400;
  }
  /* Columns of figures align; a standalone figure never does. */
  td {
    font-variant-numeric: tabular-nums;
    color: var(--ink-dim);
  }
</style>
