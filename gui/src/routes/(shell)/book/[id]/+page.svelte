<script lang="ts">
  /**
   * The book — the desk, and where the time goes, so it gets the room.
   *
   * ## Three columns, and they are justified by two different arguments
   *
   * They are not one move, and separating them says which one is fragile.
   *
   * **The right rail is an inspector.** Canvas-plus-inspector inverts
   * master–detail: the centre is the work and the rails are instruments. The
   * *Link to…* search that writes `[[Title]]` at the cursor is not reference
   * material beside the editor — it is a tool acting *on* it, and writing a note
   * and finding the note to link to is one operation. That is what justifies
   * permanent screen area.
   *
   * **The left rail is a mode selector**, which is a different argument: it turns
   * an unbounded set of destinations into a fixed column and stages the density,
   * so at rest you see one work surface rather than all of them. The centre
   * swaps; the rail is how you swap it. Nothing is modal, because every other
   * destination is on screen while you are in any one of them.
   *
   * The count against it is real and is recorded rather than argued away: header
   * plus two rails plus centre is four regions, and attended regions run to about
   * three plus periphery. The mitigation is that the right rail's contents are
   * conditional on the centre, so at any moment you are attending the centre plus
   * *one* rail — and the obvious next move, demoting the left rail while a note is
   * open, needs a visual change this phase does not take.
   *
   * ## The header lost 130px and the state lost its colour
   *
   * The hero jacket goes from 150px to 52px and the stacked identity becomes one
   * metadata line. This is epicenter design and the argument is old: **chrome is
   * cheap to add and expensive to remove, because a region that exists acquires
   * occupants** — every feature with no natural home gets filed there. A book
   * page is not a product page; you already know which book you opened.
   *
   * The state and progress fragment is `--ink-dim` rather than `--accent-text`,
   * which is one of eight jobs the accent was doing at once. The rule adopted
   * instead: **the accent is for state that is true right now and that you can
   * act on** — selection, focus, the current page, progress, the primary action.
   * Everything descriptive is carried by ink, dim, position and weight.
   *
   * ## Eight calls for one book, recorded rather than worked around
   *
   * There is no request that returns a book with its children. Item 17 named it
   * and item 18 answered the list half; the detail half is still open, and a
   * client-side aggregate would hide it from the next audit. So the calls are
   * made in parallel and grouped by what their failure *means*: the **book**
   * failing is this page failing, and the ornaments failing are not.
   */
  import { afterNavigate } from '$app/navigation';
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
  import Composer from '$lib/book/Composer.svelte';
  import Connections from '$lib/book/Connections.svelte';
  import { type Centre, inspects } from '$lib/book/desk';
  import Editor from '$lib/book/Editor.svelte';
  import Passages from '$lib/book/Passages.svelte';
  import Rail from '$lib/book/Rail.svelte';
  import Jacket from '$lib/components/Jacket.svelte';
  import { backTarget } from '$lib/nav';
  import {
    authorsLabel,
    countLabel,
    dayLabel,
    progressDetail,
    readingSpan,
    readingStateLabel,
    titleLabel,
  } from '$lib/phrasing';

  const id = $derived(Number(page.params.id));

  /**
   * Where the back link goes: the page this one was opened from.
   *
   * This page is a leaf reached from four places — the wall, reading mode, a
   * moment and the vault — so a fixed *← Library* sent three of those four
   * readers somewhere they had not been. `?note=` and `?compose=1` navigate to
   * this same path and are ignored, so opening a note does not make *back* mean
   * *this book*.
   */
  let from = $state<URL | null>(null);
  const back = $derived(backTarget(page.url, from));

  afterNavigate((nav) => {
    const previous = nav.from?.url ?? null;
    if (previous !== null && previous.pathname === nav.to?.url.pathname) return;
    from = previous;
  });

  /**
   * What the URL asked for, read once on arrival.
   *
   * Plain functions rather than `$derived`: they seed state. Deriving would
   * re-open the note every time the query string was still there — which is
   * after every save — and a reader who moved to the passages would find the
   * note open again.
   */
  function paramNote(): number | null {
    const raw = page.url.searchParams.get('note');
    if (raw === null) return null;
    const n = Number(raw);
    return Number.isInteger(n) && n > 0 ? n : null;
  }

  function paramCompose(): boolean {
    return page.url.searchParams.get('compose') !== null;
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
   * one, and a deleted note would stay open over nothing. An id resolved against
   * the current list makes both of those correct for free.
   *
   * **Seeded from `?note=`** (item 28), which is how a moment ends *in* the
   * reflection rather than beside it. It is a URL rather than a store because it
   * survives a reload — the axiom's *state persists and is visible* — and
   * because a link is the one way to arrive somewhere that cannot be a dead end.
   */
  let openNoteId = $state<number | null>(paramNote());

  /**
   * What the centre is showing — and it is **independent of which note is open**.
   *
   * That independence is the whole of how citing works. `Cite` needs a note to
   * cite *into* and a passage to cite *from*, and only one of them can be the
   * work surface; if opening a note closed the passage list, the gesture the
   * mouse makes available would be unreachable. So the note stays open — named
   * in the rail, marked as current, cited into from the passage list — while the
   * centre shows whatever you last asked it for.
   *
   * Seeded from the URL for the same reason `?note=` is a URL: a moment ends
   * *in* the reflection, and the state survives a reload.
   */
  let centre = $state<Centre>(paramNote() !== null ? 'note' : paramCompose() ? 'compose' : 'passages');

  const openNote = $derived(notes.find((n) => n.id === openNoteId) ?? null);

  /** Which passages the open note cites. **One** call, for the one open note. */
  let cited = $state<number[]>([]);
  const citedPassages = $derived(highlights.filter((h) => cited.includes(h.id)));

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
  const quoted = $derived(new Set(noteCitations.flatMap((c) => c.highlight_ids)));

  /** Every card captured from this book, so a passage can show what it gave up. */
  let flashcards = $state<FlashcardDto[]>([]);

  /**
   * The passage a search hit sent the reader to, or `null` (item 50).
   *
   * It stays marked after the jump rather than flashing and clearing: the axiom
   * asks that state persist and be visible, and a reader who scrolls away and
   * back has to be able to find the passage they were sent to.
   */
  let found = $state<number | null>(null);

  /**
   * How the right rail writes into the editor, or `null` when none is open.
   *
   * The editor hands this out while it is mounted and takes it back when it is
   * not, so the *Link to…* search cannot write into a box that has gone — and
   * the rail never touches a DOM node it does not own.
   */
  let insert = $state<((text: string) => void) | null>(null);

  /**
   * Take the reader to a passage in the centre column.
   *
   * The scroll is done from the click rather than in an effect on a prop — the
   * same hit clicked twice sets the same id, and an effect would not run the
   * second time, which is exactly the moment a reader is asking to be taken
   * back. The passages have to be the centre for there to be anything to scroll
   * to, so this says so rather than scrolling into a column that is not there.
   */
  function showPassage(highlightId: number) {
    found = highlightId;
    centre = 'passages';
    // After the centre has swapped, since the element may not exist until then.
    queueMicrotask(() =>
      document.getElementById(`passage-${highlightId}`)?.scrollIntoView({ block: 'center' }),
    );
  }

  function show(next: Centre) {
    centre = next;
  }

  function openNoteById(next: number) {
    openNoteId = next;
    centre = 'note';
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
      // The reference material. A book that loaded must not be replaced by an
      // error thrown by an ornament beneath it — the two marks on the passages
      // are ornaments by that test: a book with its highlights on screen and no
      // ticks on them is a lesser page, not a broken one.
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
  // lines.
  const stateWord = $derived(book ? readingStateLabel(book.reading_state) : null);
  const progressWord = $derived(book ? progressDetail(book.progress) : null);

  /**
   * The one metadata line under the title.
   *
   * Everything the stacked identity block used to say, in the order a
   * bibliographic record says it and then the state you are in. `filter(Boolean)`
   * rather than a chain of `{#if}`s, so an absent publisher or an unmeasured
   * length simply is not there — no empty separator, no *unknown*.
   */
  const line = $derived(
    !book
      ? []
      : [
          authorsLabel(book.authors_display),
          book.publish_year === null ? null : String(book.publish_year),
          book.page_count === null || book.page_count === 0
            ? null
            : countLabel(book.page_count, 'page'),
          stateWord,
          progressWord,
        ].filter((x): x is string => Boolean(x)),
  );

  /**
   * The reads, as lines for the right rail's readout.
   *
   * Past tense, always: when you read it and how far you got. A reread gets a
   * line per reading rather than a badge saying it is a reread, and the progress
   * on each is **that** reading's — putting today's page under a read that closed
   * in January is what `Progress::of_book` warns about.
   */
  const readLines = $derived(
    readings.map((r) =>
      [
        readingSpan(r) ?? dayLabel(r.created_at),
        readingStateLabel(r.status),
        progressDetail(r.progress),
      ]
        .filter(Boolean)
        .join(' · '),
    ),
  );

  async function reloadNotes() {
    notes = await client().listNotes(id);
    // Deleting a note takes its citations with it, so the mark is refreshed with
    // the list rather than left claiming a quote nothing makes any more.
    await reloadCitations(notes);
  }

  /** Open **or mint** — one call, and the engine decides which reading it hangs off. */
  async function anchored(kind: 'reflection' | 'review') {
    try {
      const api = client();
      const created =
        kind === 'reflection' ? await api.openReflection(id) : await api.openReview(id);
      await reloadNotes();
      openNoteById(created.id);
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    }
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
    // hand. The mark must move with the toggle — a passage just cited is quoted
    // — and re-asking `citationsForNotes` on every click would be paying for the
    // whole page of notes to learn one note's answer.
    const row = { note_id: note.id, highlight_ids: now };
    noteCitations = noteCitations.some((c) => c.note_id === note.id)
      ? noteCitations.map((c) => (c.note_id === note.id ? row : c))
      : [...noteCitations, row];
  }

  /**
   * Capture a word off a passage (item 49).
   *
   * The list is re-asked rather than patched from what was sent, and `false` is
   * exactly why: `CreateFlashcard` answers a bool, and on a repeat the card that
   * exists may carry a different passage and a different context than the one
   * just offered. Synthesizing one here would draw a card the database does not
   * have.
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
     move is back to where you came from — **literally**, since this page is
     reached from the wall, from reading mode, from a moment and from the vault.
     `$lib/nav.ts` names the page and falls back to the entrance on a reload,
     where there is no previous page to name. -->
<a class="back" href={back.href}>← {back.label}</a>

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
  <header class="ident">
    <div class="art">
      <!-- The same three states the wall's tile has: bytes, a plate in this
           jacket's own colour, or the hatch. One composition, in `Jacket`, so a
           coverless book cannot look like two different books. -->
      <Jacket src={hero} accent={book.cover_accent} />
    </div>
    <div class="who">
      <!-- Dimmed and italic for exactly `BookTile`'s reason: *Untitled* is our
           word for an absence, not a book that is called that. -->
      <h1 class:untitled={!book.title || book.title.trim() === ''}>{titleLabel(book.title)}</h1>
      {#if line.length > 0}
        <p class="line">{line.join(' · ')}</p>
      {/if}
    </div>
  </header>

  <div class="desk">
    <Rail
      bookId={id}
      {notes}
      {centre}
      {openNoteId}
      onshow={show}
      onopen={openNoteById}
      oncompose={() => (centre = 'compose')}
      onanchored={anchored}
    />

    <div class="work">
      {#if centre === 'note' && openNote}
        <Editor
          note={openNote}
          onreload={reloadNotes}
          onclose={() => {
            openNoteId = null;
            centre = 'passages';
          }}
          onready={(fn) => (insert = fn)}
        />
      {:else if centre === 'compose'}
        <Composer
          bookId={id}
          oncancel={() => (centre = 'passages')}
          onwritten={async (noteId) => {
            await reloadNotes();
            openNoteById(noteId);
          }}
        />
      {:else if centre === 'reads'}
        {#if readings.length === 0}
          <p class="note">No reading recorded for this book.</p>
          <p class="hint">
            <code>rb read start</code> opens one, and <code>rb ko pull</code> takes what a connected reader
            already knows.
          </p>
        {:else}
          <ul class="readings">
            {#each readings as r (r.id)}
              <li>
                <span class="when">{readingSpan(r) ?? dayLabel(r.created_at)}</span>
                <span class="row2">
                  {#if readingStateLabel(r.status)}
                    <span>{readingStateLabel(r.status)}</span>
                  {/if}
                  {#if progressDetail(r.progress)}
                    <span>{progressDetail(r.progress)}</span>
                  {/if}
                  <!-- The writer's name, shown rather than branched on — it grows
                       by one per importer and nothing decides on it. -->
                  <span class="src">{r.source}</span>
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if centre === 'about'}
        <About {book} {tags} {files} {provenance} />
      {:else}
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
      {/if}
    </div>

    <!-- The rail follows the **centre**, not the open note: with the passage list
         on the work surface it answers about the book, even though a note is
         still open to cite into. That is what `inspects` names. -->
    <Connections
      bookId={id}
      note={inspects(centre) === 'note' ? openNote : null}
      cited={citedPassages}
      marks={notes.length + highlights.length}
      reads={readLines}
      oninsert={insert}
      onopennote={openNoteById}
      onshowpassage={showPassage}
    />
  </div>
{:else}
  <p class="hint">Opening…</p>
{/if}

<style>
  .back {
    color: var(--ink-dim);
    font-size: 0.85rem;
    display: inline-block;
    margin-bottom: 0.9rem;
  }
  .ident {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 1rem;
    align-items: center;
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--line);
    margin-bottom: 1.6rem;
  }
  .art {
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border-radius: var(--radius);
    overflow: hidden;
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent),
      0 1px 2px rgb(0 0 0 / 0.2);
  }
  .who {
    min-width: 0;
  }
  h1 {
    font-size: 1.25rem;
    line-height: 1.25;
    /* The whole title wraps here rather than clipping. This is the one place it
       has room, which is what makes clipping it on a tile acceptable. */
    overflow-wrap: anywhere;
  }
  h1.untitled {
    color: var(--ink-dim);
    font-style: italic;
  }
  .line {
    margin: 0.2rem 0 0;
    font-size: 0.82rem;
    color: var(--ink-dim);
    overflow-wrap: anywhere;
  }

  /*
   * Where you navigate, what you are doing, what it connects to.
   *
   * Both rails are sticky and the centre is capped, so the work surface keeps a
   * measure while the window keeps growing. `align-items: start` rather than
   * `stretch`, because a sticky child of a stretched grid item has nothing to
   * stick within.
   */
  .desk {
    display: grid;
    grid-template-columns: var(--rail) minmax(0, 1fr) var(--rail-r);
    gap: 0 2.6rem;
    align-items: start;
  }
  .desk > :global(.rail),
  .desk > :global(.rrail) {
    position: sticky;
    top: 1.5rem;
    /* A rail longer than the window has to be able to reach its own bottom. */
    max-height: calc(100vh - 3rem);
    overflow-y: auto;
  }
  .work {
    min-width: 0;
  }

  /*
   * Two breakpoints, and they drop in this order: context before navigation.
   *
   * At ≤1180 the right rail unsticks and folds under the centre — an inspector
   * is what a narrow window can least afford, and it is the one region whose
   * contents are conditional anyway. The left rail stays, because it is how you
   * get anywhere on this page.
   *
   * At ≤860 the left rail stacks above the centre. **`grid-column` must be reset
   * to `auto` here**: the 1180 rule puts the folded rail in column 2, and
   * leaving that in place at one column conjures an implicit second track — the
   * "one column" layout then silently renders as two. That was a real bug in the
   * prototype, found by screenshotting at 800px.
   */
  @media (max-width: 1180px) {
    .desk {
      grid-template-columns: var(--rail) minmax(0, 1fr);
      gap: 0 2rem;
    }
    .desk > :global(.rrail) {
      grid-column: 2;
      position: static;
      max-height: none;
      margin-top: 2.4rem;
      padding-top: 1.4rem;
      border-top: 1px solid var(--line);
    }
  }
  @media (max-width: 860px) {
    .desk {
      grid-template-columns: minmax(0, 1fr);
    }
    .desk > :global(.rail) {
      position: static;
      max-height: none;
      margin-bottom: 2rem;
    }
    .desk > :global(.rrail) {
      grid-column: auto;
    }
  }

  ul.readings {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--column);
    font-size: 0.9rem;
  }
  ul.readings li {
    padding: 0.5rem 0;
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
  .src {
    color: var(--ink-dim);
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
