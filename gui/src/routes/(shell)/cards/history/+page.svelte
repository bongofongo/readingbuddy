<script lang="ts">
  /**
   * The wall — every card in the library, a page at a time (item 47), and since
   * the minimal pass it lives at `/cards/history` rather than at `/cards`.
   *
   * ## Why it moved, and what moved with it
   *
   * `/cards` used to be this page. It opened on two rows of pills — *Show*
   * with `All`, every year the library has and `Still reading`; *Order* with
   * three — and then twenty-four bordered cards, a total and a pager. Every one
   * of those controls is defensible and the page still had more interface than
   * question.
   *
   * The split is the deep-module argument applied to a screen: **quantity is
   * this page's purpose and it is not `/cards`'s.** `/cards` answers *what did I
   * last finish*, which needs no control at all; this answers *everything I have
   * read, arranged how I like*, which is what the pills are for. So the pills
   * came here, whole, and the door on `/cards` is the only way in.
   *
   * That door is also why this route takes **no nav entry**. A place reached
   * through the page that owns the question is a place the reader arrived at
   * deliberately, which is the same condition `/life` meets and the reason a
   * count is allowed on either.
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
   * A third request, `readingYears`, runs once per filter to find the years
   * (item 51). It used to be `activityByMonth`, which was a **proxy**: the
   * activity log is filled by `rb activity --refill` and by nothing
   * automatically, so a library that had never refilled offered no years at all
   * while plainly having finished books, and a year could be offered because a
   * note was written in it while no read ended. The years now come from
   * `readings.finished_at` under the wall's own filter, so the picker and the
   * wall agree by construction.
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
  import { PAGE, offsetOf, pageCount, wallFilter, type WallScope } from '$lib/cards/wall';
  import { countLabel } from '$lib/phrasing';

  let rows = $state<ReadingRow[]>([]);
  let total = $state(0);
  let years = $state<number[]>([]);
  /** Whether any reading has not ended — the *Still reading* chip, never a count. */
  let anyOpen = $state(false);
  /** Three states, not a nullable: not asked, asked, answered (item 27's finding). */
  let loaded = $state(false);
  let failure = $state<string | null>(null);

  /** The whole wall is what it opens on. */
  let scope = $state<WallScope>({ kind: 'all' });
  let sort = $state<ReadingSortDto>('finished');
  let offset = $state(0);

  const pages = $derived(pageCount(total));
  const page = $derived(Math.floor(offset / PAGE));
  /** The year in force, or `null` — narrowed here rather than in the markup. */
  const yearShown = $derived(scope.kind === 'year' ? scope.year : null);

  $effect(() => {
    client()
      // Asked over the **whole** wall rather than over the scope in force: the
      // picker's own job is to offer the other scopes, and asking it under the
      // current one would leave a reader who picked 2024 with only 2024 to pick
      // from — a control that removes its own alternatives.
      .readingYears(null)
      .then((y) => {
        years = y.years;
        anyOpen = y.open;
      })
      // The years are an ornament on the wall. A wall that loaded must not be
      // replaced by an error because the picker above it did not — the shelf's
      // ruling about its reading strip, one screen over.
      .catch(() => {
        years = [];
        anyOpen = false;
      });
  });

  $effect(() => {
    // Named locally so the effect tracks exactly these three and the fetch below
    // reads the values it was scheduled for.
    const which = scope;
    const order = sort;
    const from = offset;
    const filter = wallFilter(which);
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

  function pickScope(next: WallScope) {
    scope = next;
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

<svelte:head><title>Every card — readingbuddy</title></svelte:head>

<!-- Drawn, and `/cards` is the reason. The shell's nav says *Cards* and marks it
     current on both routes — `here('/cards')` matches this subtree — so without
     a heading the two pages are the same place with different contents, which is
     precisely the confusion a door is supposed to prevent. No figure beside it:
     the count belongs to the filter and sits with the control that sets it. -->
<h1>Every card</h1>

{#if failure}
  <!-- A failure redirects: say what was refused and name the thing that works.
       No CLI command — this screen's audience is a reader with no terminal in
       the window, and the library's failure state is the one that may name one
       because its audience is whoever mis-set the data dir. -->
  <p class="note">These cards did not open: {failure}</p>
  <p class="hint">
    Every card is also on the book that minted it — the <a href="/library">library</a> is the way
    there.
  </p>
{:else}
  <WallControls {years} {anyOpen} {scope} {sort} onscope={pickScope} onsort={pickSort} />

  {#if !loaded && rows.length === 0}
    <p class="hint">Reading the wall…</p>
  {:else if total === 0 && scope.kind === 'open'}
    <!-- The chip exists because a read is open, so this is only reachable if
         one closed between the picker's answer and the wall's. Ordinary, and it
         names where those cards went. -->
    <p class="note">Nothing open right now.</p>
    <p class="hint">Every read that has ended is under a year above, and All is the whole wall.</p>
  {:else if total === 0 && yearShown !== null}
    <!--
      A year the picker offered and no card belongs to.

      **Item 51 made this nearly unreachable and it is kept anyway.** The years
      come from `readings.finished_at` under this wall's own filter now, so an
      offered year has rows by construction — where the old `activityByMonth`
      proxy offered a year because a *note* was written in it. What is left is
      the race: a read closing between the picker's answer and the wall's. Still
      not a failure, still no *yet*.
    -->
    <p class="note">No cards from {yearShown}.</p>
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
      something in the <a href="/library">library</a> is what mints one.
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
        <button class="act" type="button" disabled={page === 0} onclick={() => turn(page - 1)}>
          ‹ Back
        </button>
        <span>Page {page + 1} of {pages}</span>
        <button
          class="act"
          type="button"
          disabled={page >= pages - 1}
          onclick={() => turn(page + 1)}
        >
          More ›
        </button>
      </nav>
    {/if}
  {/if}
{/if}

<style>
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
    grid-template-columns: repeat(auto-fill, minmax(min(258px, 100%), 1fr));
    gap: var(--s-5) var(--s-4);
    /*
     * **Stretched, not `align-items: start`.** The passage is one to four lines
     * and half the cards on a real wall have none at all, so the height spread
     * is nearly 2× — and start-aligned that came out as bottoms missing by
     * 70–115px in alternating columns, which reads as broken masonry rather
     * than as a grid. Equal rows put the ragged edge *inside* a card, where it
     * is whitespace, instead of between cards, where it is a defect. The
     * per-book page keeps `start`: it holds one or two cards and has no rows.
     *
     * The minimal pass briefly unboxed the card and set this to `start` on the
     * argument that there was no longer an edge to be ragged. The box came back
     * — `Card.svelte` says why — and so did this.
     */
  }
  .tally {
    margin: var(--s-5) 0 0;
    font-size: var(--t-fine);
    color: var(--ink-dim);
  }
  /* Two `.act`s and a sentence. The boxes went with every other control box in
     the minimal pass: *Back* and *More* beside `Page 2 of 5` are unambiguous as
     words, and a border around each made the foot of the page look like a
     second toolbar answering the one at the top. */
  .paging {
    display: flex;
    align-items: baseline;
    gap: var(--s-4);
    margin-top: var(--s-2);
    font-size: var(--t-fine);
    color: var(--ink-dim);
  }
  h1 {
    font-size: var(--t-lead);
    margin: 0 0 var(--s-4);
  }
  .note {
    max-width: var(--column);
    margin: 0 0 var(--s-2);
  }
  /* The move out of an empty state, in the accent — it is the only link in the
     sentence and it has to look like one. */
  .hint a {
    color: var(--accent-text);
  }
</style>
