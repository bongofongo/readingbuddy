<script lang="ts">
  /**
   * One card, for one reading.
   *
   * *"A card per reading, not per book. This falls straight out of the schema
   * and is better than the thing it falls out of: `readings` makes rereads
   * first-class, so reading Piranesi twice mints two cards, and the two sit side
   * by side showing what changed."* (`gui-vision.md:112`.) So this component
   * takes a `ReadingDto` and never a book's current state — `book.progress` is
   * the *current* read's and printing it under a read that closed in January is
   * exactly what `Progress::of_book` warns about.
   *
   * ## It is handed a row, and it draws one
   *
   * The prop is a whole `ReadingRow` (item 43) rather than a book and a reading,
   * because the cover, the dates, **which read this is** and **the passage** all
   * arrive together in one round trip. That is the shape that makes a wall of
   * four hundred of these affordable at all: the wall costs two requests a page
   * whatever the page size, and a card drawn from a row alone makes no request
   * of its own.
   *
   * ## Which read it is — no longer a prohibition
   *
   * This component used to refuse an ordinal, and the refusal was correct while
   * it lasted: `readings.indexOf(id) + 1` re-implements a domain rule *and*
   * silently re-acquires a dependency on `list_readings`' oldest-first ordering,
   * which is stated nowhere on the wire, with nothing on the screen looking
   * wrong. **Item 41 filled the gap**, so the rule is read rather than
   * reinvented: `read_number` and `of_reads` are on the row, counted by the
   * engine over every reading of the book, and the frontend's whole half is
   * `of_reads > 1` — the same test the TUI's gutter makes. See
   * `readOrdinalLabel`. The dates stay, because they are what a reader
   * remembers a read by.
   *
   * ## The passage is handed over, never picked
   *
   * Item 44's rule — longest, ties to the lowest id — is not restated here or
   * anywhere in `gui/`. `highlights[0]` is the thing this component exists not
   * to do: a frontend inventing a selection predicate, so that the day the TUI
   * grows a card the two apps show a different sentence for the same reading
   * with neither looking wrong. The row's `passage` comes from the same
   * `card_passage_order` a single `cardPassage` call uses, so the wall and one
   * card cannot disagree.
   *
   * ## `detail`, and the three calls behind it
   *
   * The rating and *what you left behind* are **not** on the row, and item 43
   * refused to put them there by name: a card would grow the rating and that row
   * will not. So they cost three requests per card — marks, notes, and the
   * review whose rating is a fourth hop — and this prop is where that cost is
   * decided rather than assumed. `/book/[id]/cards` sets it, because a book has
   * a handful of reads and the bound is a fact about reading rather than a page
   * size somebody chose. The wall does not, because the same code across a page
   * of cards is the N+1 item 44 wrote down in advance.
   *
   * That makes two card densities and it is the trade `docs/decisions.md` entry
   * 47 argues for: the wall draws the row, and the whole card is one click away
   * on the book that mints it.
   *
   * **`detail` decides one more thing since the minimal pass**: whether a read
   * with no passage says so. On the book's own page there are one or two cards
   * and *No passage from this read* is a fact about that read, worth a line. On
   * a wall it was printed under **most** cards on the page — the dev library
   * renders it twenty-four times out of twenty-four — where it stops being a
   * fact about any read and becomes a sentence the page repeats. Absence drawn
   * as absence is right; absence drawn twenty-four times is wallpaper.
   *
   * ## It is a box, and the minimal pass tried taking that away and put it back
   *
   * The card is a raised panel: `--bg-raised`, a hairline inset and two shadows,
   * so a card sits on the page as an object rather than being printed on it. The
   * minimal pass removed all three on the argument that a wall of twenty-four
   * panels is twenty-four borders, and that the jacket inside each already
   * carries the lift.
   *
   * **Looked at, that was wrong, and the reason is worth keeping.** A card is
   * not a row in a list — it is *one read of one book*, a composite of five
   * unlike things (a jacket, a title, a span of dates, a state, a passage
   * somebody else wrote) whose extent is not otherwise guessable. Whitespace
   * separates items whose shape repeats; it does not tell you where a ragged
   * composite ends, and without an edge the passage under card four reads as
   * though it might belong to card five. Structure is what makes the object
   * legible, and it is the thing the surface is actually made of.
   *
   * What the pass was right about is that the **contents** were shouting: a
   * brass state word on every card, and a repeated *No passage from this read*.
   * Those went. The box stayed.
   */
  import type { HighlightDto, NoteDto, RatingDto } from '$lib/api/bindings';
  import { client, type ReadingRow } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import {
    countLabel,
    dayLabel,
    noteKindLabel,
    progressDetail,
    ratingLabel,
    readingSpan,
    readingStateLabel,
    readOrdinalLabel,
    titleLabel,
  } from '$lib/phrasing';

  let { row, detail = false }: { row: ReadingRow; detail?: boolean } = $props();

  const book = $derived(row.book);
  const reading = $derived(row.reading);
  const passage = $derived(row.passage);
  const ordinal = $derived(readOrdinalLabel(row.read_number, row.of_reads));

  let marks = $state<HighlightDto[]>([]);
  let notes = $state<NoteDto[]>([]);
  let rating = $state<RatingDto | null>(null);
  /** Not asked / asked / answered — three, not a nullable. See item 27's finding. */
  let loaded = $state(false);

  // **`coverSrc`, not `heroSrc`** (item 20c, and item 47 is where it bit). The
  // art box is 84px; `cover_path` is the largest jacket a provider publishes, so
  // a wall of sixty cards was about to load sixty hero shots. `cover_shelf_path`
  // is the downscaled tier where one exists and the original where it does not,
  // and the engine is what decides which.
  const cover = $derived(client().coverSrc(book));
  const when = $derived(readingSpan(reading) ?? dayLabel(reading.created_at));
  // `stateWord`, not `state`: a top-level `const state` in a rune file shadows
  // the `$state` rune for svelte-check, which reports it as sixteen errors on
  // the *other* lines — item 27 recorded this and it cost a session then too.
  const stateWord = $derived(readingStateLabel(reading.status));
  const far = $derived(progressDetail(reading.progress));

  $effect(() => {
    const id = reading.id;
    const api = client();
    // Nothing is asked for a wall card, so nothing is `loaded` and the band
    // below never renders. `cardPassage` is **gone from here entirely** — the
    // passage is a prop now, which is the N+1 this item exists to retire.
    if (!detail) return;
    loaded = false;
    (async () => {
      // The rating is two hops and the second only happens when the first found
      // something: a rating belongs to a **review note**, never to a book and
      // never to a reading, so a read with no review has no rating to ask for.
      const [hs, ns, review] = await Promise.all([
        api.highlightsForReading(id),
        api.notesForReading(id),
        api.noteForReading(id, 'review'),
      ]);
      if (reading.id !== id) return;
      marks = hs;
      notes = ns;
      rating = review === null ? null : await api.reviewRating(review.id);
    })()
      // A card that could not load its parts still shows the read it is of. The
      // cover and the dates came in as props and are not this call's to lose.
      .catch(() => {})
      .finally(() => (loaded = true));
  });
