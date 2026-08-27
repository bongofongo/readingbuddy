<script lang="ts">
  /**
   * Reading mode — the book, and the four things you can do to it (item 54).
   *
   * ## What it is for
   *
   * Every other surface in this app is about a library. This one is about the
   * book in your hands: it is what is on screen while you are reading somewhere
   * else — on a device, on paper — and its whole job is to be worth leaving
   * open. So at rest it shows the jacket, the title, and where you are, and
   * nothing else at all.
   *
   * ## A place, not a mode, and the difference is load-bearing
   *
   * `docs/decisions.md`'s axiom says nothing is modal-by-default and nothing is
   * a dead end, and a full-window surface that covers the app is exactly what
   * that clause is usually about. This one is legitimate on four counts, and if
   * any of them stops being true the surface has become the thing the axiom
   * forbids:
   *
   * 1. **It has a URL.** `/reading?book=3` is reachable, linkable and survives a
   *    reload — the axiom's *state persists and is visible*.
   * 2. **The state is the engine's.** Which book you are reading is
   *    `currently_reading`, an open reading in the database, not a flag this
   *    route invented. Closing the window loses nothing.
   * 3. **Both exits are on screen the whole time**, at rest and with any panel
   *    open: this book's own page, and the library.
   * 4. **It carries no count and no target.** Not on a verb, not on the
   *    passages, not anywhere — see `$lib/reading/mode.ts`, where the verbs are
   *    asserted to have no digit in them.
   *
   * ## One panel at a time, which is the design rather than a simplification
   *
   * The four verbs — Note, Page, Passages, Books — open one panel each, and
   * opening one closes the last. That is the opposite of the book page, where
   * the rails are permanent *so that* nothing is modal, and the two are not in
   * tension: the book page is a desk you work at, and this is a surface you
   * glance at. A desk shows its instruments; a book does not.
   *
   * The tell that the rule is real: at rest the book is centred and large, and
   * with a panel open it becomes a compact row at the top. Nothing is hidden —
   * the identity of what you are reading is on screen in both states — but the
   * room goes to whichever of the two you asked for.
   *
   * ## Where the numbers come from
   *
   * `progressDetail` words a `ProgressDto` the engine computed, and the page
   * written by the box goes to `update_progress`, which answers with the book
   * **re-read**. So the percentage on screen after a write is the engine's
   * arithmetic over the page just stored and never this route's division — which
   * are different values for the two books in the dev library whose `page_count`
   * is zero or absent.
   */
  import { page as pageState } from '$app/state';
  import type { HighlightDto } from '$lib/api/bindings';
  import { client, type OpenReading } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { authorsLabel, progressDetail, titleLabel } from '$lib/phrasing';
  import NotePanel from '$lib/reading/NotePanel.svelte';
  import PagePanel from '$lib/reading/PagePanel.svelte';
  import PassagesPanel from '$lib/reading/PassagesPanel.svelte';
  import SwitchPanel from '$lib/reading/SwitchPanel.svelte';
  import Verbs from '$lib/reading/Verbs.svelte';
  import { chooseReading, type Panel, panelForKey, paramBook } from '$lib/reading/mode';

  let open = $state<OpenReading[]>([]);
  let loading = $state(true);
  let failure = $state<string | null>(null);
  let panel = $state<Panel>('none');
  let passages = $state<HighlightDto[]>([]);
  let passagesFailed = $state(false);
  /** What the last write stored, for the line that says it landed. */
  let stored = $state<string | null>(null);

  /**
   * The book, from the URL — **derived, not seeded**.
   *
   * The book page does the opposite with `?note=` and says why: deriving there
   * would re-open a note after every save. Here the query string *is* the
   * subject of the route rather than a thing that happened once on arrival, so
   * following it is correct — and it is what makes the Books panel a list of
   * ordinary links instead of a handler.
   */
  const wanted = $derived(paramBook(pageState.url.searchParams));
  const current = $derived(chooseReading(open, wanted));
  const book = $derived(current?.book ?? null);

  $effect(() => {
    void load();
  });

  async function load() {
    try {
      open = await client().currentlyReading();
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    } finally {
      loading = false;
    }
  }

  /**
   * Open a panel, or close whatever is up.
   *
   * The passages are refetched every time rather than cached, and that is the
   * behaviour the verb promises: *show me what has come off the device* is a
   * question about now, and a reader who has just synced in another window is
   * the exact person pressing it.
   */
  async function show(next: Panel) {
    panel = panel === next ? 'none' : next;
    if (panel !== 'passages' || current === null) return;
    passagesFailed = false;
    try {
      passages = await client().highlightsForReading(current.reading.id);
    } catch {
      // The passages failing is not this screen failing — the book is still on
      // it, and every other verb still works. Grouped by what a failure *means*,
      // the same way the book page groups its eight calls.
      passages = [];
      passagesFailed = true;
    }
  }

  /** The page landed; re-read the book so the percentage is the engine's. */
  async function turned(said: string) {
    stored = said;
    panel = 'none';
    await load();
  }

  /**
   * The keyboard, which is most of why this surface is worth leaving open.
   *
   * Two rejections before the map is consulted. A modified keystroke is the
   * platform's — `Cmd-P` prints — and a keystroke inside a field is text, or the
   * note box would be unable to contain the letter *n*. **Escape is exempt from
   * the second**, because a panel a reader cannot leave from the keyboard is the
   * trap this whole route claims not to be.
   */
  function onkeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    const target = e.target as HTMLElement | null;
    const typing = target?.matches('input, textarea, [contenteditable]') ?? false;
    if (typing && e.key !== 'Escape') return;
    const next = panelForKey(e.key);
    if (next === null) return;
    e.preventDefault();
    void show(next === 'none' ? 'none' : next);
  }
