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
    FlashcardDto,
    HighlightDto,
    NoteCitationsDto,
    NoteDto,
    ReadingDto,
  } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import About from '$lib/book/About.svelte';
  import MarkSearch from '$lib/book/MarkSearch.svelte';
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

  /**
   * The note a link asked for, or `null`.
   *
   * A plain function rather than a `$derived`: it seeds state once, on arrival.
   * Deriving it would re-open the note every time the query string was still
   * there — which is after every save — and a reader who closed the pane would
   * find it open again.
   */
  function paramNote(): number | null {
    const raw = page.url.searchParams.get('note');
    if (raw === null) return null;
    const n = Number(raw);
    return Number.isInteger(n) && n > 0 ? n : null;
  }

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
   *
   * **Seeded from `?note=`** (item 28), which is how a moment ends *in* the
   * reflection rather than beside it. The moment opens the note through
   * `OpenReflection` and lands here naming it, so the ceremony reuses this
   * pane instead of growing a second editor that would have to be fixed twice.
   * It is a URL rather than a store because it survives a reload — the axiom's
   * *state persists and is visible* — and because a link is the one way to
   * arrive somewhere that cannot be a dead end.
   *
   * An id naming a note this book does not have resolves to `null` and the pane
   * shows the list, which is the same answer a deleted note already gets.
   */
  let openNoteId = $state<number | null>(paramNote());
  const openNote = $derived(notes.find((n) => n.id === openNoteId) ?? null);

  /** Which passages the open note cites. **One** call, for the one open note. */
  let cited = $state<number[]>([]);

  /**
   * Which passages **each** of this book's notes cites — one call for the whole
   * page of notes (item 48).
   *
   * Held as the reply rather than as the union it draws, and that is the shape
   * that matters: keyed by note id, the open note's row can be corrected from
   * the `citationsFor` this page already makes after a toggle, so a click costs
   * no second batch. A bare `Set` would have to be rebuilt from the wire on
   * every cite, which is the N+1 arriving through the back door.
   */
  let noteCitations = $state<NoteCitationsDto[]>([]);
  /**
   * The mark: passages some note quotes.
   *
   * Set membership over the reply, which is what `NoteCitationsDto` says it is
   * for in as many words — *"a book's passage list wanting to mark the ones
   * already quoted somewhere"*. Not a derivation the engine is owed: no rule is
   * being invented, the union of ids is the ids.
   */
  const quoted = $derived(new Set(noteCitations.flatMap((c) => c.highlight_ids)));

  /** Every card captured from this book, so a passage can show what it gave up. */
  let flashcards = $state<FlashcardDto[]>([]);

  /**
   * The passage a search hit sent the reader to, or `null` (item 50).
   *
   * It stays marked after the jump rather than flashing and clearing: the axiom
   * asks that state persist and be visible, and a reader who scrolls away and
   * back has to be able to find the passage they were sent to. The next hit
   * moves it; nothing else does.
   */
  let found = $state<number | null>(null);

  /**
   * Take the reader to a passage in the band below.
   *
   * The scroll is done here, from the click, rather than in an effect on the
   * prop — the same hit clicked twice sets the same id, and an effect would not
   * run the second time, which is exactly the moment a reader is asking to be
   * taken back. `listHighlights` is unlimited and the search is scoped to this
   * book, so the element is always one this page drew; the optional chain is
   * for the frame before it has.
   */
  function showPassage(highlightId: number) {
    found = highlightId;
    document.getElementById(`passage-${highlightId}`)?.scrollIntoView({ block: 'center' });
  }

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
      const [rs, hs, ns] = await Promise.all([
        api.listReadings(which),
        api.listHighlights(which),
        api.listNotes(which),
      ]);
      [readings, highlights, notes] = [rs, hs, ns];
      // The reference material. A library that loaded must not be replaced by
      // an error thrown by the ornament beneath it — item 26 made that call for
      // the reading strip and it is the same call. The two marks on the
      // passages band are ornaments by the same test: a book with its
      // highlights on screen and no ticks on them is a lesser page, not a
      // broken one.
      reloadCitations(ns).catch(() => (noteCitations = []));
      reloadCards(which).catch(() => (flashcards = []));
      api.bookTags(which).then(
        (t) => (tags = t),
        () => {},
      );
      api.bookFiles(which).then(
        (f) => (files = f),
        () => {},
      );
      api.fieldProvenance(which).then(
        (p) => (provenance = p),
        () => {},
      );
    })().catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });

  /**
   * The quoted mark, for a known page of notes. **One call, never one per note.**
   *
   * Asked with the ids in hand rather than by book, because the request has no
   * `book_id` and answers exactly what it was asked: an empty note list is an
   * empty mark and needs no round trip to establish that.
   */
  async function reloadCitations(ns: NoteDto[]): Promise<void> {
    noteCitations = ns.length === 0 ? [] : await client().citationsForNotes(ns.map((n) => n.id));
  }

  async function reloadCards(which: number): Promise<void> {
    flashcards = await client().listFlashcardsForBook(which);
  }

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
    // Deleting a note takes its citations with it, so the mark is refreshed
    // with the list rather than left claiming a quote nothing makes any more.
    await reloadCitations(notes);
  }

  async function toggleCite(highlightId: number, on: boolean) {
    const note = openNote;
    if (note === null) return;
    const api = client();
    if (on) await api.cite(note.id, highlightId);
    else await api.uncite(note.id, highlightId);
    const now = (await api.citationsFor(note.id)).map((h) => h.id);
    cited = now;
    // The open note's row of the batch, corrected from the reply already in
    // hand. The mark must move with the toggle — a passage just cited is
    // quoted — and re-asking `citationsForNotes` on every click would be paying
    // for the whole page of notes to learn one note's answer.
    const row = { note_id: note.id, highlight_ids: now };
    noteCitations = noteCitations.some((c) => c.note_id === note.id)
      ? noteCitations.map((c) => (c.note_id === note.id ? row : c))
      : [...noteCitations, row];
  }

  /**
   * Capture a word off a passage (item 49).
   *
   * The list is re-asked rather than patched from what was sent, and `false` is
   * exactly why: `CreateFlashcard` answers a bool, and on a repeat the card
   * that exists may carry a different passage and a different context than the
   * one just offered. Synthesizing one here would draw a card the database does
   * not have.
   */
  async function capture(highlightId: number, word: string, context: string): Promise<boolean> {
    const created = await client().createFlashcard({
      bookId: id,
      highlightId,
      word,
      context,
    });
    await reloadCards(id);
    return created;
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
    It may have been folded into another by a merge — <code>rb book list</code> shows what is there now.
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
      <!-- Above both bands, because what it searches is drawn by both of them
           (item 50). It belongs to the column, not to a heading. -->
      <MarkSearch
        bookId={id}
        marks={notes.length + highlights.length}
        onopennote={(n) => (openNoteId = n)}
        onshowpassage={showPassage}
      />

      <NotePane
        bookId={id}
        {notes}
        open={openNote}
        onopen={(n) => (openNoteId = n)}
        onreload={reloadNotes}
      />

      <Passages
        {highlights}
        open={openNote}
        {cited}
        {quoted}
        {found}
        cards={flashcards}
        oncite={toggleCite}
        onannotate={annotate}
        oncapture={capture}
      />
    </div>

    <aside class="reference">
      {#if readings.length > 0}
        <section class="band">
          <!-- Past tense, always: when you read it, and how far you got. A
               reread gets a row per reading rather than a badge saying it is a
               reread, and the progress on each row is **that** reading's —
               putting the current page under a read that closed in January is
               what `Progress::of_book` warns about. -->
          <div class="band-head">
            <h2 class="band-title">{readings.length > 1 ? 'Reads' : 'Read'}</h2>
            <!-- The card, which is per reading — so a reread has two and they
                 sit side by side. Its own route rather than a band here: a card
                 carries a passage, and a passage wants a measure this column
                 does not have. -->
            <a class="cards" href={`/book/${id}/cards`}>Cards →</a>
          </div>
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

  .band-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
    margin-bottom: 0.3rem;
  }
  .cards {
    font-size: 0.8rem;
    color: var(--accent-text);
    flex: none;
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
