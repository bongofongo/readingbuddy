<script lang="ts">
  /**
   * The wall — every card in the library, a page at a time (item 47).
   *
   * *"A finished wall that grows, and a year filtered out of it"*
   * (`gui-vision.md:151`). It is the reading-life half of the card: **my reading
   * life**, where `/book/[id]/cards` answers **this book's reads**. Neither is a
   * view of the other, which is the user's ruling — a book's cards reached from
   * the book are not a filtered wall in the reader's head, whatever they are in
   * the query.
   *
   * ## Two requests a page, and why that is the whole item
   *
   * `listReadingRows` carries the book, the reading, the read number and the
   * passage on every row (item 43), and `countReadings` is asked once per filter
   * beside it (item 18). So a page of twenty-four cards is **two requests**, and
   * a page of four hundred would be two as well — the cost is a property of the
   * row rather than of the page size, which is what "bounded" has to mean before
   * it stops being the argument every N+1 makes.
   *
   * A third request, `activityByMonth`, runs once to find the years. It is a
   * proxy and `WallControls` says so.
   *
   * ## The cards here carry no rating and no note list
   *
   * Not an oversight and not a shortfall: item 43 refused to put them on the row
   * *by name* — "a card would grow the rating; this row will not" — so a wall
   * that showed them would be four requests per card across a paged list, which
   * is the pathology item 44 wrote down a whole item in advance. The whole card
   * is one click away, on the book that minted it. `Card.svelte`'s `detail` prop
   * is where that line is drawn.
   *
   * ## Counts, on purpose
   *
   * *"No aggregate number on a home surface"* — and this is not one. It is a page
   * you chose to open, like `/life`, and the number is a count of readings you
   * have **had**: past tense, matched by a filter you set. It is phrased as a
   * total and never as a portion of one, because *showing 24 of 400* is a
   * progress bar through your own library.
   */
  import type { ReadingSortDto } from '$lib/api/bindings';
  import { client, type ReadingRow } from '$lib/api/client';
  import Card from '$lib/card/Card.svelte';
  import WallControls from '$lib/cards/WallControls.svelte';
  import { PAGE, offsetOf, pageCount, wallFilter } from '$lib/cards/wall';
  import { wholeLife, yearsOf } from '$lib/life/period';
  import { countLabel } from '$lib/phrasing';

  let rows = $state<ReadingRow[]>([]);
  let total = $state(0);
  let years = $state<number[]>([]);
  /** Three states, not a nullable: not asked, asked, answered (item 27's finding). */
  let loaded = $state(false);
  let failure = $state<string | null>(null);

  /** `null` is the whole library, which is what the wall opens on. */
  let year = $state<number | null>(null);
  let sort = $state<ReadingSortDto>('finished');
  let offset = $state(0);

  const pages = $derived(pageCount(total));
  const page = $derived(Math.floor(offset / PAGE));

  $effect(() => {
    // Read once, so the year span and anything else asking about "today" cannot
    // straddle midnight and disagree — `/life`'s rule, and its reason.
    const today = new Date();
    const life = wholeLife(today);
    client()
      .activityByMonth(life.from, life.to)
      .then((ms) => (years = yearsOf(ms).map((y) => y.year)))
      // The years are an ornament on the wall. A wall that loaded must not be
      // replaced by an error because the picker above it did not — the shelf's
      // ruling about its reading strip, one screen over.
      .catch(() => (years = []));
  });

  $effect(() => {
    // Named locally so the effect tracks exactly these three and the fetch below
    // reads the values it was scheduled for.
    const which = year;
    const order = sort;
    const from = offset;
    const filter = wallFilter(which, new Date());
    const api = client();
    loaded = false;
    // The **same filter object** goes to both. The engine builds the page's
    // clause and the count's from one predicate, so a disagreement between the
    // two numbers could only be this call site's.
    Promise.all([
      api.listReadingRows({ limit: PAGE, sort: order, offset: from, filter }),
      api.countReadings(filter),
    ])
      .then(([rs, n]) => {
        rows = rs;
        total = n;
      })
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)))
      .finally(() => (loaded = true));
  });

  function pickYear(next: number | null) {
    year = next;
    // Back to the first page. Staying on page four of a year that has three
    // cards asks for offset 72 and gets nothing, which reads as *this year is
    // empty* and is a lie about the filter rather than about the year.
    offset = 0;
  }

  function pickSort(next: ReadingSortDto) {
    sort = next;
    offset = 0;
  }

  function turn(to: number) {
    offset = offsetOf(to, total);
  }
</script>

<svelte:head><title>Cards — readingbuddy</title></svelte:head>

