<script lang="ts">
  /**
   * What a period held — the one place counts are allowed.
   *
   * They are allowed because this is a page you chose to open. Everything in it
   * is past tense and about what happened; there is no target beside any figure,
   * nothing framed as remaining, and nothing to beat.
   *
   * ## Two kinds of number, and the difference is the whole component
   *
   * `books_finished`, `activity_days`, `notes_created` and `links_created` come
   * out of tables this app **originates**, so a zero in one is knowable and
   * prints as a zero. `minutes` and `pages` come off a device that may never
   * have existed, so they are `Option` at every level and an absence prints as
   * an absence. Rendering the second kind like the first is the failure item 42
   * was built to prevent, one grain up.
   *
   * ## `activity_days` is not a streak, and must never become one
   *
   * It is *"days the period holds at least one event"* over a range you asked
   * for — past tense, bounded, and not consecutive. A "current streak" derived
   * from it would be a threshold announced in advance wearing a costume, and it
   * is the nearest wrong turn on this screen. The label below says *with
   * something on them* rather than *in a row* for exactly that reason.
   */
  import type { ActivitySummaryDto } from '$lib/api/bindings';
  import { minutesLabel, NOT_MEASURED } from '$lib/phrasing';

  let { summary }: { summary: ActivitySummaryDto } = $props();

  type Figure = { value: string; label: string; measured: boolean };

  const figures = $derived<Figure[]>([
    { value: String(summary.books_finished), label: 'books finished', measured: true },
    // Not "in a row". See the module doc — this is the argued label.
    { value: String(summary.activity_days), label: 'days with something on them', measured: true },
    { value: String(summary.notes_created), label: 'notes written', measured: true },
    { value: String(summary.links_created), label: 'links between them', measured: true },
    {
      value: minutesLabel(summary.minutes),
      label: 'read on a device',
      measured: summary.minutes !== null,
    },
    {
      // The bare number, because the label carries the noun here. `pagesLabel`
      // is the *month row's* phrasing of the same value, where a chip has no
      // label beside it to say what it counts — two phrasings of one value, item
      // 17b's frontend half working as intended.
      value: summary.pages === null ? NOT_MEASURED : String(summary.pages),
      label: 'pages turned on a device',
      measured: summary.pages !== null,
    },
  ]);
</script>

<dl>
  {#each figures as f (f.label + f.value)}
    <div class="figure" class:absent={!f.measured}>
      <dt>{f.value}</dt>
      <dd>{f.label}</dd>
    </div>
  {/each}
</dl>
{#if summary.minutes === null || summary.pages === null}
  <!-- Names where the number would have come from, rather than apologising for
       not having it. `.hint` is the shared token: a dim line under a thing that
       explains it, never a count of what is undone. -->
  <p class="hint">
    Minutes and pages are the reader's own — <code>rb ko stats</code> brings them across from a
    device that keeps them. Everything else here was recorded by this app.
  </p>
{/if}

<style>
  dl {
    display: grid;
    /* Wraps down to one or two columns on a phone rather than shrinking six
       figures into unreadable columns. The floor is the width of the longest
       *label*, not of a number, because the label is what makes the number mean
       anything. */
    grid-template-columns: repeat(auto-fit, minmax(min(9.5rem, 100%), 1fr));
    gap: 1rem 1.4rem;
    margin: 0 0 0.9rem;
    max-width: calc(var(--column) + 12rem);
  }
  /*
   * Three and three above a phone, rather than whatever `auto-fit` lands on.
   *
   * There are six figures and `auto-fit` gave four then two, which reads as a
   * row that ran out rather than as a block. Six divides by three, so the shape
   * is stated once the width is there to state it — the sort of thing a
   * screenshot settles and reasoning about the CSS does not.
   */
  @media (min-width: 40rem) {
    dl {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      max-width: 36rem;
    }
  }
  .figure {
    min-width: 0;
  }
  dt {
    font-size: 1.5rem;
    line-height: 1.15;
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }
  dd {
    margin: 0.15rem 0 0;
    font-size: 0.82rem;
    color: var(--ink-dim);
  }
  /*
   * An absence is not a small number and must not read as one.
   *
   * Dim and italic, at body size rather than at figure size — the same voice
   * `BookTile` gives *Untitled*, for the same reason: it is our word for
   * something that is not there, and setting it in the type reserved for
   * measurements would make "not measured" look like a value.
   */
  .absent dt {
    font-size: 0.95rem;
    font-style: italic;
    color: var(--ink-dim);
    /* Sits on the same baseline as the figures beside it rather than floating
       in the middle of a taller row. */
    padding-top: 0.5rem;
  }
</style>
