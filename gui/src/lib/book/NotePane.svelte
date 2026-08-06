<script lang="ts">
  /**
   * The notes band: the list, one note open, or that note's links — **one pane,
   * three depths**, each replacing the last in place.
   *
   * No dialog and no overlay anywhere in it. That is the axiom's *nothing is
   * modal-by-default*, and it is the TUI's own arrangement carried across: its
   * links pane swaps out the note list rather than opening a layer over it.
   * Every depth shows the way back, so nothing here is a dead end.
   *
   * The four kinds live in **one list** rather than four tabs, which is also the
   * TUI's ruling and worth restating: a section is a *collection* of things, and
   * a tab holding the single reflection is the wrong shape for a thing there is
   * exactly one of. The row says which kind it is instead.
   *
   * ## Where the note search is going to go
   *
   * Nowhere, yet, and deliberately not faked. `SearchMarks` takes no `book_id`
   * and its `limit` applies to the **global** ranked list, so filtering its
   * results down to this book above the seam returns nothing whenever the top
   * hits are in other books — a search box that looks like it works and is
   * silently wrong. That is item 40, and it is being built as an engine change
   * rather than papered over here. It belongs beside the band heading below.
   */
  import type { NoteDto, RatingDto, RatingScaleDto } from '$lib/api/bindings';
  import { client } from '$lib/api/client';
  import { noteAnchorLabel, noteKindLabel, ratingLabel, trimNumber } from '$lib/phrasing';
  import LinksPane from './LinksPane.svelte';
  import { linkPane, type LinkPane } from './links';
  import { ratingSteps } from './rating';

  let {
    bookId,
    notes,
    open,
    onopen,
    onreload,
  }: {
    bookId: number;
    notes: NoteDto[];
    /** The note the *page* considers open — the passages cite into this one. */
    open: NoteDto | null;
    onopen: (id: number | null) => void;
    onreload: () => Promise<void>;
  } = $props();

  let body = $state('');
  let savedBody = $state('');
  let showLinks = $state(false);
  let pane = $state<LinkPane | null>(null);
  let composing = $state(false);
  let draftTitle = $state('');
  let draftBody = $state('');
  let rating = $state<RatingDto | null>(null);
  let scale = $state<RatingScaleDto | null>(null);
  let failure = $state<string | null>(null);

  const dirty = $derived(body !== savedBody);
  const steps = $derived(ratingSteps(scale));

  function failed(e: unknown) {
    failure = e instanceof Error ? e.message : String(e);
  }

  /**
   * Which note's body is in the box.
   *
   * A plain `let` and **not** `$state`, on purpose: it is a guard against
   * reloading, so an effect that read it as a dependency would re-run itself
   * every time it wrote it. Nothing renders it.
   */
  let loadedFor: number | null = null;

  $effect(() => {
    const note = open;
    const id = note?.id ?? null;
    if (id === loadedFor) return;
    loadedFor = id;
    showLinks = false;
    failure = null;
    if (note === null) {
      body = savedBody = '';
      return;
    }
    client()
      .noteBody(note.id)
      .then((text) => {
        // Only if it is still the note being asked about — a fast second click
        // must not drop an older body into a newer note's box.
        if (loadedFor !== note.id) return;
        body = savedBody = text;
      })
      .catch(failed);
  });

  $effect(() => {
    if (!showLinks || open === null) return;
    const id = open.id;
    const api = client();
    Promise.all([api.outgoingLinks(id), api.backlinks(id)])
      .then(([out, back]) => {
        pane = linkPane(out, back);
      })
      .catch(failed);
  });

  $effect(() => {
    const note = open;
    // A rating belongs to a **review**, never to a book and never to a note of
    // another kind. Asking for one anywhere else would be this screen inventing
    // a place for it that the schema does not have.
    if (note === null || note.kind !== 'review') {
      rating = null;
      return;
    }
    const api = client();
    Promise.all([api.reviewRating(note.id), api.activeRatingScale()])
      .then(([r, s]) => {
        rating = r;
        scale = s;
      })
      .catch(failed);
  });

  async function save() {
    if (open === null) return;
    const text = body;
    try {
      // Rewrites the file **and** reindexes the wikilink edges, which is why the
      // links pane below can be trusted right after an edit.
      await client().updateNoteBody(open.id, text);
      savedBody = text;
      pane = null;
      await onreload();
    } catch (e) {
      failed(e);
    }
  }

  async function remove() {
    if (open === null) return;
    try {
      await client().deleteNote(open.id);
      onopen(null);
      await onreload();
    } catch (e) {
      failed(e);
    }
  }

  async function write() {
    try {
      const created = await client().createNote({
        book_id: bookId,
        reading_id: null,
        highlight_id: null,
        page: null,
        location: null,
        kind: 'note',
        // Empty means the engine takes the body's first six words, which is what
        // the field's own placeholder promises. Not the empty string: that is a
        // title, and it is blank.
        title: draftTitle.trim() === '' ? null : draftTitle.trim(),
        body: draftBody,
      });
      composing = false;
      draftTitle = draftBody = '';
      await onreload();
      onopen(created.id);
    } catch (e) {
      failed(e);
    }
  }

  /** Open **or mint** — one call, and the engine decides which reading it hangs off. */
  async function anchored(kind: 'reflection' | 'review') {
    try {
      const api = client();
      const created =
        kind === 'reflection' ? await api.openReflection(bookId) : await api.openReview(bookId);
      await onreload();
      onopen(created.id);
    } catch (e) {
      failed(e);
    }
  }

  async function rate(value: number) {
    if (open === null) return;
    try {
      await client().setRating(open.id, value);
      rating = await client().reviewRating(open.id);
    } catch (e) {
      failed(e);
    }
  }

  async function unrate() {
    if (open === null) return;
    try {
      await client().clearReviewRating(open.id);
      rating = null;
    } catch (e) {
      failed(e);
    }
  }