</script>

<article class="card">
  <header>
    <div class="art">
      <Jacket src={cover} accent={book.cover_accent} />
    </div>
    <div class="who">
      <!-- The book's title, linked: nothing is a dead end, and the book is where
           every one of these parts can be edited. -->
      <a class="title" href={`/book/${book.id}`}>{titleLabel(book.title)}</a>
      <!-- Which read, and then when. `null` for a book read once, so the
           ordinary card is not captioned *your first read* — `ReadCount::ordinal`
           is "a lone read has no number" and this is that, worded. -->
      {#if ordinal}
        <p class="ordinal">{ordinal}</p>
      {/if}
      {#if when}
        <p class="when">{when}</p>
      {/if}
      <p class="how">
        {#if stateWord}<span class="state">{stateWord}</span>{/if}
        {#if far}<span class="far">{far}</span>{/if}
      </p>
      {#if rating}
        <!-- The scale travels with the value, so `4.5 / 5` is readable without
             a second call. A read with no review has no rating and no gap where
             one would be — an unrated read is not an unfinished one.

             **Labelled**, because unlabelled it was not readable at all: a bare
             `3 / 5` sat directly under the state line, in the same accent the
             progress readout uses, one row above `p. 150 of 300 · 50%`. It read
             as progress before it read as a rating. -->
        <p class="rating">Rated {ratingLabel(rating)}</p>
      {/if}
    </div>
  </header>

  {#if passage}
    <!-- One passage, the engine's choice. `blockquote` because it is the book
         talking and not this app. -->
    <blockquote>
      <p>{passage.text}</p>
      {#if passage.chapter || passage.page !== null}
        <footer>
          {[passage.chapter, passage.page === null ? null : `p. ${passage.page}`]
            .filter(Boolean)
            .join(' · ')}
        </footer>
      {/if}
    </blockquote>
  {:else if detail}
    <!-- Absence, drawn as absence — and **not gated on a load**, since the
         passage arrives as a prop and its absence is known at first paint. A
         read whose marks the dates could not place has no passage of its own,
         which is ordinary; the move that fills it is the reader's, on the book.

         Gated on `detail`, though: see the note at the top of this file. On a
         wall this line is under most cards on the page, and a sentence the page
         repeats twenty-four times is not telling anybody anything. -->
    <p class="hint no-passage">No passage from this read.</p>
  {/if}

  {#if detail && loaded}
    <section class="left">
      <!--
        **"What you left behind", not "What you left."**

        The axiom is *"the app tells you what you did; it never tells you what
        you have left"* — and the short heading was the second half of that
        sentence, word for word, over a band that then printed a count. It means
        what was left *behind* and English does not disambiguate it at a glance,
        so the phrase is finished rather than trimmed.
      -->
      <h3 class="band-title">What you left behind</h3>
      {#if notes.length > 0}
        <!-- One list, with the kind as a **left prefix** — the book view's own
             arrangement (the book page's note list), rather than a trailing chip that fired for
             reviews and not for reflections and looked arbitrary. Two screens
             listing the same notes had grown two systems. -->
        <ul>
          {#each notes as n (n.id)}
            <li>
              <span class="kind">{noteKindLabel(n.kind) ?? ''}</span>
              <a href={`/book/${book.id}?note=${n.id}`}>{n.title}</a>
            </li>
          {/each}
        </ul>
      {/if}
      {#if marks.length > 0}
        <!-- A count of your own marks. Past tense, on a page you chose to open,
             and about one read — the three things that make a number allowed. -->
        <p class="marks">{countLabel(marks.length, 'passage')} marked</p>
      {/if}
      {#if notes.length === 0 && marks.length === 0}
        <!-- Idle is not blank, and it does not apologise. **No "yet"**: that one
             word turns an absence into something outstanding, which is the same
             grammar as *pending* wearing a softer coat. The fact is identical
             without it. -->
        <p class="hint">Nothing written against this read — the book is where it goes.</p>
      {/if}
    </section>
  {/if}
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: var(--s-3);
    padding: var(--s-4);
    background: var(--bg-raised);
    border-radius: var(--radius);
    /* The same lift the jacket has, so a card sits on the page as an object
       rather than being printed on it. See the note at the top of this file for
       why this survived a pass that was removing exactly this kind of thing. */
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 10%, transparent),
      0 1px 2px rgb(0 0 0 / 0.2),
      0 6px 14px -8px rgb(0 0 0 / 0.45);
    min-width: 0;
  }
  header {
    display: grid;
    grid-template-columns: 48px minmax(0, 1fr);
    gap: var(--s-3);
    align-items: start;
  }
  .art {
    aspect-ratio: 2 / 3;
    border-radius: var(--radius);
    overflow: hidden;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent);
  }
  .who {
    min-width: 0;
  }
  .title {
    font-weight: 600;
    line-height: 1.3;
    overflow-wrap: anywhere;
  }
  .title:hover {
    color: var(--accent-text);
  }
  /* Above the dates and quieter than the title: it tells you which of two
     otherwise-identical cards you are looking at, which is a caption's job. */
  .ordinal {
    margin: var(--s-1) 0 0;
    font-size: var(--t-micro);
    color: var(--accent-text);
  }
  .when {
    margin: var(--s-1) 0 0;
    font-size: var(--t-fine);
    color: var(--ink-dim);
  }
  .how {
    display: flex;
    gap: var(--s-3);
    flex-wrap: wrap;
    margin: var(--s-1) 0 0;
    font-size: var(--t-micro);
  }
  /* **Dim, not accent.** `Read` and `Put down` are past tense and there is
     nothing to press: the app's rule is that the accent marks state that is true
     right now *and that you can act on*. On the wall this word was the second
     brightest thing on every one of twenty-four cards, which made a page of
     finished reads look like a page of things wanting attention. */
  .state {
    color: var(--ink-dim);
  }
  .far {
    color: var(--ink-dim);
  }
  .rating {
    margin: var(--s-2) 0 0;
    font-size: var(--t-fine);
    color: var(--accent-text);
  }

  blockquote {
    margin: 0;
    padding-left: var(--s-3);
    border-left: 2px solid var(--line);
  }
  blockquote p {
    margin: 0;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  blockquote footer {
    margin-top: 0.35rem;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }

  .left ul {
    list-style: none;
    padding: 0;
    margin: 0.35rem 0 0;
    font-size: var(--t-fine);
  }
  .left li {
    /*
     * Roomier than it was, and the reason is a collision rather than taste. The
     * kind is a fixed gutter, so an **untyped** note's title begins at exactly
     * the x a typed note's title wraps to — and the fixture has one of each,
     * one under the other, rendering as a single note with a wrapped line. The
     * gutter stays (it is the note list's arrangement and labelling every plain
     * note would be a column of the same word); what changes is that a new row
     * is now visibly further down than a continuation of the one above.
     */
    padding: 0.35rem 0;
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
  }
  .left li a {
    line-height: 1.3;
  }
  .left a:hover {
    color: var(--accent-text);
  }
  /* A gutter, so the titles align whether or not their row has a kind — the
     book view's arrangement, where the prefix is a column and not a tag. */
  .kind {
    flex: 0 0 4rem;
    text-align: right;
  }
  .kind,
  .marks {
    color: var(--ink-dim);
    font-size: var(--t-micro);
  }
  .marks {
    margin: 0.45rem 0 0;
  }
  /* `.hint` is the shared token; only the tight spacing inside a card is here. */
  .card :global(.hint) {
    margin-bottom: 0;
  }
  /* Indented to where a passage's text sits, past the rule it does not get.
     Unindented it was flush with the card's padding, so the cards with *no*
     passage were also the ones whose text started furthest left — the absence
     read as unstyled rather than as composed. It gets no rule of its own,
     because a rule is what says *the book is talking*. */
  .card :global(.no-passage) {
    padding-left: calc(var(--s-3) + 2px);
  }
</style>
