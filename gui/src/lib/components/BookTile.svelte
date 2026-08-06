<script lang="ts">
  import { client, type StoredBook } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { authorsLabel, progressLabel, readingStateLabel, titleLabel } from '$lib/phrasing';

  let { book, proud = false }: { book: StoredBook; proud?: boolean } = $props();

  const cover = $derived(client().coverSrc(book));
  const state = $derived(readingStateLabel(book.reading_state));
  // `authors_display`, never `authors`: the flip out of `Surname, Given` is the
  // engine's (item 17) and the record keeps the origin's own spelling.
  const authors = $derived(authorsLabel(book.authors_display));
  // Beside the state, not instead of it — "Reading · 35%" — and absent for a
  // book with nothing recorded. Every number in it was computed by the engine.
  const progress = $derived(progressLabel(book.progress));
  const title = $derived(titleLabel(book.title));
  // The absence itself, not the word for it — the word is `titleLabel`'s and is
  // deliberately indistinguishable from a real title once produced.
  const untitled = $derived(!book.title || book.title.trim() === '');

  /**
   * The box the tile reserves, before any image loads.
   *
   * `cover_aspect` is a **measurement of the stored file** (item 20b), so using
   * it is what stops the grid reflowing when jackets arrive at their true
   * shapes. It is `null` for a book with no cover *and* for a library predating
   * the back-fill — `rb covers`, which `make dev-db` runs — so the fallback is
   * not the exotic path and has to look deliberate. 2:3 is the trade paperback
   * the rest of the shelf mostly is.
   */
  const aspect = $derived(book.cover_aspect ?? 2 / 3);
</script>

<a class="tile" class:proud href={`/book/${book.id}`}>
  <div class="art" style:aspect-ratio={aspect}>
    <!-- The three states — bytes, a plate in this jacket's own colour, the hatch
         — moved into `Jacket` when the book view needed the same three (item
         27). One composition, so a coverless book cannot come to look like two
         different books on two screens of one app. -->
    <Jacket src={cover} accent={book.cover_accent} />
  </div>
  <div class="meta">
    <span class="title" class:untitled>{title}</span>
    {#if authors}
      <span class="authors">{authors}</span>
    {/if}
    {#if state || progress}
      <span class="state">{[state, progress].filter(Boolean).join(' · ')}</span>
    {/if}
  </div>
</a>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 0.55rem;
    min-width: 0;
  }
  .art {
    position: relative;
    background: var(--bg-raised);
    border-radius: var(--radius);
    overflow: hidden;
    /* The jacket sits on the shelf rather than being drawn on it. One soft drop
       plus a hairline; the hairline is inset so a pale cover still has an edge
       against a pale background, which a border alone does not give you. */
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent),
      0 1px 2px rgb(0 0 0 / 0.2),
      0 6px 14px -8px rgb(0 0 0 / 0.45);
    transition:
      transform 140ms ease,
      box-shadow 140ms ease;
  }
  .tile:hover .art,
  .tile:focus-visible .art {
    transform: translateY(-3px);
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 18%, transparent),
      0 2px 4px rgb(0 0 0 / 0.22),
      0 14px 24px -10px rgb(0 0 0 / 0.5);
  }
  .meta {
    display: flex;
    flex-direction: column;
    gap: 0.12rem;
    min-width: 0;
  }
  .title {
    font-size: 0.86rem;
    line-height: 1.3;
    /* Two lines' worth of box whether or not the title fills it, so the author
       and state lines share a baseline across a row. Without it a row of five
       tiles had its captions on two different baselines depending on which
       titles happened to wrap. */
    min-height: 2.6em;
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
    color: var(--accent-text);
  }
  /* "Untitled" is our word for an absence, not a book actually called that.
     Dimmed so it does not read as a title in the same voice as its neighbours. */
  .title.untitled {
    color: var(--ink-dim);
    font-style: italic;
  }

  /* Pulled proud: the same tile, larger, for the strip above the shelf.
     Size alone does **not** carry the hierarchy — at 13% larger the strip read
     as a second grid, and at one width it rendered smaller than the shelf. The
     ground and the shelf rule in `+page.svelte` are what make it a different
     kind of object; this only gives the title the room that ground implies. */
  .proud .title {
    font-size: 0.95rem;
    -webkit-line-clamp: 3;
    line-clamp: 3;
  }

  @media (prefers-reduced-motion: reduce) {
    .art {
      transition: none;
    }
    .tile:hover .art,
    .tile:focus-visible .art {
      transform: none;
    }
  }
</style>
