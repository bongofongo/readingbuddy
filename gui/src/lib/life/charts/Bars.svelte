<script lang="ts">
  /**
   * A horizontal bar chart — for categories whose names are words.
   *
   * Horizontal because the labels are author names and subject names: rotated
   * ticks under a column chart are unreadable, and a name reads left to right in
   * its own row without any of that.
   *
   * **The value rides the tip of its own bar.** That is the sanctioned direct
   * label for this form and it is not the "a number on every point" failure —
   * that one is about scattered marks and stacked segments, where labels collide
   * and go unread. Here every value sits in its own row on a shared right edge,
   * which is the axis a bar chart would otherwise need.
   *
   * There is no track behind the bar. A track is a **meter**'s idiom — it means
   * *this much of a fixed whole* — and none of these values is a portion of
   * anything.
   */
  import type { Bar } from '$lib/life/graphs';

  let {
    bars,
    /** Optional per-row link, so a bar about a book can be one. */
    link,
  }: { bars: Bar[]; link?: (bar: Bar) => string | null } = $props();

  const peak = $derived(bars.reduce((m, b) => Math.max(m, b.value), 0));
</script>

<ul>
  {#each bars as b (b.key)}
    {@const to = link?.(b) ?? null}
    <li>
      {#if to}
        <a class="label" href={to}>{b.label}</a>
      {:else}
        <span class="label">{b.label}</span>
      {/if}
      <span class="lane">
        <span class="bar" style:width={`${peak === 0 ? 0 : (b.value / peak) * 100}%`}></span>
      </span>
      <span class="value">{b.value}</span>
    </li>
  {/each}
</ul>

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: grid;
    /* One shared right edge for the figures, which is what lets the column be
       read down as an axis. */
    grid-template-columns: minmax(0, 10rem) minmax(0, 1fr) 2.2rem;
    gap: 0.6rem;
    align-items: center;
    /* The row is the hit target; the bar itself is 8px and would be a pinpoint. */
    padding: 0.18rem 0;
  }
  li:hover .bar {
    filter: brightness(1.12);
  }
  .label {
    font-size: var(--t-fine);
    color: var(--ink);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  a.label:hover {
    color: var(--accent-text);
  }
  .lane {
    display: block;
    min-width: 0;
  }
  .bar {
    display: block;
    height: 8px;
    background: var(--accent);
    /* Rounded at the data end, square where it leaves the axis. */
    border-radius: 0 3px 3px 0;
    min-width: 2px;
  }
  .value {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
</style>