</script>

<svelte:window {onkeydown} />

<svelte:head>
  <title>{book ? titleLabel(book.title) : 'Reading'}</title>
</svelte:head>

<main class="reading" class:working={panel !== 'none'}>
  {#if loading}
    <p class="hint">Opening…</p>
  {:else if failure}
    <p class="refusal">The library did not open: {failure}</p>
    <p class="hint">
      <a href="/">The library</a> is the way back, and <code>rb open</code> reads the same database
      from the terminal.
    </p>
  {:else if current === null || book === null}
    <!-- Idle is not blank, and an empty state names the moves that fill it. No
         "yet": a shelf with nothing open is a fact about a library, not an
         omission somebody owes. -->
    <div class="empty">
      <h1>No book is open.</h1>
      <p class="hint">
        Reading mode follows the book you have open. Start one from the library and it is here.
      </p>
      <p><a class="way" href="/">The library</a></p>
    </div>
  {:else}
    <section class="book" aria-label="What you are reading">
      <a class="hero" href="/book/{book.id}" aria-label="Open {titleLabel(book.title)}">
        <Jacket src={client().heroSrc(book)} accent={book.cover_accent} />
      </a>
      <div class="identity">
        <h1>{titleLabel(book.title)}</h1>
        {#if authorsLabel(book.authors_display)}
          <p class="by">{authorsLabel(book.authors_display)}</p>
        {/if}
        <!-- The accent, spent the way the layout rework rules it may be spent:
             on state that is true right now and that you can act on. Where you
             are is the one such fact on this surface, and the Page verb is the
             act. -->
        {#if progressDetail(book.progress)}
          <p class="where">{progressDetail(book.progress)}</p>
        {/if}
        <!-- Past tense, about a thing you just did, and gone the moment you do
             anything else. -->
        {#if stored}
          <p class="said" aria-live="polite">{stored}</p>
        {/if}
      </div>
    </section>

    <!-- Wrapped so the route can place the row without reaching into the
         component's own styles: centred under the jacket at rest, and flush with
         the column's left edge once a panel is up, where the book row and the
         panel are both already on that edge. A row floating between two aligned
         blocks was the one thing the working-state screenshot showed. -->
    <div class="verbrow"><Verbs {panel} bookId={book.id} onshow={show} /></div>

    {#if panel !== 'none'}
      <section class="panel">
        {#if panel === 'page'}
          <PagePanel {book} onturned={turned} oncancel={() => show('none')} />
        {:else if panel === 'note'}
          <NotePanel {book} reading={current.reading} onwritten={() => show('none')} />
        {:else if panel === 'passages'}
          <PassagesPanel {passages} failed={passagesFailed} bookId={book.id} />
        {:else if panel === 'books'}
          <SwitchPanel {open} currentBookId={book.id} onpicked={() => (panel = 'none')} />
        {/if}
      </section>
    {/if}
  {/if}
</main>

<style>
  /*
   * The window, and it is the whole window on purpose: this route sits outside
   * `(shell)/`, so there is no header row above it and no `--shell` gutter
   * around it. `100dvh` rather than `100vh` because a webview's chrome moves.
   */
  .reading {
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
    align-items: center;
    /*
     * Centred at rest, top-aligned once a panel is up — **one property**, and
     * that is the repair rather than the design. The first cut pushed the book
     * down with `margin-top: auto` and left every other block to fight it with
     * an auto margin of its own; the screenshot showed the whole composition
     * pinned to the bottom edge under a field of nothing. Auto margins in a
     * flex column do not compose, and `justify-content` is the property that
     * was being approximated.
     */
    justify-content: center;
    padding: 2rem 1.5rem 2.5rem;
    gap: 1.5rem;
  }
  .reading.working {
    justify-content: flex-start;
  }

  /*
   * At rest the book takes the room; with a panel open it gives it up. One
   * class, and the transition is what makes the two states read as one surface
   * changing rather than two screens swapping.
   */
  .book {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 1.1rem;
    transition:
      gap 160ms ease,
      margin 160ms ease;
  }
  .hero {
    display: block;
    width: 190px;
    aspect-ratio: 2 / 3;
    border-radius: var(--radius);
    overflow: hidden;
    box-shadow: 0 10px 34px rgb(0 0 0 / 45%);
    transition:
      width 160ms ease,
      box-shadow 160ms ease;
  }
  .working .book {
    flex-direction: row;
    text-align: left;
    align-items: center;
    gap: 1rem;
    width: 100%;
    max-width: var(--column);
  }
  .working .hero {
    width: 46px;
    flex: none;
    box-shadow: 0 3px 12px rgb(0 0 0 / 35%);
  }
  .working h1 {
    font-size: 1.05rem;
  }

  h1 {
    font-size: 1.5rem;
    /* A 220-character title is in the fixture on purpose. Wrapping is fine;
       shoving the verbs off the bottom of the window is not. */
    max-width: var(--column);
  }
  .by {
    margin: 0;
    color: var(--ink-dim);
  }
  .where {
    margin: 0;
    color: var(--accent-text);
    font-variant-numeric: tabular-nums;
  }
  .said {
    margin: 0;
    color: var(--ink-dim);
    font-size: 0.85rem;
  }

  .verbrow {
    display: flex;
    justify-content: center;
  }
  .working .verbrow {
    width: 100%;
    max-width: var(--column);
    justify-content: flex-start;
  }

  .panel {
    width: 100%;
    max-width: var(--column);
    border-top: 1px solid var(--line);
    padding-top: 1.25rem;
  }

  .empty {
    text-align: center;
    max-width: var(--column);
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
  }
  .empty .way {
    color: var(--accent-text);
    border-bottom: 1px solid var(--accent);
    padding-bottom: 1px;
  }

  .refusal {
    margin: 0;
    max-width: var(--column);
  }
  .hint {
    color: var(--ink-dim);
    font-size: 0.9rem;
    max-width: var(--column);
    margin: 0;
  }
  .hint a {
    color: var(--accent-text);
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.9em;
  }

  /* The reader who opened this in a narrow pane beside a terminal, which is the
     window this whole app is designed to sit next to. */
  @media (max-width: 620px) {
    .reading {
      padding: 1.25rem 1rem 1.5rem;
    }
    .hero {
      width: 140px;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .book,
    .hero {
      transition: none;
    }
  }
</style>
