<script lang="ts">
  /**
   * The dense layout: one line per book, jacket reduced to a chip.
   *
   * It earns its place twice. It is **useful** — a long library is read faster
   * as a list than as a wall of jackets, and the title is legible here at a size
   * where a grid has clipped it to two lines. And it is what makes the seam
   * honest: it needs a fixed box rather than a reserved aspect, it puts the
   * title where the grid puts a cover, and it has no hover lift. Everything the
   * contract in `layouts.ts` did not anticipate would have surfaced here.
   *
   * Note what it still does not do: choose or order its books. That is the
   * engine's, and a layout that sorted would be a second opinion about an order
   * we were handed.
   */
  import { plateColor } from '$lib/accent';
  import { client } from '$lib/api/client';
  import { authorsLabel, progressLabel, readingStateLabel, titleLabel } from '$lib/phrasing';

  import type { ShelfLayoutProps } from './layouts';

  let { books }: ShelfLayoutProps = $props();
</script>

<ul class="rows">
  {#each books as book (book.id)}
    {@const state = readingStateLabel(book.reading_state)}
    {@const progress = progressLabel(book.progress)}
    {@const authors = authorsLabel(book.authors_display)}
    {@const cover = client().coverSrc(book)}
    {@const plate = plateColor(book.cover_accent)}
    <li>
      <a href={`/book/${book.id}`}>
        <!-- A chip, not a thumbnail: at this size a jacket is a colour and a
             hint of its composition, so the accent carries it when the bytes
             are absent and nothing looks broken.

             No measurement gets the hatch, never a flat pale fill. A fill at
             this size is indistinguishable from a genuinely pale jacket, and
             `gui/CLAUDE.md` requires "never measured" and "this jacket is grey"
             to stay different pictures — the grid already honoured that and
             this arrangement did not. -->
        <span class="chip" class:unmeasured={!plate} style:--plate={plate ?? 'transparent'}>
          {#if cover}
            <img src={cover} alt="" loading="lazy" />
          {/if}
        </span>
        <span class="line">
          <span class="title">{titleLabel(book.title)}</span>
          {#if authors}
            <span class="authors">{authors}</span>
          {/if}
        </span>
        {#if state || progress}
          <span class="state">{[state, progress].filter(Boolean).join(' · ')}</span>
        {/if}
      </a>
    </li>
  {/each}
</ul>

<style>
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    /* A row is a line of text, and a line of text has a readable length. Left
       full-bleed on a wide window the state label sat ~600px from the title it
       describes and the two stopped reading as one row. */
    max-width: 78ch;
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  a {
    display: flex;
    align-items: center;
    gap: 0.85rem;
    padding: 0.55rem 0.5rem;
    border-radius: var(--radius);
    min-width: 0;
  }
  a:hover {
    background: var(--bg-raised);
  }
  .chip {
    flex: none;
    width: 26px;
    height: 38px;
    border-radius: 2px;
    overflow: hidden;
    background: var(--plate);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--ink) 14%, transparent);
  }
  .chip.unmeasured {
    background: repeating-linear-gradient(
      -45deg,
      transparent,
      transparent 3px,
      var(--line) 3px,
      var(--line) 4px
    );
  }
  .chip img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .line {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1;
  }
  .title {
    font-size: 0.9rem;
    line-height: 1.35;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .authors {
    font-size: 0.76rem;
    color: var(--ink-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .state {
    flex: none;
    font-size: 0.76rem;
    color: var(--accent-text);
    white-space: nowrap;
  }
  /*
   * At a phone width the state **moves**, and is never dropped.
   *
   * It was `display: none` here, which was wrong on the axiom rather than on
   * taste: the state is *what you did*, the one thing this surface exists to
   * say and the only number it is allowed to carry. Hiding it left title and
   * author — the shelf saying what you own and nothing about your reading.
   * So it becomes a third line under the author instead of competing with the
   * title for the same one.
   */
  @media (max-width: 480px) {
    a {
      align-items: flex-start;
      flex-wrap: wrap;
    }
    .chip {
      align-self: center;
    }
    .state {
      flex-basis: 100%;
      margin-left: calc(26px + 0.85rem);
    }
  }
</style>
