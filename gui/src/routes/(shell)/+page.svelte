<script lang="ts">
  /**
   * The library — the home surface, and the calmest one.
   *
   * Two bands and nothing else: the books you have open, previewed properly,
   * and the wall of everything you have read, grouped by the year the reading
   * closed.
   *
   * ## The two halves of the brief pull against each other, and this is the split
   *
   * *"Opening the app should be calming, with less happening and more
   * whitespace"* and *"the single book page is where the time goes"* are not the
   * same room, so they are not the same page. **The library got quieter and the
   * book page got much bigger.** The shelf is where you look; the book is where
   * you work. Nothing is added here to make this page useful, because being
   * useful is not its job.
   *
   * ## Why the wall is grouped by year, and why that is the default
   *
   * Grouping by **the year a reading closed** is the cheapest thing on this page
   * and does the most work. It is past tense, it contains no target, and
   * scrolling from this year down through the last is the *look back and feel you
   * did something* the brief asked for — delivered with no digits, no streak and
   * no badge. A count would have said the same thing worse. Time also needs zero
   * maintenance, can never be empty, needs no taxonomy decision and maps onto how
   * people actually remember: *when* is recalled far better than *which folder*.
   *
   * The key is **the reading's finish year**, never `BookSort::Year`, which is
   * the *publication* year and must not share the name.
   *
   * ## Three requests, and the one that is an N+1
   *
   * `listBooks` for the wall, `listReadingRows` (paged, closed readings only) for
   * the years, and `currentlyReading` for the band. The band then needs the
   * newest mark per open reading, and there is **no request for that** — so it is
   * `listHighlights` plus `listNotes` per open reading, which fetches every
   * highlight in a book to render one line. It works and it is wrong;
   * `$lib/library/latest.ts` records why it is left visible rather than hidden
   * behind a client-side aggregate.
   *
   * Each band's failure is its own. The library failing is **this page**
   * failing; the reading band failing is not, and must not replace the wall.
   */
  import type { ReadingFilterDto } from '$lib/api/bindings';
  import { client, type OpenReading, type StoredBook } from '$lib/api/client';
  import { latestMark, promoted, type Preview as PreviewOf } from '$lib/library/latest';
  import Preview from '$lib/library/Preview.svelte';
  import Moment from '$lib/moments/Moment.svelte';
  import {
    arrangementById,
    finishYears,
    recallArrangement,
    rememberArrangement,
    shelfGroups,
    type ArrangementId,
  } from '$lib/shelf/arrangements';
  import ShelfSwitch from '$lib/shelf/ShelfSwitch.svelte';
  import Wall from '$lib/shelf/Wall.svelte';

  /**
   * How much of the library the wall asks for.
   *
   * `listBooks` defaults to 200 and the wall wants the whole shelf, so the
   * number is stated rather than inherited. **It is still a limit**, and a
   * library past it is silently short — which is an engine item (item 18's other
   * half: a request that pages, or one that answers "all"), not something a
   * frontend fixes by asking for a bigger number every wave.
   */
  const SHELF_LIMIT = 2000;

  /** One page of closed readings. Big enough that most libraries take one trip. */
  const READINGS_PAGE = 500;

  /** Closed readings only: an open one has no `finished_at` and so is in no year. */
  const CLOSED: ReadingFilterDto = {
    book_id: null,
    status: null,
    open: false,
    finished_in: null,
  };

  let books = $state<StoredBook[] | null>(null);
  let years = $state<Map<number, number>>(new Map());
  let previews = $state<PreviewOf<OpenReading>[]>([]);
  let failure = $state<string | null>(null);

  let arrangement = $state<ArrangementId>(recallArrangement());
  const sort = $derived(arrangementById(arrangement).sort);

  const groups = $derived(books === null ? [] : shelfGroups(books, years, arrangement));
  const band = $derived(promoted(previews));

  function pick(id: ArrangementId) {
    arrangement = id;
    rememberArrangement(id);
  }

  // `$effect` rather than a `+page.ts` load: the data comes from an in-process
  // engine over Tauri's IPC, which does not exist during `vite build`, and a load
  // function is the one place SvelteKit might try to run it there.
  //
  // Reads `sort`, so switching arrangement re-asks the engine for the order.
  // **The ordering is never computed here** (item 17) — the switch names a
  // `BookSort` and the wall draws whatever comes back.
  $effect(() => {
    const which = sort;
    client()
      .listBooks(SHELF_LIMIT, which)
      .then((b) => (books = b))
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });

  // The years, fetched once: switching arrangement must not re-ask the library
  // what it finished. Its failure is not the wall's — a shelf with no year
  // groups is a lesser page, not a broken one, and every book still lands in a
  // group that is true.
  $effect(() => {
    loadYears().catch(() => (years = new Map()));
  });

  // The band, and its own failure. An ornament on the library: a library that
  // loaded must not be replaced by an error thrown by the thing above it.
  $effect(() => {
    loadBand().catch(() => (previews = []));
  });

  /**
   * Every closed reading, a page at a time.
   *
   * `countReadings` first, with the **same filter object** the rows are asked
   * with, so the total and the pages cannot be about different sets — the rule
   * `/cards` already follows. The loop is bounded by that count rather than by
   * "keep going until a short page", which would spin for ever on a request that
   * started answering with fewer rows than asked for.
   */
  async function loadYears(): Promise<void> {
    const api = client();
    const total = await api.countReadings(CLOSED);
    const rows = [];
    for (let offset = 0; offset < total; offset += READINGS_PAGE) {
      rows.push(
        ...(await api.listReadingRows({
          limit: READINGS_PAGE,
          offset,
          sort: 'finished',
          filter: CLOSED,
        })),
      );
    }
    years = finishYears(rows);
  }

  /**
   * The open readings, each with the newest thing written against it.
   *
   * Every open reading is asked about and then `promoted` keeps four — rather
   * than slicing first — because the *order* is by latest mark, and a slice
   * before the marks are known would promote by whatever order the engine
   * happened to return. That is the N+1 the module's header records: it is
   * bounded by `currentlyReading`'s own limit, not by the size of the library.
   */
  async function loadBand(): Promise<void> {
    const api = client();
    const open = await api.currentlyReading();
    previews = await Promise.all(
      open.map(async (r) => {
        const [hs, ns] = await Promise.all([
          api.listHighlights(r.book.id),
          api.listNotes(r.book.id),
        ]);
        return { reading: r, mark: latestMark(hs, ns), touched: r.reading.last_modified };
      }),
    );
  }
