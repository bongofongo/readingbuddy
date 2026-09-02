<script lang="ts">
  /**
   * One book on the wall: a jacket, in a fixed box, that is a link.
   *
   * ## A tile is its jacket
   *
   * The caption is **off** by default and the tile carries its identity in the
   * accessible name and the `title` attribute instead. That is not a shortcut —
   * it is what the group it is in is for. The year groups are books you have
   * read, where re-finding one is *recognition*: you already hold a template of
   * the cover, and a caption there buys a text row on every tile, doubles the
   * vertical space and turns a wall of images into a mixed image-and-text
   * surface that scans worse than either pure form.
   *
   * `caption` is therefore a property of the **group**, never a setting. See
   * `shelf/arrangements.ts` for where it is decided and why "No reading
   * recorded" is the one group that turns it on.
   *
   * ## The box is fixed, and the jacket letterboxes inside it
   *
   * This used to reserve `cover_aspect` — the measured shape of the stored file
   * — so the grid did not reflow as images arrived. The box is now a flat 2:3
   * for every tile, and the difference matters at 86px: with a per-book aspect
   * the tops and bottoms of a row do not line up, and a row of jackets whose
   * baselines disagree does not read as a shelf. The measurement has not stopped
   * being useful — it is what `Jacket` needs to know it is letterboxing rather
   * than failing — but the *field* wants one rhythm.
   */
  import { client, type StoredBook } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { bookHref } from '$lib/nav';
  import { authorsLabel, titleLabel } from '$lib/phrasing';

  let { book, caption = false }: { book: StoredBook; caption?: boolean } = $props();

  const cover = $derived(client().coverSrc(book));
  // `authors_display`, never `authors`: the flip out of `Surname, Given` is the
  // engine's (item 17) and the record keeps the origin's own spelling.
  const authors = $derived(authorsLabel(book.authors_display));
  const title = $derived(titleLabel(book.title));
  // The absence itself, not the word for it — the word is `titleLabel`'s and is
  // deliberately indistinguishable from a real title once produced.
  const untitled = $derived(!book.title || book.title.trim() === '');

  /**
   * What the tile is called when there is no caption under it.
   *
   * The author is in it, not only the title: a wall is where two editions of one
   * book and two books of one title sit side by side, and a screen reader moving
   * across a row of jackets gets exactly what a sighted reader gets from the
   * artwork. It is also the `title` attribute, which is the hover the mouse has.
   */
  const name = $derived(authors ? `${title} — ${authors}` : title);

  /**
   * Where the tile goes — its page, or reading mode if this book is one you are
   * in the middle of.
   *
   * The wall holds both, so the destination is a property of the *row* and not
   * of the wall: a tile in the *Still reading* group leads into the book, and
   * every tile in a year group leads to its page. `$lib/nav.ts` holds the rule
   * so the tile and the "Reading now" preview cannot disagree about it.
   */
  const into = $derived(bookHref(book));
</script>

<a class="tile" class:captioned={caption} href={into} title={name} aria-label={name}>
  <div class="art">
    <!-- The three states — bytes, a plate in this jacket's own colour, the hatch
         — live in `Jacket`, so a coverless book cannot come to look like two
         different books on two screens of one app. -->
    <Jacket src={cover} accent={book.cover_accent} />
  </div>
  {#if caption}
    <!-- `aria-hidden`, because the link already carries this text as its name:
         without it a screen reader reads the title twice, once as the link and
         once as the caption inside it. -->
    <div class="meta" aria-hidden="true">
      <span class="title" class:untitled>{title}</span>
      {#if authors}
        <span class="authors">{authors}</span>
      {/if}
    </div>
  {/if}
</a>

<style>
  .tile {
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
    min-width: 0;
  }
  .art {
    position: relative;
    /* Reserved before any image exists, and identical for every tile. */
    aspect-ratio: 2 / 3;
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
    gap: 0.1rem;
    min-width: 0;
  }
  .title {
    font-size: var(--t-micro);
    line-height: 1.3;
    /* Two lines' worth of box whether or not the title fills it, so the authors
       under a row of tiles share a baseline. Without it a row has its captions
       on two different baselines depending on which titles happened to wrap. */
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
  .authors {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  /* "Untitled" is our word for an absence, not a book actually called that.
     Dimmed so it does not read as a title in the same voice as its neighbours. */
  .title.untitled {
    color: var(--ink-dim);
    font-style: italic;
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