<a class="back" href="/">← Library</a>

<!-- "Cards", with no figure beside it. The count belongs to the filter and sits
     with the control that sets it, not in the heading. -->
<h1>Cards</h1>

{#if failure}
  <!-- A failure redirects: say what was refused and name the thing that works.
       No CLI command — this screen's audience is a reader with no terminal in
       the window, and the library's failure state is the one that may name one
       because its audience is whoever mis-set the data dir. -->
  <p class="note">These cards did not open: {failure}</p>
  <p class="hint">
    Every card is also on the book that minted it — the <a href="/">library</a> is the way there.
  </p>
{:else}
  <WallControls {years} {year} {sort} onyear={pickYear} onsort={pickSort} />

  {#if !loaded && rows.length === 0}
    <p class="hint">Reading the wall…</p>
  {:else if total === 0 && year !== null}
    <!--
      A year the picker offered and no card belongs to.

      **Not a failure and never styled as one**, and no *yet*: that one word
      turns an absence into something outstanding. A year you read in and closed
      nothing in is an ordinary year, and saying which years did close something
      is the move — the switch above is where it is taken.
    -->
    <p class="note">No cards from {year}.</p>
    <!-- The move is named in plain text and **nothing is bolded**: a review read
         an emphasised `All` as a link, and it is a pill directly above rather
         than something to click here. -->
    <p class="hint">
      A card is minted when a read ends, and none ended that year. Another year is above, and All is
      the whole wall.
    </p>
  {:else if total === 0}
    <!-- Idle is not blank. It says what a card is and where one comes from,
         and it names no command. -->
    <p class="note">No cards here.</p>
    <p class="hint">
      A card is one read of one book — its cover, its dates, and a passage you marked. Reading
      something in the <a href="/">library</a> is what mints one.
    </p>
  {:else}
    <div class="wall">
      {#each rows as row (row.reading.id)}
        <Card {row} />
      {/each}
    </div>

    <!-- Past tense, and a **total** rather than a portion of one. `showing 24 of
         400` is a progress bar through your own library; `400 cards` is a fact
         about what you read. -->
    <p class="tally">{countLabel(total, 'card')}</p>

    {#if pages > 1}
      <nav class="paging" aria-label="Pages">
        <button type="button" disabled={page === 0} onclick={() => turn(page - 1)}>‹ Back</button>
        <span>Page {page + 1} of {pages}</span>
        <button type="button" disabled={page >= pages - 1} onclick={() => turn(page + 1)}>
          More ›
        </button>
      </nav>
    {/if}
  {/if}
{/if}

<style>
  .back {
    color: var(--ink-dim);
    font-size: 0.85rem;
    display: inline-block;
    margin-bottom: 1.1rem;
  }
  h1 {
    font-size: 1.05rem;
    color: var(--ink-dim);
    font-weight: 500;
    margin-bottom: 1.4rem;
  }
  /*
   * A wall, not a track.
   *
   * `/book/[id]/cards` puts two cards beside each other and clamps the pair,
   * because one reading is its ordinary case and a lone card stretched across a
   * window is a banner. This page's ordinary case is dozens, so it fills the
   * width and the column floor is what decides how many fit. `auto-fill` rather
   * than `auto-fit`: a last row holding one card should leave the empty tracks
   * empty rather than stretching that card across them.
   */
  .wall {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(min(19rem, 100%), 1fr));
    gap: 1.2rem;
    /*
     * **Stretched, not `align-items: start`.** The passage is one to four lines
     * and half the cards on a real wall have none at all, so the height spread
     * is nearly 2× — and start-aligned that came out as bottoms missing by
     * 70–115px in alternating columns, which reads as broken masonry rather
     * than as a grid. Equal rows put the ragged edge *inside* a card, where it
     * is whitespace, instead of between cards, where it is a defect. The
     * per-book page keeps `start`: it holds one or two cards and has no rows.
     */
  }
  .tally {
    margin: 1.4rem 0 0;
    font-size: 0.85rem;
    color: var(--ink-dim);
  }
  .paging {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    margin-top: 0.5rem;
    font-size: 0.85rem;
    color: var(--ink-dim);
  }
  .paging button {
    font: inherit;
    padding: 0.25rem 0.7rem;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: transparent;
    color: var(--ink-dim);
    cursor: pointer;
  }
  .paging button:hover:not(:disabled) {
    color: var(--ink);
  }
  .paging button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
  /* The move out of an empty state, in the accent — it is the only link in the
     sentence and it has to look like one. */
  .hint a {
    color: var(--accent-text);
  }
</style>
