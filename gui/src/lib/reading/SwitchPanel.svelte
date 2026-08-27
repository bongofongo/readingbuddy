<script lang="ts">
  /**
   * The other books you have open.
   *
   * ## Links, not buttons, and that is what makes the route honest
   *
   * Each row is an `<a href="/reading?book=12">`. The URL is the subject of the
   * route, so following a link *is* the switch — no handler, no store, and the
   * back button walks the reader through the books they looked at. A button
   * calling `goto` would work identically until somebody middle-clicked one.
   *
   * The `onclick` beside it closes the panel and does not preventDefault, so the
   * navigation is still the anchor's.
   *
   * ## The set is the engine's
   *
   * `currently_reading` is a selection predicate and lives below the seam (item
   * 17): this panel does not decide what counts as open, does not sort, and does
   * not drop the book you are on — it marks it instead, because a list that
   * silently removed the current entry would change length as you moved through
   * it.
   *
   * ## The way to a book that is not open
   *
   * Reading mode follows open reads and cannot start one. That is not a dead
   * end and it is not a gap to paper over here: starting a read is an act with a
   * home, and the library is one link away at the bottom of the list.
   */
  import type { OpenReading } from '$lib/api/client';
  import { authorsLabel, progressDetail, titleLabel } from '$lib/phrasing';

  let {
    open,
    currentBookId,
    onpicked,
  }: {
    open: OpenReading[];
    currentBookId: number;
    onpicked: () => void;
  } = $props();
</script>

<div class="switch">
  <h2 class="band-title">Open</h2>

  <ul>
    {#each open as o (o.book.id)}
      {@const here = o.book.id === currentBookId}
      <li>
        <a href="/reading?book={o.book.id}" aria-current={here ? 'page' : undefined} onclick={onpicked}>
          <span class="title">{titleLabel(o.book.title)}</span>
          {#if authorsLabel(o.book.authors_display)}
            <span class="by">{authorsLabel(o.book.authors_display)}</span>
          {/if}
          {#if progressDetail(o.book.progress)}
            <span class="where">{progressDetail(o.book.progress)}</span>
          {/if}
        </a>
      </li>
    {/each}
  </ul>

  <!-- Deliberately not another link called *The library*: the verbs row already
       has one two inches below, and two links with one name to one place is the
       shape a reader reads as two destinations. This one names what is at the
       other end — the shelf is the library page's own word for its second
       band. -->
  <p class="hint">A book that is not open gets opened <a href="/">on the shelf</a>.</p>
</div>

<style>
  .switch {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    max-height: 48vh;
    overflow-y: auto;
  }
  li a {
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: baseline;
    gap: 0.15rem 0.8rem;
    padding: 0.5rem 0.6rem;
    border-radius: var(--radius);
    border: 1px solid transparent;
  }
  li a:hover {
    border-color: var(--line);
  }
  .title {
    color: var(--ink);
    /* A 220-character title is in the fixture on purpose: one line, ellipsised,
       so a row cannot push the list past the window. */
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .by {
    grid-column: 1;
    color: var(--ink-dim);
    font-size: 0.85rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .where {
    grid-column: 2;
    grid-row: 1;
    color: var(--ink-dim);
    font-size: 0.85rem;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  /* The one you are on, said in the accent — *true right now*, and the reason
     the row is still in the list rather than removed from it. */
  li a[aria-current='page'] {
    border-color: var(--accent);
  }
  li a[aria-current='page'] .title {
    color: var(--accent-text);
  }
  .hint {
    margin: 0;
    color: var(--ink-dim);
    font-size: 0.85rem;
  }
  .hint a {
    color: var(--accent-text);
  }
</style>
