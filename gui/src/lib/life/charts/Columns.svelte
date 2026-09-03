<script lang="ts">
  /**
   * A column chart over an ordered axis — time, or a scale like a rating.
   *
   * Every mark carries its own hover **and focus** readout, because "the same
   * details on keyboard focus as on hover" is the difference between a chart
   * with a tooltip and a chart only a mouse can read.
   *
   * ## The specs, and why each is a spec rather than a taste
   *
   * - **Columns cap at 24px** and never fill their band. The leftover is the
   *   surface gap doing the separating; a stroke around each bar would be
   *   data-weight ink that is not data.
   * - **4px rounded cap, square at the baseline.** The rounding marks the data
   *   end; rounding the foot too would lift the bar off its own zero.
   * - **A zero is drawn as a hairline, not as nothing.** A bar of no height and
   *   an absent month look identical, and this chart is only ever handed months
   *   that exist — so the reader must be able to see that the value is zero
   *   rather than that the month is missing.
   * - **Axis labels are thinned, never rotated, and never clipped.** Rotated
   *   ticks are unreadable at this size and `overflow: hidden` on a tick crops
   *   its first characters, which is worse than no tick at all. Two modes: a
   *   *dense* axis labels every band and is only asked for when the labels are
   *   short and few (a rating, a decade, twelve month names); a *sparse* axis
   *   labels the two ends and the middle and lets the readout and the table
   *   carry the rest. A month-by-month series over three years is the second
   *   case, and drawing every nth label there is what put `Jan 25Mar 25May 25`
   *   on one line.
   */
  import type { Bar } from '$lib/life/graphs';

  let {
    bars,
    unit = '',
    /**
     * `1` labels every band — only for short labels in small numbers. Anything
     * else takes the sparse axis: first, middle and last.
     */
    every = 0,
  }: { bars: Bar[]; unit?: string; every?: number } = $props();

  const peak = $derived(bars.reduce((m, b) => Math.max(m, b.value), 0));
  const dense = $derived(every === 1 && bars.length <= 14);
  /** First, middle and last — deduped, so a two-bar chart shows two labels. */
  const ends = $derived(
    [...new Set([0, Math.floor((bars.length - 1) / 2), bars.length - 1])]
      .filter((i) => i >= 0 && i < bars.length)
      .map((i) => bars[i]!.label),
  );

  let active = $state<number | null>(null);

  function pct(v: number): number {
    // Against the peak, not against a round number: this is a shape, and a
    // headroom the data never reaches makes every bar shorter for nothing.
    return peak === 0 ? 0 : (v / peak) * 100;
  }
</script>

<div class="wrap">
  <div class="plot" role="group" aria-label="Column chart">
    {#each bars as b, i (b.key)}
      <div
        class="slot"
        class:on={active === i}
        role="button"
        tabindex="0"
        aria-label={`${b.label}: ${b.value}${unit ? ` ${unit}` : ''}`}
        onmouseenter={() => (active = i)}
        onmouseleave={() => (active = null)}
        onfocus={() => (active = i)}
        onblur={() => (active = null)}
      >
        <!-- The hit area is the whole slot, not the painted column: a 6px bar
             is a pinpoint nobody lands on. -->
        <span class="col" class:zero={b.value === 0} style:height={`${pct(b.value)}%`}></span>
      </div>
    {/each}
  </div>

  {#if dense}
    <div class="axis" aria-hidden="true">
      {#each bars as b (b.key)}
        <span class="tick">{b.label}</span>
      {/each}
    </div>
  {:else}
    <div class="axis sparse" aria-hidden="true">
      {#each ends as label (label)}
        <span class="tick">{label}</span>
      {/each}
    </div>
  {/if}

  <!-- The readout sits under the plot rather than floating over it: a tooltip
       that covers the neighbouring bars hides the comparison the reader is in
       the middle of making. It holds its height so nothing reflows on hover. -->
  <p class="readout" aria-live="polite">
    {#if active !== null}
      <strong>{bars[active]!.value}{unit ? ` ${unit}` : ''}</strong>
      <span>{bars[active]!.label}</span>
    {/if}
  </p>
</div>

<style>
  .plot {
    display: flex;
    align-items: end;
    /* The gap is the surface doing the separating. */
    gap: 2px;
    height: 6.5rem;
  }
  .slot {
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: end;
    justify-content: center;
    background: none;
    border: 0;
    padding: 0;
    cursor: default;
  }
  .col {
    display: block;
    width: 100%;
    /* Capped, so a wide chart of few bars does not become blocks. */
    max-width: 24px;
    background: var(--accent);
    /* The data end is rounded; the baseline end is square. */
    border-radius: 3px 3px 0 0;
    min-height: 1px;
  }
  /* A measured zero is a hairline on the baseline — visibly *a value*, and
     distinct from a month that is not in the data at all. */
  .zero {
    height: 1px;
    background: var(--ink-dim);
    border-radius: 0;
  }
  .slot.on .col,
  .slot:hover .col {
    filter: brightness(1.12);
  }
  .slot:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
    border-radius: var(--radius);
  }
  .axis {
    display: flex;
    gap: 2px;
    margin-top: 0.35rem;
  }
  /* The two ends and the middle, pushed apart — three labels cannot collide
     however narrow the card gets. */
  .axis.sparse {
    justify-content: space-between;
  }
  .tick {
    min-width: 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .axis:not(.sparse) .tick {
    flex: 1 1 0;
    text-align: center;
  }
  .readout {
    /* Reserved, so hovering does not move the chart under the pointer. */
    min-height: 1.35rem;
    margin: 0.4rem 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  /* The value leads and the label follows: the reader has the category already
     and came for the number. */
  .readout strong {
    color: var(--ink);
    font-weight: 600;
    margin-right: 0.4rem;
  }
</style>