</script>

<section class="band">
  <div class="band-head">
    <h2 class="band-title">Notes</h2>
    <!-- Item 40's search box goes here: one input, `SearchMarks` narrowed to
         this book by the engine. Left empty rather than stubbed — an input that
         looks like a search and is not one is worse than no input. -->
    {#if open === null && !composing}
      <div class="acts">
        <button type="button" onclick={() => (composing = true)}>Write a note</button>
        <button type="button" onclick={() => anchored('reflection')}>Reflection</button>
        <button type="button" onclick={() => anchored('review')}>Review</button>
      </div>
    {/if}
  </div>

  {#if failure}
    <!-- A failure says what was refused and names the thing that still works.
         The vault is markdown on disk either way, which is the whole point of
         it being markdown on disk. -->
    <p class="note">That did not go through: {failure}</p>
    <p class="hint">
      Your notes are plain markdown in the vault regardless — <code>rb notes</code> lists them and
      <code>rb note</code> writes one.
    </p>
  {/if}

  {#if open !== null}
    <div class="pane">
      <div class="pane-head">
        {#if showLinks}
          <button type="button" class="back" onclick={() => (showLinks = false)}>‹ Note</button>
        {:else}
          <button type="button" class="back" onclick={() => onopen(null)}>‹ Notes</button>
        {/if}
        <h3>{open.title}</h3>
        {#if noteKindLabel(open.kind)}
          <span class="kind">{noteKindLabel(open.kind)}</span>
        {/if}
      </div>

      {#if showLinks && pane}
        <LinksPane {pane} onopen={(n) => onopen(n.id)} />
      {:else if showLinks}
        <p class="hint">Reading the graph…</p>
      {:else}
        <!-- The markdown, as markdown. Not a rich-text editor: the file in the
             vault is the origin and Obsidian is the other thing editing it, so
             a WYSIWYG surface here would be a second opinion about the bytes. -->
        <textarea
          bind:value={body}
          rows="10"
          spellcheck="true"
          aria-label="Note body"
          placeholder={'Write. A [[wikilink]] makes an edge — even to a note that does not exist yet.'}
        ></textarea>
        <div class="acts">
          <button type="button" class="primary" disabled={!dirty} onclick={save}>
            {dirty ? 'Save' : 'Saved'}
          </button>
          <button type="button" onclick={() => (showLinks = true)}>Links</button>
          <button type="button" onclick={remove}>Delete</button>
        </div>

        {#if open.kind === 'review'}
          <!-- A review is the one kind that carries a rating: it is written for
               other people, and the reflection deliberately never gets one. -->
          <div class="rating">
            <span class="rating-label">Rating</span>
            {#if steps.length > 0}
              {#each steps as v (v)}
                <button
                  type="button"
                  class="point"
                  class:on={rating !== null && rating.value === v}
                  aria-pressed={rating !== null && rating.value === v}
                  onclick={() => rate(v)}
                >
                  {trimNumber(v)}
                </button>
              {/each}
              {#if rating !== null}
                <button type="button" class="clear" onclick={unrate}>Clear</button>
              {/if}
            {:else if rating !== null}
              <!-- A scale nothing can draw as a row still has a value to report.
                   Saying so beats offering four hundred boxes. -->
              <span class="recorded">{ratingLabel(rating)}</span>
            {:else}
              <span class="hint">
                No scale this app can offer as a row — <code>rb rating scale</code> defines one.
              </span>
            {/if}
          </div>
        {/if}
      {/if}
    </div>
  {:else if composing}
    <div class="pane">
      <div class="pane-head">
        <button type="button" class="back" onclick={() => (composing = false)}>‹ Notes</button>
        <h3>A new note</h3>
      </div>
      <input
        bind:value={draftTitle}
        aria-label="Note title"
        placeholder="Title — or leave it, and the first few words become one"
      />
      <textarea bind:value={draftBody} rows="6" aria-label="Note body" placeholder="Write."
      ></textarea>
      <div class="acts">
        <button type="button" class="primary" onclick={write}>Write it</button>
      </div>
    </div>
  {:else if notes.length === 0}
    <!-- Idle is not blank, and this is not an apology or a count of zero. It
         names the three moves that fill it. -->
    <p class="note">Nothing written against this book yet.</p>
    <p class="hint">
      A note is markdown in your vault. A <strong>reflection</strong> is private and grows as you
      read; a <strong>review</strong> is the one written for other people, and the only one that
      carries a rating.
    </p>
  {:else}
    <ul class="notes">
      {#each notes as n (n.id)}
        <li>
          <button type="button" class="row" onclick={() => onopen(n.id)}>
            {#if noteKindLabel(n.kind)}
              <span class="kind">{noteKindLabel(n.kind)}</span>
            {/if}
            {#if noteAnchorLabel(n)}
              <span class="anchor">{noteAnchorLabel(n)}</span>
            {/if}
            <span class="title">{n.title}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  /* The band's own spacing, so a page composing several does not reach into
     them — and so no screen's rule leaks onto another's. */
  section.band {
    margin-top: 2.2rem;
  }
  .band-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 0.9rem;
  }
  .acts {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  button {
    font: inherit;
    font-size: 0.78rem;
    line-height: 1.4;
    color: var(--ink-dim);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.2rem 0.65rem;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    color: var(--ink);
  }
  button:disabled {
    cursor: default;
    opacity: 0.55;
  }
  button.primary:not(:disabled) {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
    font-weight: 600;
  }

  .pane {
    max-width: var(--measure);
  }
  .pane-head {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    margin-bottom: 0.7rem;
    flex-wrap: wrap;
  }
  .back {
    background: none;
    border: 0;
    padding: 0;
    flex: none;
  }
  h3 {
    font-size: 1rem;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .kind,
  .anchor {
    font-size: 0.72rem;
    color: var(--ink-dim);
    flex: none;
  }

  textarea,
  input {
    font: inherit;
    font-size: 0.92rem;
    width: 100%;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.6rem 0.7rem;
    margin-bottom: 0.6rem;
    resize: vertical;
  }
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    line-height: 1.6;
  }

  .rating {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
    margin-top: 1.1rem;
  }
  .rating-label {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-dim);
    margin-right: 0.3rem;
  }
  .point {
    min-width: 2.2rem;
    text-align: center;
  }
  /* The selected point is the one that has to read best — the same correction
     `ShelfSwitch` records, where a white label on brass measured 2.95:1 while
     its unselected sibling measured 5.61. */
  .point.on {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
    font-weight: 600;
  }
  .recorded {
    font-size: 0.9rem;
  }

  ul.notes {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--measure);
  }
  ul.notes li {
    border-bottom: 1px solid var(--line);
  }
  ul.notes li:last-child {
    border-bottom: 0;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    width: 100%;
    text-align: start;
    font-size: 0.9rem;
    background: none;
    border: 0;
    border-radius: 0;
    padding: 0.45rem 0.1rem;
    color: inherit;
  }
  .row:hover .title {
    color: var(--accent-text);
  }
  .row .title {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
</style>
