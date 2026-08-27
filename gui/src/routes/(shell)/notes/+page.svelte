<script lang="ts">
  /**
   * The vault, as a place.
   *
   * The engine has had `notes_fts` since migration `0001` and it has had no
   * interface outside the CLI. A full-text index over everything you have ever
   * thought does not become central by living inside one book's page — and the
   * brief makes exploring notes a central activity rather than a subsidiary one.
   *
   * **This is also the page that keeps the rest of the vault honest.**
   * Bidirectional linking sells relief from *"where am I going to put this?"*,
   * and that relief holds only while you believe you will retrieve the notes
   * later; the observed failure of the pattern is that the need to take notes
   * far outstrips the need to review them, and the vault becomes a garbage dump
   * full of crufty links. Retrieval is what this page is for, so it is not a
   * nice-to-have surface.
   *
   * ## The split is 26rem then the prose measure, and that is not the draft
   *
   * The redesign drafted results at up to 68ch with a 22rem detail pane. That is
   * backwards twice. A result row is a title, a kind and a two-line snippet —
   * **structured metadata, which wants a list with predictable element positions
   * and a stable left edge**, not a prose column. And 22rem minus padding is a
   * 38–42 character measure for the *note*, at or below the floor for
   * multi-column text, so the one thing on the page that is genuinely prose got
   * the width that cannot hold it.
   *
   * The `max-width` on the grid is load-bearing for a second reason: with a bare
   * `1fr` first column the two halves sit at opposite ends of a 1440px window
   * with dead air between them.
   *
   * ## Counts are allowed here
   *
   * `/notes` is a page you chose to open, which is the condition the axiom sets.
   * `3 out · 2 in` are edges that **exist**, stated in the past tense. Nothing on
   * this page counts something you have not written — and in particular the
   * search says *nothing matched* in words rather than reporting `0 results`.
   *
   * ## Deliberately absent: a dangling-links index
   *
   * "Notes waiting to be written" is an orphan queue with better manners, and
   * `docs/decisions.md` rules the orphan queue out by name. There is a second,
   * independent reason: unlinked-mention and dangling indexes are where link
   * inflation starts — more edges, less signal, and a graph that gets *less*
   * useful the more diligently the feature is used. A dangling target is visible
   * wherever it is linked *from*, which is where it means something.
   */
  import type { NoteDto, SearchHitDto, SearchSourceDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import { linkPane, type LinkPane } from '$lib/book/links';
  import LinksPane from '$lib/book/LinksPane.svelte';
  import { snippetSegments } from '$lib/book/snippet';
  import { noteKindLabel, titleLabel } from '$lib/phrasing';

  /**
   * A ceiling, not a page size: there is no cursor on `SearchMarks`, so *more*
   * is the same search with a bigger number. Low enough to stay a list and high
   * enough that the cut is out of reach of a real query.
   */
  const LIMIT = 40;
  /** What *Recently written* shows. The cut is the engine's, after its own sort. */
  const RECENT = 30;

  /**
   * The three scopes, and they are the engine's own two sources plus both.
   *
   * The draft asked for *All · Notes · Reflections · Reviews*. **Note kind is not
   * on this request**, so those three would have to be a client-side filter over
   * a ranked, limited list — which silently returns fewer rows than exist and
   * calls it an answer. `source` is what the engine actually narrows on, so the
   * chips narrow on that and the kind is shown on every row instead. Filtering by
   * kind is an engine item.
   */
  const SCOPES: { id: SearchSourceDto | null; label: string }[] = [
    { id: null, label: 'All' },
    { id: 'note', label: 'Notes' },
    { id: 'highlight', label: 'Passages' },
  ];

  let query = $state('');
  let scope = $state<SearchSourceDto | null>(null);
  let hits = $state<SearchHitDto[]>([]);
  let recent = $state<NoteDto[]>([]);
  let failure = $state<string | null>(null);
  /** The query the shown list answers, so *nothing matched* can quote it. */
  let answered = $state('');

  /** Which note the right column is showing, by id — resolved against the list. */
  let focusedId = $state<number | null>(null);
  let body = $state('');
  let pane = $state<LinkPane | null>(null);
  let ofBook = $state<StoredBook | null>(null);

  /**
   * Which query the list belongs to.
   *
   * A plain counter and not `$state`: it exists so a slow reply for `gr` cannot
   * land on top of the list for `grief`, and an effect reading it as a
   * dependency would re-run itself on every write.
   */
  let asked = 0;

  /** Every note on screen, whichever list drew it — the focus resolves in here. */
  const notesShown = $derived(
    query.trim() === '' ? recent : hits.flatMap((h) => (h.kind === 'note' ? [h.note] : [])),
  );
  const focused = $derived(notesShown.find((n) => n.id === focusedId) ?? null);

  /**
   * The first note on the list reads itself into the right column.
   *
   * A preview pane with nothing in it is half a page, and the first row is where
   * a reader's eye already is. It is a **view** and not a move: nothing is
   * written, nothing is marked as seen, and picking another row replaces it. It
   * also re-seats itself when a search replaces the list under it, which is what
   * stops the column emptying the moment you type.
   */
  $effect(() => {
    const shown = notesShown;
    if (shown.length === 0) return;
    if (shown.some((n) => n.id === focusedId)) return;
    focusedId = shown[0]!.id;
  });

  $effect(() => {
    client()
      .listNotes(null, RECENT)
      .then((ns) => (recent = ns))
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });

  $effect(() => {
    const q = query;
    const source = scope;
    const seq = ++asked;
    // The empty query is **not a search**: the engine issues no statement for it
    // and answers with nothing, so this needs no guard against blankness — but it
    // does need to stop drawing the last list, since an empty box with hits under
    // it is a screen answering a question nobody is asking.
    if (q.trim() === '') {
      hits = [];
      answered = '';
      failure = null;
      return;
    }
    const handle = setTimeout(() => {
      client()
        .searchMarks(q, null, LIMIT, source)
        .then((found) => {
          if (seq !== asked) return;
          hits = found;
          answered = q;
          failure = null;
        })
        .catch((e) => {
          if (seq !== asked) return;
          hits = [];
          answered = q;
          failure = e instanceof Error ? e.message : String(e);
        });
      // One search per pause rather than one per keystroke.
    }, 140);
    return () => clearTimeout(handle);
  });

  // The focused note's body, its edges and the book it is filed under. Three
  // calls for one selection, made when the selection changes rather than per
  // row — a preview per result would be the whole vault on the wire to draw one.
  $effect(() => {
    const note = focused;
    if (note === null) {
      body = '';
      pane = null;
      ofBook = null;
      return;
    }
    const api = client();
    let live = true;
    api
      .noteBody(note.id)
      .then((t) => live && (body = t))
      .catch(() => live && (body = ''));
    Promise.all([api.outgoingLinks(note.id), api.backlinks(note.id)])
      .then(([out, back]) => live && (pane = linkPane(out, back)))
      .catch(() => live && (pane = null));
    // The book's name is not on `NoteDto` and not on a search hit — recorded as
    // a gap. One call for the note you are looking at is the honest shape of it;
    // a name on every row would be one call per row.
    if (note.book_id === null) ofBook = null;
    else
      api
        .getBook(note.book_id)
        .then((b) => live && (ofBook = b))
        .catch(() => live && (ofBook = null));
    return () => {
      live = false;
    };
  });

  /**
   * A note hit's title, unless the snippet already *is* the title.
   *
   * `notes_fts` indexes the title beside the body and `snippet(…, -1, …)` picks
   * whichever column matched, so a query matching a title comes back with the
   * title as its snippet — and printing the title above it draws the same words
   * twice, once marked and once not.
   */
  function whence(title: string, snippet: string): string | null {
    const flat = snippetSegments(snippet)
      .map((s) => s.text)
      .join('');
    return flat.trim() === title.trim() ? null : title;
  }
</script>

<svelte:head><title>Notes — readingbuddy</title></svelte:head>

<h1 class="sr-only">Notes</h1>

<div class="box">
  <label>
    <span class="sr-only">Search your notes and passages</span>
    <input
      bind:value={query}
      type="search"
      autocomplete="off"
      spellcheck="false"
      placeholder="A word you remember"
    />
  </label>
  <div class="scopes" role="group" aria-label="What to search">
    {#each SCOPES as s (s.label)}
      <button
        type="button"
        class:on={s.id === scope}
        aria-pressed={s.id === scope}
        onclick={() => (scope = s.id)}
      >
        {s.label}
      </button>
    {/each}
  </div>
  <p class="hint">Full text, over every note in the vault and the passages too.</p>
</div>

<div class="split">
  <div class="results">
    {#if failure}
      <p class="note">That search did not go through: {failure}</p>
      <p class="hint"><code>rb find</code> asks the same index from the terminal.</p>
    {:else if query.trim() === ''}
      <h2 class="band-title">Recently written</h2>
      {#if recent.length === 0}
        <!-- Idle is not blank: the two moves that fill a vault, and no apology
             about it being empty. -->
        <p class="note">Nothing in the vault.</p>
        <p class="hint">
          <code>rb note</code> writes one from the terminal, and a reflection opens from any book.
        </p>
      {:else}
        <ul>
          {#each recent as n (n.id)}
            <li>
              <button
                type="button"
                class:on={n.id === focusedId}
                aria-current={n.id === focusedId ? 'true' : undefined}
                onclick={() => (focusedId = n.id)}
              >
                <span class="row">
                  <span class="title">{n.title}</span>
                  {#if noteKindLabel(n.kind)}
                    <span class="kind">{noteKindLabel(n.kind)}</span>
                  {/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else if hits.length === 0}
      <!-- No count, and no apology. What was searched, and where else to ask. -->
      <p class="note">Nothing matches “{answered}”.</p>
      <p class="hint">
        The words searched are your notes and your passages — <code>rb find</code> asks the same index
        from the terminal.
      </p>
    {:else}
      <!-- The engine's order, drawn straight. No grouping by kind: the rank is
           the answer and two sections would be a second ranking. -->
      <ul>
        {#each hits as hit (hit.kind === 'note' ? `n${hit.note.id}` : `h${hit.highlight.id}`)}
          <li>
            {#if hit.kind === 'note'}
              <button
                type="button"
                class:on={hit.note.id === focusedId}
                aria-current={hit.note.id === focusedId ? 'true' : undefined}
                onclick={() => (focusedId = hit.note.id)}
              >
                {#if whence(hit.note.title, hit.snippet)}
                  <span class="row">
                    <span class="title">{hit.note.title}</span>
                    {#if noteKindLabel(hit.note.kind)}
                      <span class="kind">{noteKindLabel(hit.note.kind)}</span>
                    {/if}
                  </span>
                {/if}
                <span class="snippet">
                  {#each snippetSegments(hit.snippet) as seg, i (i)}{#if seg.match}<mark
                        >{seg.text}</mark
                      >{:else}{seg.text}{/if}{/each}
                </span>
              </button>
            {:else}
              <!-- A passage is not a note, so it does not select the pane beside
                   it — it is a link into the book that keeps it, which is where
                   its neighbours and its citations are. -->
              <a class="passage" href={`/book/${hit.highlight.book_id}`}>
                <span class="row">
                  <span class="kind">Passage</span>
                  {#if hit.highlight.page !== null}
                    <span class="kind">p. {hit.highlight.page}</span>
                  {/if}
                </span>
                <span class="snippet">
                  {#each snippetSegments(hit.snippet) as seg, i (i)}{#if seg.match}<mark
                        >{seg.text}</mark
                      >{:else}{seg.text}{/if}{/each}
                </span>
              </a>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>

  <div class="focus">
    {#if focused === null}
      <p class="hint">Pick a note to read it here.</p>
    {:else}
      <h2>{focused.title}</h2>
      <p class="whose">
        {#if ofBook}
          <a href={`/book/${ofBook.id}`}>{titleLabel(ofBook.title)}</a>
        {:else if focused.book_id === null}
          <!-- A note filed under no book is ordinary: `rb note` writes one, and
               the vault is not an index of the library. -->
          <span>Not filed under a book</span>
        {/if}
      </p>
      <!-- The body, faded under a mask rather than cut: the fade is a scent
           signal — *there is more here* — and it reads as a deliberate preview
           rather than as a truncation artefact now that the column is a real
           reading column. -->
      <div class="body">{body}</div>
      <div class="acts">
        {#if focused.book_id !== null}
          <a href={`/book/${focused.book_id}?note=${focused.id}`}>Open</a>
          <a href={`/book/${focused.book_id}`}>Go to the book</a>
        {/if}
      </div>
      {#if pane}
        <section class="links">
          <h3 class="band-title">Links</h3>
          <LinksPane
            {pane}
            onopen={(n) => (focusedId = n.id)}
          />
        </section>
      {/if}
    {/if}
  </div>
</div>

<style>
  .box {
    margin-bottom: 1.8rem;
  }
  input {
    font: inherit;
    font-size: 1rem;
    width: 100%;
    max-width: 46rem;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.55rem 0.8rem;
  }
  .scopes {
    display: inline-flex;
    gap: 1px;
    padding: 2px;
    margin: 0.6rem 0 0.5rem;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg-raised);
  }
  .scopes button {
    font: inherit;
    font-size: 0.76rem;
    color: var(--ink-dim);
    background: none;
    border: 0;
    border-radius: 2px;
    padding: 0.15rem 0.7rem;
    cursor: pointer;
  }
  .scopes button:hover {
    color: var(--ink);
  }
  /* The selected point is a surface, so it takes `--accent` with an
     `--accent-on` label — the pair `app.css` defines for exactly this. */
  .scopes button.on {
    color: var(--accent-on);
    background: var(--accent);
    font-weight: 600;
  }
  .hint {
    margin: 0;
  }

  /*
   * Results at a fixed, generous 26rem; the note at the prose measure.
   *
   * The cap on the whole grid is what stops the two halves ending up at opposite
   * ends of a wide window with dead air between them.
   */
  .split {
    display: grid;
    grid-template-columns: 26rem minmax(0, var(--column));
    gap: 0 3rem;
    align-items: start;
    max-width: calc(26rem + var(--column) + 3rem);
  }
  @media (max-width: 900px) {
    .split {
      grid-template-columns: minmax(0, 1fr);
      gap: 2rem 0;
    }
  }

  ul {
    list-style: none;
    padding: 0;
    margin: 0.7rem 0 0;
  }
  li {
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  .results button,
  .results .passage {
    display: block;
    width: 100%;
    text-align: start;
    font: inherit;
    font-size: 0.85rem;
    background: none;
    border: 0;
    /* Stated on both states so selecting a row does not shift its text. */
    border-left: 2px solid transparent;
    padding: 0.5rem 0.3rem 0.5rem 0.6rem;
    color: inherit;
    cursor: pointer;
  }
  .results button.on {
    background: var(--bg-raised);
    border-left-color: var(--accent);
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
  }
  .title {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .kind {
    font-size: 0.68rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-dim);
    flex: none;
    margin-left: auto;
  }
  .row .kind + .kind {
    margin-left: 0;
  }
  .snippet {
    display: block;
    font-size: 0.82rem;
    color: var(--ink-dim);
    overflow-wrap: anywhere;
    /* Two lines, so a long body cannot make one result the height of the list. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  /* The terms, marked. A tint rather than a fill: `mark`'s user-agent yellow is
     not in this palette and would be the loudest thing on the page. */
  mark {
    background: color-mix(in srgb, var(--accent) 30%, transparent);
    color: var(--ink);
    border-radius: 2px;
  }

  .focus {
    min-width: 0;
  }
  .focus h2 {
    font-size: 1.05rem;
    overflow-wrap: anywhere;
  }
  .whose {
    margin: 0.25rem 0 0.8rem;
    font-size: 0.8rem;
    color: var(--ink-dim);
  }
  .whose a:hover {
    color: var(--accent-text);
  }
  .body {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.9rem;
    line-height: 1.7;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    max-height: 15rem;
    overflow: hidden;
    -webkit-mask-image: linear-gradient(to bottom, #000 60%, transparent);
    mask-image: linear-gradient(to bottom, #000 60%, transparent);
  }
  .acts {
    display: flex;
    gap: 1rem;
    margin-top: 0.6rem;
    font-size: 0.82rem;
    color: var(--ink-dim);
  }
  .acts a:hover {
    color: var(--accent-text);
  }
  .links {
    margin-top: 1.6rem;
  }
  .links h3 {
    margin-bottom: 0.5rem;
  }
  .note {
    max-width: var(--column);
    margin: 0.7rem 0 0.5rem;
  }
</style>
