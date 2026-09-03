<script lang="ts">
  /**
   * What is known about the book, and who said it.
   *
   * The reference half of the screen, below the reader's own half — because the
   * passages and the notes are what you came for and the ISBN is what you check.
   *
   * Two things here are honest about absence in a way that took the engine a
   * migration each to be able to be:
   *
   * - **Provenance** (item 29). An empty list means *nobody has claimed the
   *   field* — every book predating migration `0012` reports one however
   *   well-populated it is — so it renders as nothing at all rather than as
   *   "unknown provider", which would be a claim.
   * - **The chapter list** (item 32). `null` is *no file here we can read* and
   *   `entries: []` is *this file carries no TOC*. Two different sentences;
   *   collapsing them tells a reader the same thing about a missing file and an
   *   ordinary EPUB.
   *
   * The contents are loaded **when the disclosure opens**, not with the page:
   * the engine derives them from the file on every call and stores nothing, so
   * asking unprompted would open an EPUB every time anyone looked at a book.
   */
  import type { BookFileDto, BookTagDto, FieldSourceDto, TableOfContentsDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import { dayLabel, fieldLabel, fileSizeLabel, progressDetail, sourceLabel } from '$lib/phrasing';

  let {
    book,
    tags,
    files,
    provenance,
  }: {
    book: StoredBook;
    tags: BookTagDto[];
    files: BookFileDto[];
    provenance: FieldSourceDto[];
  } = $props();

  let contents = $state<TableOfContentsDto | null>(null);
  /**
   * Four states, not a boolean and a nullable.
   *
   * `contents === null` is a **real answer** — no readable file — so it cannot
   * also stand for "not asked yet" or "still reading". Collapsing them showed
   * *No readable file for this book* for the whole duration of every load,
   * which is the page telling the reader something false about their library
   * and then correcting itself.
   */
  let phase = $state<'idle' | 'reading' | 'ready' | 'failed'>('idle');

  async function loadContents() {
    if (phase !== 'idle') return;
    phase = 'reading';
    try {
      contents = await client().tableOfContents(book.id);
      phase = 'ready';
    } catch {
      // The chapter list is an ornament on a page that has already loaded. It
      // reports its own absence rather than replacing the book with an error,
      // which is item 26's ruling about the reading strip.
      phase = 'failed';
    }
  }
</script>

<section class="band">
  <h2 class="band-title">About</h2>

  <dl>
    {#if progressDetail(book.progress)}
      <!-- The long phrasing: this screen has room for the page a reader
           recognises. Every number in it was computed by the engine — a
           percentage needs a denominator, and one book in the dev library has a
           `page_count` of 0 while another has NULL. `Progress` collapses both to
           absence, so this says the page alone without needing to know why. -->
      <dt>Progress</dt>
      <dd>{progressDetail(book.progress)}</dd>
    {:else if book.page_count !== null}
      <dt>Pages</dt>
      <dd>{book.page_count}</dd>
    {/if}
    {#if book.series_label}
      <!-- The engine's label, not the pair reassembled here: `series_index` is a
           REAL and two frontends formatting it will eventually print `#2.5` two
           different ways. -->
      <dt>Series</dt>
      <dd>{book.series_label}</dd>
    {/if}
    {#if book.publisher}
      <dt>Publisher</dt>
      <dd>{book.publisher}{book.publish_year ? `, ${book.publish_year}` : ''}</dd>
    {:else if book.publish_year}
      <dt>Published</dt>
      <dd>{book.publish_year}</dd>
    {/if}
    {#if book.language}
      <dt>Language</dt>
      <dd>{book.language}</dd>
    {/if}
    {#if book.isbn_13 ?? book.isbn_10}
      <dt>ISBN</dt>
      <dd>{book.isbn_13 ?? book.isbn_10}</dd>
    {/if}
    {#if book.subjects.length > 0}
      <!-- What a provider says the book is *about*. Not `book_tags`, which are
           minted shelves and are shown separately below for that reason. -->
      <dt>Subjects</dt>
      <dd>{book.subjects.join(' · ')}</dd>
    {/if}
    {#if tags.length > 0}
      <dt>Shelves</dt>
      <dd>
        {#each tags as t (t.tag)}
          <!-- The origin's own spelling in the title, because the normalization
               is ours and the shelf name is theirs. -->
          <span class="chip" title={`${t.raw ?? t.tag} — ${sourceLabel(t.source)}`}>{t.tag}</span>
        {/each}
      </dd>
    {/if}
    {#if files.length > 0}
      <dt>Files</dt>
      <dd>
        {#each files as f (f.sha256)}
          <span class="file">{f.format.toUpperCase()} · {fileSizeLabel(f.size)}</span>
        {/each}
      </dd>
    {/if}
  </dl>

  {#if book.description}
    <p class="blurb">{book.description}</p>
  {/if}

  <details ontoggle={loadContents}>
    <summary>Contents</summary>
    {#if phase === 'failed'}
      <!-- Every command named in this app's empty states and failures is one
           that exists. `rb files` does not; `rb show` does. -->
      <p class="hint">
        The file could not be read just now. <code>rb show {book.id}</code> says what is owned.
      </p>
    {:else if phase !== 'ready'}
      <p class="hint">Reading the file…</p>
    {:else if contents === null}
      <!-- No file, which is not the same as a file with no contents. -->
      <p class="hint">
        No readable file for this book. <code>rb epub &lt;file&gt;</code> attaches one.
      </p>
    {:else if contents.entries.length === 0}
      <!-- A file, and it carries no table of contents. Ordinary. -->
      <p class="hint">This file carries no table of contents.</p>
    {:else}
      <ol class="toc">
        {#each contents.entries as e, i (`${i}-${e.target}`)}
          <li style:--depth={e.depth}>{e.label}</li>
        {/each}
      </ol>
    {/if}
  </details>

  {#if provenance.length > 0}
    <!-- Rendered only when something claimed something. An empty list is every
         book predating migration `0012`, and "unattributed" printed over a
         whole library would be noise about our own schema history. -->
    <details>
      <summary>Where this came from</summary>
      <ul class="prov">
        {#each provenance as p (p.field)}
          <li>
            <span class="field">{fieldLabel(p.field)}</span>
            <span class="src">{sourceLabel(p.source)}</span>
            <span class="when">{dayLabel(p.fetched_at)}</span>
          </li>
        {/each}
      </ul>
    </details>
  {/if}
</section>

<style>
  /* Each band owns its own spacing, here and in its siblings — deliberately not
     a `:global(.band)` rule from a route, which would be one screen's spacing
     leaking onto another's. */
  section.band {
    margin-top: 2.2rem;
  }
  .band-title {
    margin-bottom: 0.9rem;
  }
  dl {
    display: grid;
    grid-template-columns: max-content minmax(0, 1fr);
    gap: 0.25rem 1.1rem;
    margin: 0;
    max-width: var(--column);
    font-size: var(--t-fine);
  }
  dt {
    color: var(--ink-dim);
  }
  dd {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .chip,
  .file {
    display: inline-block;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0 0.4rem;
    margin: 0 0.3rem 0.25rem 0;
    font-size: var(--t-micro);
  }
  .blurb {
    max-width: var(--column);
    color: var(--ink-dim);
    margin: 1.1rem 0 0;
  }
  details {
    max-width: var(--column);
    margin-top: 1rem;
    border-top: 1px solid var(--line);
    padding-top: 0.6rem;
  }
  summary {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    cursor: pointer;
  }
  .toc {
    list-style: none;
    padding: 0;
    margin: 0.6rem 0 0;
    font-size: var(--t-fine);
  }
  .toc li {
    /* Depth is a column on the entry, exactly as the engine has it — a flat list
       with an indent, not a tree rebuilt here. */
    padding: 0.12rem 0 0.12rem calc(var(--depth) * 1.1rem);
    color: var(--ink);
  }
  .prov {
    list-style: none;
    padding: 0;
    margin: 0.6rem 0 0;
    font-size: var(--t-micro);
  }
  .prov li {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
    padding: 0.12rem 0;
  }
  .prov .field {
    min-width: 8rem;
    color: var(--ink-dim);
  }
  .prov .when {
    color: var(--ink-dim);
  }
</style>
