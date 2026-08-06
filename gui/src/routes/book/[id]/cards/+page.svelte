<script lang="ts">
  /**
   * A book's cards — one per reading, side by side.
   *
   * **Reached by selecting a book**, which is the user's ruling and is also what
   * the cost of the data says. Entry 44: the card's passage is *"one call per
   * card, which is right for a card reached by selecting a book and wrong for
   * the wall of cards across the library"* — that wall wants item 43 first, and
   * a route that quietly grew into it would be the N+1 across four hundred cards
   * item 18 exists to remove.
   *
   * Two cards on one book is the whole point of the screen: *"the two sit side
   * by side showing what changed. What you rated it at 22 and at 31. Which
   * passages you marked both times. That comparison is not available anywhere
   * else in the world, because nowhere else kept both readings."* So the layout
   * is a track that puts them beside each other wherever there is room, and the
   * common case — one reading, one card — is a single card at a readable width
   * rather than a lonely column in a two-column grid.
   */
  import { page } from '$app/state';
  import type { ReadingDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import Card from '$lib/card/Card.svelte';
  import { titleLabel } from '$lib/phrasing';

  const id = $derived(Number(page.params.id));

  let book = $state<StoredBook | null>(null);
  let readings = $state<ReadingDto[]>([]);
  let missing = $state(false);
  let loaded = $state(false);
  let failure = $state<string | null>(null);

  $effect(() => {
    const which = id;
    loaded = false;
    (async () => {
      const api = client();
      const b = await api.getBook(which);
      if (b === null) {
        missing = true;
        return;
      }
      book = b;
      readings = await api.listReadings(which);
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
  {#if readings.length === 0}
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
    <!-- A card per reading, oldest first — the order `list_readings` returns and
         the order a reading life happened in. No ordinal on any of them: see
         `Card.svelte`, and item 41. -->
    <div class="wall">
      {#each readings as r (r.id)}
        <Card {book} reading={r} />
      {/each}
    </div>
  {/if}
{:else}
  <p class="hint">Opening…</p>
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
    overflow-wrap: anywhere;
    max-width: var(--measure);
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
    max-width: calc(var(--measure) + 23rem);
    align-items: start;
  }
  .wall > :global(article) {
    max-width: 34rem;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
  .note a {
    color: var(--accent-text);
  }
</style>
