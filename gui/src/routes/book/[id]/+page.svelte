<script lang="ts">
  /**
   * The book, and the notes — item 27.
   *
   * The TUI's `ui/book.rs` is the reference for what belongs here and most of
   * its choices are right; what a page has that a pane did not is room, so the
   * four sections that were tabs there are **bands** here, all present at once.
   * The one thing that is not always present is a note's body, and that
   * replaces the note list *in place* rather than opening over it.
   *
   * ## The hero, and why it is a different file from the shelf's
   *
   * `cover_path`, not `cover_shelf_path`. The shelf tier exists because a grid
   * of sixty tiles must not fetch sixty full-size jackets; this screen shows one
   * book and wants the file nobody downscaled. Two client methods rather than a
   * flag, so the call site says which it meant.
   *
   * ## Eight calls for one book, recorded rather than worked around
   *
   * There is no request that returns a book with its children. Item 17 already
   * named this — *"the detail screen makes four calls for one book, which for a
   * list is eight hundred"* — and item 18 answered the list half with
   * `BookSummaries`. The detail half is still open, and inventing a client-side
   * aggregate would hide it from the next audit. So the calls are made in
   * parallel and grouped by what their failure means: the **book** failing is
   * this page failing, and the ornaments failing are not.
   */
  import { page } from '$app/state';
  import type {
    BookFileDto,
    BookTagDto,
    FieldSourceDto,
    HighlightDto,
    NoteDto,
    ReadingDto,
  } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import About from '$lib/book/About.svelte';
  import NotePane from '$lib/book/NotePane.svelte';
  import Passages from '$lib/book/Passages.svelte';
  import Jacket from '$lib/components/Jacket.svelte';
  import {
    authorsLabel,
    dayLabel,
    progressDetail,
    readingSpan,
    readingStateLabel,
    titleLabel,
  } from '$lib/phrasing';

  const id = $derived(Number(page.params.id));

  let book = $state<StoredBook | null>(null);
  let readings = $state<ReadingDto[]>([]);
  let highlights = $state<HighlightDto[]>([]);
  let notes = $state<NoteDto[]>([]);
  let tags = $state<BookTagDto[]>([]);
  let files = $state<BookFileDto[]>([]);
  let provenance = $state<FieldSourceDto[]>([]);
  let missing = $state(false);
  let failure = $state<string | null>(null);

  /**
   * Which note is open, by **id** rather than by value.
   *
   * The list is refetched after every write, so holding the `NoteDto` itself
   * would pin a stale object — a note whose title changed would keep the old
   * one, and a deleted note would stay open over nothing. An id resolved
   * against the current list makes both of those correct for free.
   */
  let openNoteId = $state<number | null>(null);
  const openNote = $derived(notes.find((n) => n.id === openNoteId) ?? null);

  /** Which passages the open note cites. **One** call, for the one open note. */
  let cited = $state<number[]>([]);

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
      // The reader's own material, which the page is not worth much without.
      [readings, highlights, notes] = await Promise.all([
        api.listReadings(which),
        api.listHighlights(which),
        api.listNotes(which),
      ]);
      // The reference material. A library that loaded must not be replaced by
      // an error thrown by the ornament beneath it — item 26 made that call for
      // the reading strip and it is the same call.
      api.bookTags(which).then((t) => (tags = t), () => {});
      api.bookFiles(which).then((f) => (files = f), () => {});
      api.fieldProvenance(which).then((p) => (provenance = p), () => {});
    })().catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });

  $effect(() => {
    const note = openNote;
    if (note === null) {
      cited = [];
      return;
    }
    client()
      .citationsFor(note.id)
      .then((hs) => (cited = hs.map((h) => h.id)))
      .catch(() => (cited = []));
  });

  const hero = $derived(book ? client().heroSrc(book) : null);
  // Not `state`: a top-level `const state` in a rune file shadows the `$state`
  // rune for svelte-check, which reports it as two dozen errors on the *other*
  // lines. `BookTile` gets away with the name because it declares no `$state`.
  const stateWord = $derived(book ? readingStateLabel(book.reading_state) : null);
  const progressWord = $derived(book ? progressDetail(book.progress) : null);

  async function reloadNotes() {
    notes = await client().listNotes(id);
  }

  async function toggleCite(highlightId: number, on: boolean) {
    if (openNote === null) return;
    const api = client();
    if (on) await api.cite(openNote.id, highlightId);
    else await api.uncite(openNote.id, highlightId);
    cited = (await api.citationsFor(openNote.id)).map((h) => h.id);
  }

  async function annotate(highlightId: number, text: string | null) {
    await client().setAnnotation(highlightId, text);
    highlights = await client().listHighlights(id);
  }
</script>

<svelte:head><title>{book ? titleLabel(book.title) : 'Book'} — readingbuddy</title></svelte:head>

<!-- Nothing is a dead end: every screen shows its next move, and on a leaf that
     move is back to where you came from. -->
<a class="back" href="/">← Library</a>

