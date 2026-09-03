<script lang="ts">
  /**
   * Two facts about one book, plotted against each other.
   *
   * **There is no trend line and there must not be one.** A regression through
   * *what somebody read* is a verdict dressed as arithmetic — it tells a reader
   * their taste has a direction and that they are on or off it. The dots are the
   * whole content; any shape in them is the reader's to see.
   *
   * ## The hit target is the problem this form always has
   *
   * A 9px dot is a pinpoint. Each point therefore carries a transparent hit area
   * far larger than the mark, and the marks themselves carry a 2px surface ring
   * so overlapping books stay countable rather than merging into a blob.
   *
   * Points are drawn in SVG rather than as positioned elements because the ring,
   * the overlap and the hit area all want to be one object with the dot.
   */
  import type { Point } from '$lib/life/graphs';
  import { titleLabel } from '$lib/phrasing';

  let {
    points,
    xLabel,
    yLabel,
    /** How to phrase each axis in the readout. */
    xText,
    yText,
  }: {
    points: Point[];
    xLabel: string;
    yLabel: string;
    xText: (v: number) => string;
    yText: (v: number) => string;
  } = $props();

  const W = 100;
  const H = 62;
  const PAD = 3;

  const xs = $derived(points.map((p) => p.x));
  const ys = $derived(points.map((p) => p.y));
  const bounds = $derived({
    x0: Math.min(...xs),
    x1: Math.max(...xs),
    y0: Math.min(...ys),
    y1: Math.max(...ys),
  });

  let active = $state<number | null>(null);

  /** A single value collapses its own axis; centre it rather than divide by zero. */
  function at(v: number, lo: number, hi: number, size: number): number {
    if (hi === lo) return size / 2;
    return PAD + ((v - lo) / (hi - lo)) * (size - PAD * 2);
  }
</script>

<div class="wrap">
  <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label={`${yLabel} against ${xLabel}`}>
    <!-- A hairline frame on two sides, one step off the surface. Solid, never
         dashed: a dashed rule reads as a threshold when it is only an axis. -->
    <line x1={PAD} y1={H - PAD} x2={W - PAD} y2={H - PAD} class="axis" />
    <line x1={PAD} y1={PAD} x2={PAD} y2={H - PAD} class="axis" />

    {#each points as p, i (`${p.book.id}-${i}`)}
      {@const cx = at(p.x, bounds.x0, bounds.x1, W)}
      {@const cy = H - at(p.y, bounds.y0, bounds.y1, H)}
      <g class="pt" class:on={active === i}>
        <circle {cx} {cy} r="2.2" class="dot" />
        <!-- The hit area, invisible and much larger than the mark. -->
        <circle
          {cx}
          {cy}
          r="5"
          class="hit"
          role="button"
          tabindex="0"
          aria-label={`${titleLabel(p.book.title)} — ${xText(p.x)}, ${yText(p.y)}`}
          onmouseenter={() => (active = i)}
          onmouseleave={() => (active = null)}
          onfocus={() => (active = i)}
          onblur={() => (active = null)}
        />
      </g>
    {/each}
  </svg>

  <p class="axes" aria-hidden="true">
    <span>{xLabel} →</span>
    <span>↑ {yLabel}</span>
  </p>

  <p class="readout" aria-live="polite">
    {#if active !== null}
      <strong>{titleLabel(points[active]!.book.title)}</strong>
      <span>{xText(points[active]!.x)} · {yText(points[active]!.y)}</span>
    {/if}
  </p>
</div>

<style>
  svg {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }
  .axis {
    stroke: var(--line);
    stroke-width: 0.4;
  }
  .dot {
    fill: var(--accent);
    /* The surface ring, so overlapping books stay countable. */
    stroke: var(--bg);
    stroke-width: 0.7;
  }
  .hit {
    fill: transparent;
    cursor: default;
  }
  .pt.on .dot {
    fill: var(--accent-text);
    r: 3;
  }
  .hit:focus-visible {
    outline: none;
  }
  .pt:has(.hit:focus-visible) .dot {
    fill: var(--accent-text);
    stroke: var(--accent-text);
    stroke-width: 1.4;
  }
  .axes {
    display: flex;
    justify-content: space-between;
    margin: 0.3rem 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  .readout {
    min-height: 1.35rem;
    margin: 0.3rem 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .readout strong {
    color: var(--ink);
    font-weight: 600;
    margin-right: 0.4rem;
  }
</style>
