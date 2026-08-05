<script lang="ts">
  import { client, type StoredBook } from '$lib/api/client';
  import { authorsLabel, readingStateLabel, titleLabel } from '$lib/phrasing';

  let { book }: { book: StoredBook } = $props();

  const cover = $derived(client().coverSrc(book));
  const state = $derived(readingStateLabel(book.reading_status));
  const authors = $derived(authorsLabel(book.authors));
</script>

<a class="tile" href={`/book/${book.id}`}>
  <!-- The aspect box is reserved whether or not a cover loads. Nothing stores
       cover dimensions yet (item 20b), and a tile that resizes when an image
       arrives makes the whole grid jump. -->
  <div class="art">
    {#if cover}
      <img src={cover} alt="" loading="lazy" />
    {:else}
      <!-- Not a broken-image icon and not an apology. An empty state is a
           designed one. -->
      <span class="bare" aria-hidden="true"></span>
    {/if}
  </div>
  <div class="meta">
    <span class="title">{titleLabel(book.title)}</span>
    {#if authors}
      <span class="authors">{authors}</span>
    {/if}
    {#if state}
      <span class="state">{state}</span>
    {/if}
  </div>
</a>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .art {
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    overflow: hidden;
  }
  .art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .bare {
    display: block;
    height: 100%;
    background: repeating-linear-gradient(
      -45deg,
      transparent,
      transparent 7px,
      var(--line) 7px,
      var(--line) 8px
    );
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .title {
    font-size: 0.86rem;
    line-height: 1.3;
    /* Two lines then ellipsis. The dev library holds a 220-character title on
       purpose, and `-webkit-line-clamp` is the only thing that clips by rendered
       line rather than by character count — which is what makes it correct for
       the CJK title beside it. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .authors,
  .state {
    font-size: 0.76rem;
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .state {
    color: var(--accent);
  }
</style>