</script>

<svelte:head><title>readingbuddy</title></svelte:head>

<!--
  The moment, above both bands and above the failure below them.

  It is the app noticing something rather than the shelf reporting something, so
  it is not inside the `{#if}` that decides whether a library loaded: a moment is
  still true when the database will not open, and the two failures are unrelated.
  It renders nothing when there is nothing, which is most of the time.
-->
<Moment />

<!-- The page's name is in the wordmark and in the nav, where the shell says
     where you are — so this heading is for the document outline and for a
     screen reader, and takes no space on a surface whose whole brief is calm. -->
<h1 class="sr-only">Library</h1>

{#if failure}
  <!-- A failure redirects. The repo's refusal-with-a-next-move shape: `ko pull`
       names `--new`, `calibre status` reports absence and prescribes nothing. -->
  <p class="note">
    The library did not open: {failure}
  </p>
  <p class="hint">
    readingbuddy reads the library in <code>READINGBUDDY_DATA_DIR</code>, or the
    directory it was started in. <code>make dev-db</code> builds one to look at.
  </p>
{:else if books === null}
  <p class="hint">Reading the shelf…</p>
{:else if books.length === 0}
  <!-- Idle is not blank. An empty state names the moves that fill it and never
       apologises. These are the two importers that need no network. -->
  <p class="note">Nothing on the shelf.</p>
  <p class="hint">
    <code>rb epub &lt;file&gt;</code> adds a book from a file.
    <code>rb ko pull</code> takes what is on a connected reader.
  </p>
{:else}
  {#if band.length > 0}
    <!--
      No heading and no empty message when nothing is open: the band renders
      nothing at all rather than a row saying so. Which books these are is the
      engine's answer (`currently_reading`), and how many of them are shown is
      `promoted`'s — capped at four, cut in silence.
    -->
    <section class="band reading">
      <div class="band-head">
        <h2 class="band-title">Reading now</h2>
        <!--
          The door to reading mode (item 54), and it is in this band rather than
          in the header on purpose: it is only a place to go when something is
          open, and this band is the part of the app that already knows that —
          it renders nothing at all when nothing is. A permanent nav entry would
          be a link to an empty state most of the time.

          The arrow says it leaves, the same way `Cards →` does on the book's
          rail. No count beside it, here of all places.
        -->
        <a class="into" href="/reading">Reading mode →</a>
      </div>
      <div class="previews">
        {#each band as p (p.reading.reading.id)}
          <Preview book={p.reading.book} reading={p.reading.reading} mark={p.mark} />
        {/each}
      </div>
    </section>
  {/if}

  <section class="band">
    <div class="band-head">
      <h2 class="band-title">The shelf</h2>
      <ShelfSwitch current={arrangement} onpick={pick} />
    </div>
    <!-- No count. Not here and not in the header: this is the surface you land
         on, and a number on it is the completion framing `docs/decisions.md`
         bans. The switch says which arrangement is on and nothing about size. -->
    <Wall {groups} />
  </section>
{/if}

<style>
  .band + .band {
    margin-top: 3.4rem;
  }
  .band-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1.3rem;
  }
  .reading .band-head {
    margin-bottom: 1.3rem;
  }
  /* Dim at rest like every other secondary link in the app; the accent is spent
     on state you can act on, and *there is a place over there* is not that. */
  .into {
    font-size: 0.85rem;
    color: var(--ink-dim);
    white-space: nowrap;
  }
  .into:hover {
    color: var(--accent-text);
  }
  /*
   * A wrapping grid on the page's own ground — not the strip this replaces.
   *
   * That was a horizontally-scrolling band with a mask, scroll-snap and a raised
   * bleed to the window edge: three mechanisms holding up a row that usually
   * contains two items, with the rest of it hidden behind a gesture. A plain
   * grid is quieter and shows everything it has.
   *
   * `auto-fit` here and `auto-fill` on the wall, deliberately: the band is
   * capped at four, so a stretched track is a preview using the width it was
   * given, while an empty track would be a hole where a fifth book is not.
   */
  .previews {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(310px, 1fr));
    gap: 1.25rem 2rem;
  }
  @media (max-width: 680px) {
    .previews {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