{#if failure}
  <p class="note">This book did not open: {failure}</p>
  <p class="hint">
    The library itself may still be fine — <code>rb book list</code> reads the same database, and
    <code>rb show {id}</code> reads this row.
  </p>
{:else if missing}
  <p class="note">There is no book with that id.</p>
  <p class="hint">
    It may have been folded into another by a merge — <code>rb book list</code> shows what is
    there now.
  </p>
{:else if book}
  <article>
    <div class="art">
      <!-- The hero shot, and the same three states the shelf tile has: bytes, a
           plate in this jacket's own colour, or the hatch. One composition, in
           `Jacket`, so a coverless book cannot look like two different books. -->
      <Jacket src={hero} accent={book.cover_accent} />
    </div>

    <div class="identity">
      <!-- Dimmed and italic for exactly `BookTile`'s reason: *Untitled* is our
           word for an absence, not a book that is called that, and styling it
           like a real title on one screen while the shelf marks it on another
           is one absence with two voices. The absence is `book.title`; the word
           is `titleLabel`'s and is deliberately indistinguishable once made. -->
      <h1 class:untitled={!book.title || book.title.trim() === ''}>{titleLabel(book.title)}</h1>
      {#if authorsLabel(book.authors_display)}
        <!-- `authors_display`: the engine read the comma, this joins. -->
        <p class="by">{authorsLabel(book.authors_display)}</p>
      {/if}
      {#if stateWord || progressWord}
        <p class="state">{[stateWord, progressWord].filter(Boolean).join(' · ')}</p>
      {/if}
    </div>
  </article>

  <!--
    Two columns, and the split is by *whose* they are rather than by size: what
    you wrote is the page, and what is known about the book is the margin. At
    one column the same order still reads top to bottom, which is why the
    reference half is last in the markup.
  -->
  <div class="columns">
    <div class="yours">
      <NotePane
        bookId={id}
        {notes}
        open={openNote}
        onopen={(n) => (openNoteId = n)}
        onreload={reloadNotes}
      />

      <Passages {highlights} open={openNote} {cited} oncite={toggleCite} onannotate={annotate} />
    </div>

    <aside class="reference">
      {#if readings.length > 0}
        <section class="band">
          <!-- Past tense, always: when you read it, and how far you got. A
               reread gets a row per reading rather than a badge saying it is a
               reread, and the progress on each row is **that** reading's —
               putting the current page under a read that closed in January is
               what `Progress::of_book` warns about. -->
          <h2 class="band-title">{readings.length > 1 ? 'Reads' : 'Read'}</h2>
          <ul class="readings">
            {#each readings as r (r.id)}
              <li>
                <span class="when">{readingSpan(r) ?? dayLabel(r.created_at)}</span>
                <span class="row2">
                  {#if readingStateLabel(r.status)}
                    <span class="how">{readingStateLabel(r.status)}</span>
                  {/if}
                  {#if progressDetail(r.progress)}
                    <span class="far">{progressDetail(r.progress)}</span>
                  {/if}
                  <!-- The writer's name, shown rather than branched on — it
                       grows by one per importer and nothing decides on it. -->
                  <span class="src">{r.source}</span>
                </span>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      <About {book} {tags} {files} {provenance} />
    </aside>
  </div>
{:else}
  <p class="hint">Opening…</p>
{/if}

<style>
  .back {
    color: var(--ink-dim);
    font-size: 0.85rem;
    display: inline-block;
    margin-bottom: 1.1rem;
  }
  article {
    display: grid;
    grid-template-columns: 150px minmax(0, 1fr);
    gap: 1.6rem;
    align-items: end;
    padding-bottom: 1.4rem;
    border-bottom: 1px solid var(--line);
  }
  @media (max-width: 620px) {
    article {
      grid-template-columns: minmax(0, 1fr);
      align-items: start;
    }
    .art {
      max-width: 130px;
    }
  }
  .art {
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border-radius: var(--radius);
    overflow: hidden;
    /* The tile's own lift, so a jacket sits on the page rather than being
       printed on it. Same shadow as `BookTile`'s `.art`, without the hover:
       nothing here is a link. */
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent),
      0 1px 2px rgb(0 0 0 / 0.2),
      0 6px 14px -8px rgb(0 0 0 / 0.45);
  }
  .identity {
    min-width: 0;
  }
  h1 {
    font-size: 1.5rem;
    line-height: 1.25;
    /* The whole title wraps here rather than clipping. This is the one place it
       has room, which is what makes clipping it on a tile acceptable. */
    overflow-wrap: anywhere;
  }
  h1.untitled {
    color: var(--ink-dim);
    font-style: italic;
  }
  .by {
    color: var(--ink-dim);
    margin: 0.35rem 0 0;
  }
  .state {
    color: var(--accent-text);
    font-size: 0.85rem;
    margin: 0.5rem 0 0;
  }

  /*
   * What you wrote, and what is known about the book.
   *
   * One column until there is genuinely room for two. `--measure` caps prose at
   * 68ch, so below ~1024px a sidebar would be taking width the passages need
   * rather than width nobody was using — which is the version of this that
   * reads as a squeezed page instead of a laid-out one.
   */
  .columns {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 0 3rem;
  }
  @media (min-width: 1024px) {
    .columns {
      grid-template-columns: minmax(0, var(--measure)) minmax(0, 20rem);
      align-items: start;
    }
    /* The hero's rule stops where the columns stop. A divider that overshoots
       the content it divides reads as a layout bug rather than as a decision. */
    article,
    .columns {
      max-width: calc(var(--measure) + 23rem);
    }
    /* The reference column starts level with the first band beside it rather
       than a band's margin lower. */
    .reference {
      margin-top: 0;
    }
  }
  .yours,
  .reference {
    min-width: 0;
  }

  /* Each band owns its own top margin, here and in the three components — and
     deliberately **not** as a `:global(.band)` rule from this file, which would
     be one screen's spacing leaking onto the shelf. */
  section.band {
    margin-top: 2.2rem;
  }

  ul.readings {
    list-style: none;
    padding: 0;
    margin: 0;
    font-size: 0.85rem;
  }
  ul.readings li {
    padding: 0.35rem 0;
    border-bottom: 1px solid var(--line);
  }
  ul.readings li:last-child {
    border-bottom: 0;
  }
  .row2 {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
    color: var(--ink-dim);
  }
  .how {
    color: var(--accent-text);
  }
  .far,
  .src {
    color: var(--ink-dim);
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
</style>
