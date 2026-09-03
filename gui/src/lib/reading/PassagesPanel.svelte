<script lang="ts">
  /**
   * What has come across from the device, for this read.
   *
   * ## The order is the engine's and nothing here touches it
   *
   * `highlights_for_reading` returns the marks of **one read** in the order a
   * book reads them. This renders that list as given: no sort, no group, no
   * filter, no *newest first*. Item 17's line — ordering is a derived fact and
   * lives below the seam — and the specific trap it guards against here is that
   * the TUI shows the same passages and would then be showing them in a
   * different order, with neither app looking wrong.
   *
   * ## Reading-scoped, not book-scoped, and that is the answer to the question
   *
   * The verb asks *what has synced*, and the honest scope for that on this
   * surface is the pass you are in. `HighlightDto.reading_id` is `null` for a
   * mark the dates could not place, so this is a genuinely narrower list than
   * the book's — which is right: a passage from a read you finished in 2019 is
   * not something that just arrived.
   *
   * ## Both notes, kept apart
   *
   * `ko_note` is the device's and is rewritten toward the device on every pull.
   * `annotation` is the reader's and an import never touches it. That split is
   * the whole of the highlight-ownership seam in `docs/decisions.md`, so the two
   * are drawn as two things with two labels — never merged into "note", which
   * would make a line the device is about to overwrite look like a line the
   * reader owns.
   *
   * ## No count, and no editing
   *
   * The passages are not counted, here or on the verb. And nothing on this panel
   * writes: annotating a passage, citing it, taking a word off it are all acts
   * with a proper home on the book's page, and the link at the bottom goes
   * there. This surface is for looking.
   */
  import type { HighlightDto } from '$lib/api/bindings';

  let {
    passages,
    failed,
    bookId,
  }: {
    passages: HighlightDto[];
    failed: boolean;
    bookId: number;
  } = $props();

  /** `Chapter 9 · p. 640`, or whichever half of it the mark actually has. */
  function place(h: HighlightDto): string | null {
    const parts: string[] = [];
    if (h.chapter) parts.push(h.chapter);
    if (h.page !== null) parts.push(`p. ${h.page}`);
    return parts.length === 0 ? null : parts.join(' · ');
  }
</script>

<div class="passages">
  <h2 class="band-title">Come across</h2>

  {#if failed}
    <p class="refusal" role="alert">The passages did not load.</p>
    <p class="hint">
      The book is still here and everything else on this screen still works.
      <a href="/book/{bookId}">The book’s own page</a> shows them too.
    </p>
  {:else if passages.length === 0}
    <!-- An absence, rendered as an absence. Not "no passages yet", which would
         frame an unmarked read as a thing the reader owes. -->
    <p class="hint">
      Nothing from this read has come across. Marks arrive when the device syncs —
      <code>rb sync</code> pulls them.
    </p>
  {:else}
    <ul>
      {#each passages as h (h.id)}
        <li>
          <blockquote>{h.text}</blockquote>
          {#if place(h)}
            <p class="place">{place(h)}</p>
          {/if}
          {#if h.ko_note}
            <p class="said"><span class="who">Device</span>{h.ko_note}</p>
          {/if}
          {#if h.annotation}
            <p class="said"><span class="who you">You</span>{h.annotation}</p>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}

  <p class="hint">
    <a href="/book/{bookId}">The book’s page</a> is where a passage can be annotated, quoted or kept.
  </p>
</div>

<style>
  .passages {
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    /* The panel is a look, not a page. Past this it scrolls inside itself rather
       than pushing the book off the top of the window. */
    max-height: 48vh;
    overflow-y: auto;
  }
  li {
    border-left: 2px solid var(--line);
    padding-left: 0.8rem;
  }
  blockquote {
    margin: 0;
    /* The reader's own words carry the weight here; everything around them is
       dim. The passage is why the panel exists. */
    color: var(--ink);
  }
  .place {
    margin: 0.35rem 0 0;
    color: var(--ink-dim);
    font-size: var(--t-micro);
  }
  .said {
    margin: 0.4rem 0 0;
    color: var(--ink-dim);
    font-size: var(--t-fine);
  }
  .who {
    display: inline-block;
    margin-right: 0.45rem;
    font-size: var(--t-micro);
    color: var(--ink-dim);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0 0.3rem;
  }
  /* The reader's half of the ownership split, marked the way the book page marks
     it — the accent, because this line is theirs and is the one they can change. */
  .who.you {
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .refusal {
    margin: 0;
    font-size: var(--t-fine);
  }
  .hint {
    margin: 0;
    color: var(--ink-dim);
    font-size: var(--t-fine);
  }
  .hint a {
    color: var(--accent-text);
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.9em;
  }
</style>
