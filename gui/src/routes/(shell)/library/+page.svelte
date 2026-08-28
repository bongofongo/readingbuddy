<script lang="ts">
  /**
   * The library — the whole collection, as a wall of jackets.
   *
   * ## Why this is not the entrance any more
   *
   * It was, and the "Reading now" band sat on top of it. The two do different
   * jobs and only one of them is what you open the app for: *what am I reading*
   * is a question with two or three answers and you ask it every day, and *what
   * have I read* is a question with hundreds and you ask it when you are
   * browsing. Sharing a page meant the first was always a strip above the
   * second, and the second always had a preamble.
   *
   * So the band became `/` and this became a place you go. What that buys the
   * wall is the whole page: the arrangement switch is at the top of its own
   * surface, and the year groups start at the first scroll rather than below a
   * band.
   *
   * ## What did not change, and must not
   *
   * The grouping is still **the year a reading closed**, never `BookSort::Year`,
   * which is the *publication* year and must not share the name. There is still
   * no count — not per group, not in the header, not in the nav. This is the
   * surface a number would do the most damage on: a wall of everything you have
   * read is a record, and a record with a total is a target.
   *
   * ## Two requests, and each failure is its own
   *
   * `listBooks` for the wall and `listReadingRows` (paged, closed readings only)
   * for the years. The library failing is **this page** failing; the years
   * failing is not, and a wall with no year groups is a lesser page rather than
   * a broken one — every book still lands in a group that is true.
   */
  import type { ReadingFilterDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
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
  let failure = $state<string | null>(null);

  let arrangement = $state<ArrangementId>(recallArrangement());
  const sort = $derived(arrangementById(arrangement).sort);

  const groups = $derived(books === null ? [] : shelfGroups(books, years, arrangement));

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
  // what it finished.
  $effect(() => {
    loadYears().catch(() => (years = new Map()));
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
</script>

<svelte:head><title>Library — readingbuddy</title></svelte:head>

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
  <section class="band">
    <div class="band-head">
      <h2 class="band-title">The shelf</h2>
      <ShelfSwitch current={arrangement} onpick={pick} />
    </div>
    <!-- No count. Not here and not in the header: this is a record of what you
         have read, and a number on it is the completion framing
         `docs/decisions.md` bans. The switch says which arrangement is on and
         nothing about size. -->
    <Wall {groups} />
  </section>
{/if}

<style>
  .band-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1.3rem;
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
