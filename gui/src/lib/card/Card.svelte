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
   * ## What it carries, and one thing it deliberately does not
   *
   * The cover, the dates, the rating if a review was written, one passage, and
   * what you left behind. What it does **not** carry is *which* read it is —
   * no "Read #2" anywhere. `ReadNumbering` is the engine's since item 17c
   * precisely because it depends on `list_readings`' oldest-first ordering,
   * which is stated nowhere on the wire; `readings.indexOf(id) + 1` here would
   * re-implement a domain rule *and* silently re-acquire that dependency, with
   * nothing on the screen looking wrong. Item 41 is the request that would fill
   * it. The dates say which read this is in the meantime, and they say it in the
   * terms the reader actually remembers.
   *
   * ## The passage is asked for, never picked
   *
   * `cardPassage` is one request (item 44) and the rule behind it — longest,
   * ties to the lowest id — is not restated here or anywhere in `gui/`.
   * `highlights[0]` is the thing this component exists not to do: it is a
   * frontend inventing a selection predicate, and the day the TUI grows a card
   * the two apps would show a different sentence for the same reading with
   * neither looking wrong.
   */
  import type { HighlightDto, NoteDto, RatingDto, ReadingDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import {
    countLabel,
    dayLabel,
    noteKindLabel,
    progressDetail,
    ratingLabel,
    readingSpan,
    readingStateLabel,
    titleLabel,
  } from '$lib/phrasing';

  let { book, reading }: { book: StoredBook; reading: ReadingDto } = $props();

  let passage = $state<HighlightDto | null>(null);
  let marks = $state<HighlightDto[]>([]);
  let notes = $state<NoteDto[]>([]);
  let rating = $state<RatingDto | null>(null);
  /** Not asked / asked / answered — three, not a nullable. See item 27's finding. */
  let loaded = $state(false);

  const cover = $derived(client().heroSrc(book));
  const when = $derived(readingSpan(reading) ?? dayLabel(reading.created_at));
  // `stateWord`, not `state`: a top-level `const state` in a rune file shadows
  // the `$state` rune for svelte-check, which reports it as sixteen errors on
  // the *other* lines — item 27 recorded this and it cost a session then too.
  const stateWord = $derived(readingStateLabel(reading.status));
  const far = $derived(progressDetail(reading.progress));
  const reflection = $derived(notes.find((n) => n.kind === 'reflection') ?? null);
  /** Everything except the reflection, which gets its own line above them. */
  const rest = $derived(notes.filter((n) => n.kind !== 'reflection'));

  $effect(() => {
    const id = reading.id;
    const api = client();
    loaded = false;
    (async () => {
      // The rating is two hops and the second only happens when the first found
      // something: a rating belongs to a **review note**, never to a book and
      // never to a reading, so a read with no review has no rating to ask for.
      const [p, hs, ns, review] = await Promise.all([
        api.cardPassage(id),
        api.highlightsForReading(id),
        api.notesForReading(id),
        api.noteForReading(id, 'review'),
      ]);
      if (reading.id !== id) return;
      passage = p;
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
             one would be — an unrated read is not an unfinished one. -->
        <p class="rating">{ratingLabel(rating)}</p>
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
  {:else if loaded}
    <!-- Absence, drawn as absence. A read whose marks the dates could not place
         has no passage of its own, which is ordinary — and the move that fills
         it is the reader's own, on the book. -->
    <p class="hint">No passage from this read.</p>
  {/if}

  {#if loaded}
    <section class="left">
      <h3 class="band-title">What you left</h3>
      {#if reflection}
        <p class="reflection">
          <a href={`/book/${book.id}?note=${reflection.id}`}>{reflection.title}</a>
        </p>
      {/if}
      {#if rest.length > 0}
        <ul>
          {#each rest as n (n.id)}
            <li>
              <a href={`/book/${book.id}?note=${n.id}`}>{n.title}</a>
              {#if noteKindLabel(n.kind)}<span class="kind">{noteKindLabel(n.kind)}</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
      {#if marks.length > 0}
        <!-- A count of your own marks. Past tense, on a page you chose to open,
             and about one read — the three things that make a number allowed. -->
        <p class="marks">{countLabel(marks.length, 'passage')} marked</p>
      {/if}
      {#if !reflection && rest.length === 0 && marks.length === 0}
        <!-- Idle is not blank, and it does not apologise. This names the move
             and does not frame the read as missing anything. -->
        <p class="hint">Nothing written against this read yet — the book is where it goes.</p>
      {/if}
    </section>
  {/if}
</article>

<style>
  .card {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1.1rem;
    background: var(--bg-raised);
    border-radius: var(--radius);
    /* The same lift the jacket has, so a card sits on the page as an object
       rather than being printed on it. */
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 10%, transparent),
      0 1px 2px rgb(0 0 0 / 0.2),
      0 6px 14px -8px rgb(0 0 0 / 0.45);
    min-width: 0;
  }
  header {
    display: grid;
    grid-template-columns: 84px minmax(0, 1fr);
    gap: 1rem;
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
  .when {
    margin: 0.25rem 0 0;
    font-size: 0.85rem;
    color: var(--ink-dim);
  }
  .how {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin: 0.3rem 0 0;
    font-size: 0.82rem;
  }
  .state {
    color: var(--accent-text);
  }
  .far {
    color: var(--ink-dim);
  }
  .rating {
    margin: 0.35rem 0 0;
    font-size: 0.9rem;
    color: var(--accent-text);
  }

  blockquote {
    margin: 0;
    padding-left: 0.9rem;
    border-left: 2px solid var(--line);
  }
  blockquote p {
    margin: 0;
    line-height: 1.5;
    overflow-wrap: anywhere;
  }
  blockquote footer {
    margin-top: 0.35rem;
    font-size: 0.78rem;
    color: var(--ink-dim);
  }

  .left ul {
    list-style: none;
    padding: 0;
    margin: 0.35rem 0 0;
    font-size: 0.85rem;
  }
  .left li {
    padding: 0.2rem 0;
    display: flex;
    gap: 0.5rem;
    align-items: baseline;
  }
  .left a:hover {
    color: var(--accent-text);
  }
  .reflection {
    margin: 0.35rem 0 0;
    font-size: 0.9rem;
  }
  .kind,
  .marks {
    color: var(--ink-dim);
    font-size: 0.78rem;
  }
  .marks {
    margin: 0.45rem 0 0;
  }
  /* `.hint` is the shared token; only the tight spacing inside a card is here. */
  .card :global(.hint) {
    margin-bottom: 0;
  }
</style>
