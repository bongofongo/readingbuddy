<script lang="ts">
  /**
   * A book's cards — one per reading, side by side.
   *
   * **Reached by selecting a book**, which is the user's ruling. It stayed when
   * `/cards` arrived (item 47) because the two answer different questions — *my
   * reading life* and *this book's reads* — and a book's cards reached from the
   * book are not a filtered view of a library wall in the reader's head,
   * whatever they are in the query.
   *
   * Two cards on one book is the whole point of the screen: *"the two sit side
   * by side showing what changed. What you rated it at 22 and at 31. Which
   * passages you marked both times. That comparison is not available anywhere
   * else in the world, because nowhere else kept both readings."* So the layout
   * is a track that puts them beside each other wherever there is room, and the
   * common case — one reading, one card — is a single card at a readable width
   * rather than a lonely column in a two-column grid.
   *
   * ## It asks the wall's question, narrowed to one book
   *
   * `listReadingRows` with `ReadingFilterDto.book_id` set, which is what that
   * field exists for — its own doc comment says so, and item 43's entry says the
   * single-book page being served by the same list *"is what makes the item
   * complete rather than half done"*. It replaces `listReadings` **plus one
   * `cardPassage` per card**: entry 44 named that N+1 an item in advance, and
   * this is where it is paid off. The passage and the read number now arrive
   * with the row.
   *
   * The remaining three calls a card makes — marks, notes, and the review whose
   * rating is a fourth hop — are `Card.svelte`'s `detail` prop, which this page
   * sets and the wall does not. They are per *read* and a book has a handful, so
   * the bound is a fact about reading rather than a page size somebody chose;
   * `docs/decisions.md` entry 47 has the argument and the arithmetic.
   */
  import { page } from '$app/state';
  import { client, type ReadingRow, type StoredBook } from '$lib/api/client';
  import Card from '$lib/card/Card.svelte';
  import { titleLabel } from '$lib/phrasing';

  const id = $derived(Number(page.params.id));

  let book = $state<StoredBook | null>(null);
  let rows = $state<ReadingRow[]>([]);
  let missing = $state(false);
  let loaded = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
    const which = id;
    loaded = false;
    (async () => {
      const api = client();
      // Still asked for, and not taken from `rows[0].book`: a book with no
      // reading has no row to take it from, and the empty state below names the
      // book. The missing-book case needs it too.
      const b = await api.getBook(which);
      if (b === null) {
        missing = true;
        return;
      }
      book = b;
      const all = await api.listReadingRows({
        // `-1` is no limit — every reading of one book, which is a handful.
        // This page does not page, and if it ever did, the reorder below would
        // have to become an engine item rather than a wider slice.
        limit: -1,
        // Any sort would do, since the order is imposed below; `started` is the
        // one whose key **is** `reading_age_key`, so this is that list reversed
        // rather than reshuffled.
        sort: 'started',
        offset: 0,
        filter: { book_id: which, status: null, open: null, finished_in: null },
      });
      // Oldest first — the order a reading life happened in, and the order the
      // side-by-side comparison reads in.
      //
      // **Sorted by the engine's own ordinal, not reversed.** Every arm of
      // `ReadingSort` is descending and there is no ascending one, so this had
      // to come from somewhere; `read_number` is `reading_age_key` counted by
      // the engine over every sibling (item 41), so ordering by it is *reading a
      // field* rather than a frontend inventing an order. `.reverse()` would
      // have been the same answer today and a silent lie the day anything paged
      // this list — a reversed page is not the head of a reversed list.
      rows = all.toSorted((a, b2) => a.read_number - b2.read_number);
    })()
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)))
      .finally(() => (loaded = true));
  });
</script>

<svelte:head>
  <title>{book ? titleLabel(book.title) : 'Cards'} — readingbuddy</title>
</svelte:head>

<a class="back" href={`/book/${id}`}>← The book</a>

{#if failure}
  <p class="note">These cards did not open: {failure}</p>
  <p class="hint">
    The book itself may still be fine — <code>rb show {id}</code> reads the same row.
  </p>
{:else if missing}
  <p class="note">There is no book with that id.</p>
  <p class="hint">
    It may have been folded into another by a merge — <code>rb book list</code> shows what is there
    now.
  </p>
{:else if book && loaded}
  <!-- "Cards", not the book's title: every card below carries the title itself,
       because a card is an object you could show somebody and one without a
       title is not. A heading repeating it would print the same words three
       times on a reread. -->
  <h1>Cards</h1>
  {#if rows.length === 0}
    <!--
      Idle is not blank and never an apology — and this state was three things
      wrong before it was looked at rather than reasoned about.

      **No "yet"**: that one word turns an absence into something outstanding,
      which is the grammar of *pending* in a softer coat. **No CLI**: the
      library's failure state may name `make dev-db` because its audience is
      whoever mis-set the data dir, but this is an ordinary book with no card and
      its audience is a reader — there is no terminal in this window, so a
      command names no move that can be taken here, and `rb read start 4` leaked
      a raw row id into user-facing copy besides. **And it names the book**:
      without the title this was two sentences on an empty field with nothing
      saying whose cards were missing.
    -->
    <p class="note">
      <a href={`/book/${book.id}`}>{titleLabel(book.title)}</a> has no card. A card is minted by a
      read.
    </p>
    <p class="hint">Reading it is what mints one, and the book is where a read begins.</p>
  {:else}
    <!-- A card per reading, oldest first — see the reorder above for where that
         order comes from now. `detail`, because this is the page a card is
         reached from and the one whose card is whole: the rating, the notes and
         the marks are three requests each, and a book has a handful of reads. -->
    <div class="wall">
      {#each rows as row (row.reading.id)}
        <Card {row} detail />
      {/each}
    </div>
  {/if}
{:else}
  <p class="hint">Opening…</p>
{/if}

<style>
  .back {
    color: var(--ink-dim);
    font-size: var(--t-fine);
    display: inline-block;
    margin-bottom: 1.1rem;
  }
  h1 {
    font-size: var(--t-lead);
    color: var(--ink-dim);
    font-weight: 500;
    margin-bottom: 1.4rem;
    overflow-wrap: anywhere;
    max-width: var(--column);
  }
  /*
   * Beside each other where there is room, stacked where there is not.
   *
   * `auto-fit` rather than a fixed two columns: one reading is the ordinary
   * case, and a grid laid out for two leaves the single card looking like it
   * lost its sibling.
   *
   * **The ceiling is on the card, not on the track**, and that is the whole of
   * what two rounds of looking at this settled. A `1fr` track stretched a lone
   * card to a thousand pixels and put four words of an empty state across the
   * window. Putting the ceiling on the *track* fixed that and cost something
   * worse: at 720px two 30rem tracks no longer fit, so the reread stacked — and
   * the side-by-side comparison is the entire reason this screen exists, so
   * losing it at the width a laptop half-screen actually is was the wrong trade.
   *
   * A `1fr` track with a `max-width` on the item keeps both: two cards share the
   * width wherever two fit, and one card is a card rather than a banner.
   */
  .wall {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(20rem, 100%), 1fr));
    gap: 1.2rem;
    max-width: calc(var(--column) + 23rem);
    align-items: start;
  }
  .wall > :global(article) {
    max-width: 34rem;
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
  .note a {
    color: var(--accent-text);
  }
</style>
