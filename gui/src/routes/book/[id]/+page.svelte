<script lang="ts">
  import { page } from '$app/state';
  import { client, type StoredBook } from '$lib/api/client';
  import type { HighlightDto, NoteDto, ReadingDto } from '$lib/api/bindings';
  import { authorsLabel, progressDetail, readingStateLabel, titleLabel } from '$lib/phrasing';

  const id = $derived(Number(page.params.id));

  let book = $state<StoredBook | null>(null);
  let readings = $state<ReadingDto[]>([]);
  let highlights = $state<HighlightDto[]>([]);
  let notes = $state<NoteDto[]>([]);
  let missing = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
    const which = id;
    (async () => {
      const api = client();
      const b = await api.getBook(which);
      if (b === null) {
        missing = true;
        return;
      }
      book = b;
      // Four calls rather than one: the API has no request that returns a book
      // with its children, and inventing a client-side aggregate would hide that
      // from the audit. It is an item 18 line, recorded rather than worked around.
      [readings, highlights, notes] = await Promise.all([
        api.listReadings(which),
        api.listHighlights(which),
        api.listNotes(which),
      ]);
    })().catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });

  const cover = $derived(book ? client().coverSrc(book) : null);
</script>

<svelte:head><title>{book ? titleLabel(book.title) : 'Book'} — readingbuddy</title></svelte:head>

<!-- Nothing is a dead end: every screen shows its next move, and on a leaf that
     move is back to where you came from. -->
<a class="back" href="/">← Library</a>

{#if failure}
  <p class="note">Could not read this book: {failure}</p>
{:else if missing}
  <p class="note">There is no book with that id.</p>
  <p class="note dim">It may have been folded into another by a merge.</p>
{:else if book}
  <article>
    <div class="art">
      {#if cover}
        <img src={cover} alt="" />
      {:else}
        <span class="bare" aria-hidden="true"></span>
      {/if}
    </div>

    <div class="body">
      <h1>{titleLabel(book.title)}</h1>
      {#if authorsLabel(book.authors_display)}
        <!-- `authors_display`: the engine read the comma, this joins. -->
        <p class="by">{authorsLabel(book.authors_display)}</p>
      {/if}

      <dl>
        {#if readingStateLabel(book.reading_state)}
          <dt>State</dt>
          <dd>{readingStateLabel(book.reading_state)}</dd>
        {/if}
        {#if progressDetail(book.progress)}
          <!-- The long phrasing, because this screen has room for the page a
               reader recognises. The engine decided which numbers are in it: a
               percentage needs a denominator, and `page_count` is 0 for one book
               in the dev library and NULL for another; `Progress` normalises
               both to absence, so this says the page alone without knowing why.
               -->
          <dt>Progress</dt>
          <dd>{progressDetail(book.progress)}</dd>
        {:else if book.page_count !== null}
          <!-- Only where the progress line did not already carry it. -->
          <dt>Pages</dt>
          <dd>{book.page_count}</dd>
        {/if}
        {#if book.series_label}
          <!-- The engine's label, not the pair reassembled here: `series_index`
               is a REAL and two frontends formatting it will eventually print
               `#2.5` two ways. -->
          <dt>Series</dt>
          <dd>{book.series_label}</dd>
        {/if}
        {#if book.publisher}
          <dt>Publisher</dt>
          <dd>{book.publisher}{book.publish_year ? `, ${book.publish_year}` : ''}</dd>
        {/if}
        {#if book.isbn_13 ?? book.isbn_10}
          <dt>ISBN</dt>
          <dd>{book.isbn_13 ?? book.isbn_10}</dd>
        {/if}
        {#if book.subjects.length > 0}
          <dt>Subjects</dt>
          <dd>{book.subjects.join(' · ')}</dd>
        {/if}
      </dl>

      {#if book.description}
        <p class="blurb">{book.description}</p>
      {/if}

      {#if readings.length > 1}
        <!-- A reread is not an anomaly to flag. Shown only when there is more
             than one, because a single row reading "1" on every book is noise —
             the same condition the TUI's highlight read-gutter uses. -->
        <h2>Read {readings.length} times</h2>
      {/if}

      <h2>Highlights</h2>
      {#if highlights.length === 0}
        <p class="note dim">
          None here yet. <code>rb ko pull</code> brings across what is on a reader.
        </p>
      {:else}
        <ul class="highlights">
          {#each highlights as h (h.id)}
            <li>
              <p>{h.text}</p>
              <!-- KOReader does produce a highlight with neither, and an empty
                   span still takes a line. Absence gets no element. -->
              {#if h.chapter || h.page !== null}
                <span class="where">
                  {[h.chapter, h.page !== null ? `p. ${h.page}` : null].filter(Boolean).join(' · ')}
                </span>
              {/if}
              {#if h.ko_note}
                <p class="ko-note">{h.ko_note}</p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      <h2>Notes</h2>
      {#if notes.length === 0}
        <p class="note dim">Nothing written against this book yet.</p>
      {:else}
        <ul class="notes">
          {#each notes as n (n.id)}
            <li><span class="kind">{n.kind}</span> {n.title}</li>
          {/each}
        </ul>
      {/if}
    </div>
  </article>
{:else}
  <p class="note dim">Opening…</p>
{/if}

<style>
  .back {
    color: var(--ink-dim);
    font-size: 0.85rem;
    display: inline-block;
    margin-bottom: 1rem;
  }
  article {
    display: grid;
    grid-template-columns: 180px minmax(0, 1fr);
    gap: 1.75rem;
    align-items: start;
  }
  @media (max-width: 620px) {
    article {
      grid-template-columns: minmax(0, 1fr);
    }
    .art {
      max-width: 180px;
    }
  }
  .art {
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .bare {
    display: block;
    height: 100%;
    background: repeating-linear-gradient(
      -45deg,
      transparent,
      transparent 7px,
      var(--line) 7px,
      var(--line) 8px
    );
  }
  .body {
    min-width: 0;
  }
  h1 {
    font-size: 1.35rem;
    /* Long titles wrap rather than clip here: this is the one place the whole
       title has room, which is what makes clipping it on a tile acceptable. */
    overflow-wrap: anywhere;
  }
  .by {
    color: var(--ink-dim);
    margin: 0.25rem 0 1.25rem;
  }
  h2 {
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-dim);
    margin: 1.75rem 0 0.6rem;
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 0.2rem 1rem;
    margin: 0;
    font-size: 0.88rem;
  }
  dt {
    color: var(--ink-dim);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .blurb {
    max-width: var(--measure);
    color: var(--ink-dim);
    margin-top: 1.25rem;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--measure);
  }
  .highlights li {
    border-left: 2px solid var(--accent);
    padding: 0 0 0 0.85rem;
    margin-bottom: 1.1rem;
  }
  .highlights p {
    margin: 0;
  }
  .where {
    font-size: 0.75rem;
    color: var(--ink-dim);
  }
  .ko-note {
    font-size: 0.85rem;
    color: var(--ink-dim);
    margin-top: 0.3rem !important;
  }
  .notes li {
    padding: 0.2rem 0;
    font-size: 0.9rem;
  }
  .kind {
    color: var(--ink-dim);
    font-size: 0.75rem;
    margin-right: 0.4rem;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
  .dim {
    color: var(--ink-dim);
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85em;
    color: var(--accent);
  }
</style>
