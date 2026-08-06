<script lang="ts">
  /**
   * The default shelf layout: jackets, face out, on a responsive grid.
   *
   * The tile reserves its box from `cover_aspect`, so the columns stay aligned
   * while images arrive and a tall jacket does not shove its neighbours. What
   * this file owns is the *field* — how wide a column is and how the rows
   * breathe; the jacket itself is `BookTile`.
   */
  import BookTile from '$lib/components/BookTile.svelte';

  import type { ShelfLayoutProps } from './layouts';

  let { books }: ShelfLayoutProps = $props();
</script>

<div class="grid">
  {#each books as book (book.id)}
    <BookTile {book} />
  {/each}
</div>

<style>
  .grid {
    display: grid;
    /* `auto-fill`, not `auto-fit`: with `auto-fit` a shelf of three books
       stretches them to the full width of the window and a jacket the size of a
       poster is not what a shelf looks like. Empty tracks stay empty. */
    grid-template-columns: repeat(auto-fill, minmax(132px, 1fr));
    gap: 1.6rem 1.1rem;
    align-items: start;
  }

  /* Tighter columns where there is no room for them to be generous. The
     terminal sibling's `every_screen_draws_at_every_size` exists because the
     layout bug is always at an extreme, and 320px is this app's. */
  @media (max-width: 420px) {
    .grid {
      grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
      gap: 1.2rem 0.8rem;
    }
  }
</style>
