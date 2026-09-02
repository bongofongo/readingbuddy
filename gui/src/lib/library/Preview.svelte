<script lang="ts">
  /**
   * One book you have open, previewed — the only place on the home surface that
   * carries more than a jacket.
   *
   * ## The mark is gone from the face of it, and is still the order
   *
   * This used to quote the last thing you kept or wrote, under *Latest passage*
   * or *Latest note*. It no longer does: the preview is the book, its progress,
   * and the two places you can take it. What replaced the quotation is a pair of
   * icon doors, which is the same trade the minimal pass made everywhere —
   * a surface says less and points more precisely.
   *
   * **`latestMark` did not go with it, and must not.** The mark is what
   * `latest.ts` sorts this page by, so it is still fetched and still decides the
   * order; only the rendering of it went. Deleting the two calls in the page
   * because nothing draws their result any more would silently replace the
   * ordering that keeps stale readings sinking — which is that file's whole
   * argument — with the engine's arbitrary one. The N+1 is therefore still here
   * and still owed a request.
   *
   * ## The two doors, and the duplication in them
   *
   * `/reading?book=…` and `/book/…` — the book with the window to itself, and
   * the book's own work surface. Both are `.act`, the quiet control, and
   * deliberately **not** `.door`: a door is `--accent-text`, and two of them per
   * preview times however many books are open would spend the accent budget on
   * the calmest surface in the app.
   *
   * The reading door restates where the jacket and the title already go
   * (`bookHref`, and every book here has an open reading by construction). That
   * is a duplicate by entry 57's third corollary, and it is taken knowingly: the
   * alternative is rewiring `bookHref`, whose rule is shared with the wall's
   * tiles and is documented in three places as living in `nav.ts` precisely so
   * it has one copy. A named, labelled control beside its sibling is worth more
   * here than the strict reading — but it is the thing to revisit if the pair
   * ever feels redundant.
   *
   * ## The number rule, restated because this band carries the most digits
   *
   * `p. 214 of 502 · 43%` describes **one book you chose to open**. It never
   * describes the shelf. There is no "3 books in progress", no total, and no
   * aggregate anywhere on this page.
   *
   * ## Two absences, two different renderings
   *
   * - no page count → the rail is omitted and the line degrades to
   *   `p. 214 · started …`. Never `p. 214 of 0`: the engine already collapses a
   *   zero length to absence, and a bar must not draw an empty track over one.
   * - no title → `titleLabel`'s word, dimmed and italic, the same absence in the
   *   same voice as the tile and the book page.
   */
  import type { ReadingDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { bookHref, readingHref } from '$lib/nav';
  import { authorsLabel, dayLabel, progressDetail, titleLabel } from '$lib/phrasing';

  let { book, reading }: { book: StoredBook; reading: ReadingDto } = $props();

  const cover = $derived(client().coverSrc(book));
  /**
   * Where the jacket and the title go: **reading mode**, not the book's page.
   *
   * Every book in this band has an open reading, so `bookHref` sends all of them
   * to `/reading?book=…` — the rule is in `$lib/nav.ts` rather than spelt here
   * because the wall's tiles obey the same one, and two copies of it drift.
   */
  const into = $derived(bookHref(book));
  const title = $derived(titleLabel(book.title));
  const untitled = $derived(!book.title || book.title.trim() === '');
  const authors = $derived(authorsLabel(book.authors_display));
  // **This** reading's progress, not the book's: on a reread the book's is
  // today's numbers, and this row is about the read it names.
  const detail = $derived(progressDetail(reading.progress));
  const began = $derived(dayLabel(reading.started_at));
  /**
   * The fill, as a fraction, or `null`.
   *
   * `fraction` is the engine's and is absent for a page with no honest
   * denominator — which is the whole reason the rail can be omitted rather than
   * drawn empty. Nothing here divides: `gui/CLAUDE.md` names
   * `current_page / page_count` as the derivation that must never appear in a
   * component, and one book in the dev library has a `page_count` of zero to
   * catch it.
   */
  const fraction = $derived(
    reading.progress.progress === 'started' ? reading.progress.fraction : null,
  );
  /**
   * The two doors, or `null` for a book the engine has not stored.
   *
   * `StoredBook`'s id is nullable and `bookHref` already answers that case by
   * sending the jacket to the library; the pair has no such fallback, so it is
   * simply not drawn. `/book/null` was reachable from the old *The book* link
   * and is the one thing that got quietly fixed on the way past.
   */
  const doors = $derived(
    book.id === null ? null : { read: readingHref(book.id), page: `/book/${book.id}` },
  );
</script>

<article class="preview">
  <a class="art" href={into} tabindex="-1" aria-hidden="true">
    <Jacket src={cover} accent={book.cover_accent} />
  </a>

  <div class="body">
    <h3><a href={into} class:untitled>{title}</a></h3>
    {#if authors}
      <p class="by">{authors}</p>
    {/if}

    {#if fraction !== null}
      <!-- A surface, so `--accent` rather than `--accent-text`: it carries no
           words, which is the line `app.css` draws between the two tokens.
           `aria-hidden` because the line under it says the same thing in
           figures, and a bar a screen reader announces as a percentage is the
           same fact twice. -->
      <div class="rail" aria-hidden="true">
        <span class="fill" style:width={`${Math.round(fraction * 100)}%`}></span>
      </div>
    {/if}

    {#if detail || began}
      <p class="where">{[detail, began && `started ${began}`].filter(Boolean).join(' · ')}</p>
    {/if}

    {#if doors}
      <!--
        The two places this book goes, as icons.

        Icon-only, so each carries `aria-label` **and** `title`: the label is the
        accessible name and the tooltip is the sighted reader's, and a control
        whose whole face is a glyph owes both. The `<svg>`s are `aria-hidden` so
        the name is said once rather than twice.

        Inline paths rather than a sprite or an icon package: two glyphs do not
        earn a dependency, and the artifact CSP the webview runs under would
        block an external one anyway.
      -->
      <p class="doors">
        <a class="act icon" href={doors.read} aria-label="Read {title}" title="Read">
          <!-- An open book: the surface that gives it the window. -->
          <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
            <path d="M8 4.4C6.8 3.4 5.2 3 3.4 3.1a.8.8 0 0 0-.8.8v7.3c0 .45.36.8.8.79 1.8-.1 3.4.3 4.6 1.3" />
            <path d="M8 4.4c1.2-1 2.8-1.4 4.6-1.3.44 0 .8.35.8.8v7.3c0 .45-.36.8-.8.79-1.8-.1-3.4.3-4.6 1.3" />
            <path d="M8 4.4v9.6" />
          </svg>
        </a>
        <a class="act icon" href={doors.page} aria-label="The page for {title}" title="The book">
          <!-- A card with lines: passages, notes, reads and about. -->
          <svg viewBox="0 0 16 16" aria-hidden="true" focusable="false">
            <rect x="2.6" y="2.6" width="10.8" height="10.8" rx="1.7" />
            <path d="M5.3 6.3h5.4M5.3 8.7h5.4M5.3 11.1h3.1" />
          </svg>
        </a>
      </p>
    {/if}
  </div>
</article>

<style>
  .preview {
    display: grid;
    grid-template-columns: 68px minmax(0, 1fr);
    gap: 1.1rem;
    align-items: start;
    min-width: 0;
  }
  .art {
    display: block;
    aspect-ratio: 2 / 3;
    background: var(--bg-raised);
    border-radius: var(--radius);
    overflow: hidden;
    box-shadow:
      inset 0 0 0 1px color-mix(in srgb, var(--ink) 12%, transparent),
      0 1px 2px rgb(0 0 0 / 0.2),
      0 6px 14px -8px rgb(0 0 0 / 0.45);
  }
  .body {
    min-width: 0;
  }
  h3 {
    font-size: var(--t-fine);
    line-height: 1.3;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  h3 a:hover {
    color: var(--accent-text);
  }
  .untitled {
    color: var(--ink-dim);
    font-style: italic;
  }
  .by {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    margin: 0.15rem 0 0;
  }
  .rail {
    height: 2px;
    background: var(--line);
    border-radius: 1px;
    margin: 0.6rem 0 0.4rem;
    overflow: hidden;
  }
  .fill {
    display: block;
    height: 100%;
    background: var(--accent);
  }
  .where {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    margin: 0.25rem 0 0;
  }
  /*
   * The doors, as a row that closes the body.
   *
   * Left-aligned under the metadata rather than floated to the container's far
   * edge, so the body reads as one column of decreasing weight — title, author,
   * progress, where you can take it. With the quotation gone the two columns now
   * end within a few pixels of each other, which is what makes the row look like
   * part of the card rather than a tray bolted under it.
   */
  .doors {
    display: flex;
    gap: var(--s-1);
    margin: var(--s-2) 0 0;
    /* The glyphs carry their own padding, so pull the first one back to the
       text's left edge — otherwise the row is visibly indented from everything
       above it. */
    margin-left: -0.34rem;
  }
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    /* A comfortable target around a 16px glyph. `padding` rather than a fixed
       box, so the hit area grows with the glyph if it ever does. */
    padding: 0.34rem;
    border-radius: var(--radius);
    color: var(--ink-dim);
  }
  .icon svg {
    display: block;
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.35;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  /* `.act`'s own hover is `--ink`; the accent is the app saying *this one is
     live under your cursor*, and one preview can only be hovered at a time, so
     it does not spend the surface's fill budget. `--bg-raised` measures Lc 0.0
     against `--bg` and is deliberately doing nothing on its own here — the
     colour change is what carries the state. */
  .icon:hover {
    color: var(--accent-text);
    background: var(--bg-raised);
  }
</style>
