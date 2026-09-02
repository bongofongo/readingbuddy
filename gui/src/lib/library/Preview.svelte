<script lang="ts">
  /**
   * One book you have open, previewed — the only place on the home surface that
   * carries more than a jacket.
   *
   * **The latest mark is the whole point of it.** "More of a preview" means the
   * reader's own material, because that is the only content that earns the
   * space: a publisher's blurb would not, and a progress bar on its own is a
   * status readout rather than a reason to look. What is here is the last thing
   * you kept or wrote, quoted, with the app saying which of the two it is.
   *
   * ## The number rule, restated because this band carries the most digits
   *
   * `p. 214 of 502 · 43%` describes **one book you chose to open**. It never
   * describes the shelf. There is no "3 books in progress", no total, and no
   * aggregate anywhere on this page — the band is capped at four and cut
   * silently for exactly that reason (`latest.ts`).
   *
   * ## Three absences, three different renderings
   *
   * - no highlights and no notes → the mark block is **omitted**, not replaced
   *   with "nothing yet". An open book you have not written against is not an
   *   omission.
   * - no page count → the rail is omitted and the line degrades to
   *   `p. 214 · started …`. Never `p. 214 of 0`: the engine already collapses a
   *   zero length to absence, and a bar must not draw an empty track over one.
   * - no title → `titleLabel`'s word, dimmed and italic, the same absence in the
   *   same voice as the tile and the book page.
   */
  import type { ReadingDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { bookHref } from '$lib/nav';
  import { authorsLabel, dayLabel, progressDetail, titleLabel } from '$lib/phrasing';

  import type { Mark } from './latest';

  let {
    book,
    reading,
    mark,
  }: { book: StoredBook; reading: ReadingDto; mark: Mark | null } = $props();

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

    {#if mark}
      <div class="mark">
        <!-- Which of the two it is, said in a word. A passage is the device's
             capture and a note is yours; unlabelled they are two grey blocks,
             and this band exists to show the difference. -->
        <span class="band-title">{mark.kind === 'passage' ? 'Latest passage' : 'Latest note'}</span>
        {#if mark.kind === 'passage'}
          <blockquote>{mark.text}</blockquote>
        {:else}
          <p class="note-title">{mark.title}</p>
        {/if}
      </div>
    {/if}

    <!--
      One door, and it used to be two.

      The jacket and the title already lead into reading mode, so a preview is a
      link before it is anything else. It also carried *Write* (`?compose=1`) and
      *The book* — which is three destinations per preview, and the entrance
      draws one preview per open reading. On a real library that was **eighteen
      controls on the calmest surface in the app**, over a band whose whole brief
      is *less happening and more whitespace*.

      *Write* is the one that went. It is not a lesser act — it is one click
      further along a path this link already starts, and the book page puts it in
      the row at the top. Nothing that was reachable stopped being reachable;
      what went is the app asking, nine times down the page, whether you would
      like to write something.

      `?compose=1` still works and is still a view state in a URL rather than a
      write, which is what a moment's link relies on.
    -->
    <a class="act" href={`/book/${book.id}`}>The book</a>
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
  .mark {
    margin-top: 0.7rem;
  }
  .mark .band-title {
    display: block;
    font-size: var(--t-micro);
    margin-bottom: 0.2rem;
  }
  blockquote,
  .note-title {
    margin: 0;
    font-size: var(--t-fine);
    line-height: 1.5;
    font-style: italic;
    overflow-wrap: anywhere;
    /* Two lines, then the sentence stops. A preview that grows with the passage
       would make one long highlight the tallest thing on the home surface. */
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  blockquote::before {
    content: '“';
  }
  blockquote::after {
    content: '”';
  }

  /* `app.css`'s quiet control, as a link: only the placement is this file's. */
  .act {
    display: inline-block;
    margin-top: var(--s-2);
  }
  .act:hover {
    color: var(--accent-text);
  }
</style>
