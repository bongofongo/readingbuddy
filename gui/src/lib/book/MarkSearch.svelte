<script lang="ts">
  /**
   * Search this book — its notes and its passages, in one list (item 50).
   *
   * ## Why this is not in the Notes band, which is where it was specified
   *
   * `gui/CLAUDE.md` said the input belonged beside the *Notes* heading, and
   * that was written when the plan was to search notes. It is the wrong place
   * for what item 40 actually built: `SearchMarks` returns **one ranked list
   * over both indexes**, and the two things it ranks are drawn by two different
   * bands below. A box inside one band answering about the other is a heading
   * that lies; a box scoped to `source: 'note'` to make the heading true would
   * throw away the passages, which are the larger half of what a reader keeps
   * and the half a phrase is most likely to be in.
   *
   * So it sits above both bands, belonging to the column rather than to a band,
   * and every hit is a **move into the page that is already open** — a note hit
   * opens the note pane, a passage hit takes you to that passage in the
   * Passages band. It adds no fourth place to read something.
   *
   * ## What it is careful about
   *
   * **The order is the engine's.** Notes and passages arrive interleaved by
   * within-source rank and nothing here re-sorts, re-groups or splits them into
   * two sections; splitting would spend one `limit` across two lists whose
   * lengths then vary with the query, and it would be this file inventing the
   * ranking the single method exists to make unnecessary.
   *
   * **No count, ever.** Not *12 results* and not *0 results*. A search that
   * matched nothing says so in words and names the move — the box is a place to
   * look, not a score.
   *
   * **The snippet is text, not markup.** `$lib/book/snippet` un-mixes the
   * `>>`/`<<` the engine wrote from the reader's own prose, which arrives
   * unescaped; Svelte escapes what this renders, and `{@html}` appears nowhere.
   */
  import type { SearchHitDto } from '$lib/api/bindings';
  import { client } from '$lib/api/client';
  import { snippetSegments } from './snippet';

  let {
    bookId,
    marks,
    onopennote,
    onshowpassage,
  }: {
    bookId: number;
    /**
     * How many notes and passages this book has — the box draws nothing at `0`.
     *
     * A review found the search offered on a book with an empty Notes band and
     * an empty Highlights band under it: a full-width control above two honest
     * empty states, whose only possible reply is *nothing matches*. Those bands
     * already name the moves that fill them, and a third element that can only
     * refuse is not *idle is not blank* — it is a dead end wearing an input.
     * The box arrives with the first mark, which is also when it starts being
     * able to answer.
     */
    marks: number;
    /** A note hit reuses the pane the page already has, at its list depth. */
    onopennote: (noteId: number) => void;
    /** A passage hit takes the reader to it in the band below. */
    onshowpassage: (highlightId: number) => void;
  } = $props();

  /**
   * A ceiling that a book cannot realistically reach, not a page size.
   *
   * There is no offset and no cursor on this request — *more* is the same
   * search with a bigger number — so a low limit here would be a silent
   * truncation with nothing on screen to say so. A book's whole mark set is
   * bounded by what one person kept, and 50 puts the cut out of reach of
   * every real one while still bounding the pathological import.
   *
   * `limit` is **required** by the client method for the reason it is stated
   * here: `0` is an empty list rather than *no limit*, so a forgotten default
   * would be a search box that never finds anything.
   */
  const LIMIT = 50;

  let query = $state('');
  let hits = $state<SearchHitDto[]>([]);
  let failure = $state<string | null>(null);

  /**
   * Which query the list belongs to.
   *
   * A plain counter and not `$state`: it exists so a slow reply for `gr` cannot
   * land on top of the list for `grief`, and an effect reading it as a
   * dependency would re-run itself on every write. Nothing renders it.
   */
  let asked = 0;

  /** The query the shown list answers, so *nothing matched* can quote it. */
  let answered = $state('');

  $effect(() => {
    const q = query;
    const seq = ++asked;
    // The empty query is **not a search**: the engine issues no statement for
    // it and answers with nothing, so this needs no guard against blankness —
    // but it does need to stop drawing the last list, since an empty box with
    // hits under it is a screen answering a question nobody is asking.
    if (q.trim() === '') {
      hits = [];
      answered = '';
      failure = null;
      return;
    }
    const handle = setTimeout(() => {
      client()
        .searchMarks(q, bookId, LIMIT)
        .then((found) => {
          if (seq !== asked) return;
          hits = found;
          answered = q;
          failure = null;
        })
        .catch((e) => {
          if (seq !== asked) return;
          hits = [];
          answered = q;
          failure = e instanceof Error ? e.message : String(e);
        });
      // One search per pause rather than one per keystroke. A number small
      // enough that a reader who stops typing sees the answer as arriving, and
      // large enough that a typed word is one round trip and not six.
    }, 140);
    return () => clearTimeout(handle);
  });

  /** Chapter and page, the same line the passage carries in its own band. */
  function where(chapter: string | null, page: number | null): string | null {
    const parts = [chapter, page !== null ? `p. ${page}` : null].filter(Boolean);
    return parts.length === 0 ? null : parts.join(' · ');
  }

  /**
   * A note hit's title, unless the snippet already *is* the title.
   *
   * `snippet(…, -1, …)` picks whichever indexed column matched, and
   * `notes_fts` indexes the title beside the body — so a query matching a title
   * comes back with the title as its snippet, and printing the title under it
   * draws the same words twice, once marked and once not. The wire does not say
   * which column matched (recorded as an engine gap), so this compares the
   * text: same words, one line.
   */
  function noteWhence(title: string, snippet: string): string | null {
    const flat = snippetSegments(snippet)
      .map((s) => s.text)
      .join('');
    return flat.trim() === title.trim() ? null : title;
  }
</script>

{#if marks > 0}
  <section class="search">
    <label>
      <!-- **The scope is in the label, not in the placeholder.** It was in the
         placeholder, and at phone width that string clipped mid-word — with the
         clipped half being the half that said *your own notes* — so the reader
         at the narrowest viewport was the one never told what was searched.
         A placeholder also disappears the moment the box is used, which is the
         moment the question matters most. -->
      <span class="what">Search this book — passages and notes</span>
      <input
        bind:value={query}
        type="search"
        autocomplete="off"
        spellcheck="false"
        placeholder="A word you remember"
      />
    </label>

    {#if failure}
      <!-- Says what was refused and names what still answers the same question. -->
      <p class="hint">
        That search did not go through: {failure}. <code>rb find</code> asks the same index from the terminal.
      </p>
    {:else if answered !== '' && hits.length === 0}
      <!-- No number, and no apology. The two things a reader can do next, in the
         order they would try them. -->
      <p class="hint">
        Nothing here matches “{answered}”. The words searched are your passages and your notes for
        this book — <code>rb find</code> asks the whole library.
      </p>
    {:else if hits.length > 0}
      <!-- The engine's order, drawn straight. No grouping by kind: the rank is
         the answer and two sections would be a second ranking. -->
      <ul>
        {#each hits as hit (hit.kind === 'note' ? `n${hit.note.id}` : `h${hit.highlight.id}`)}
          <li>
            <button
              type="button"
              onclick={() =>
                hit.kind === 'note' ? onopennote(hit.note.id) : onshowpassage(hit.highlight.id)}
            >
              <span class="kind">{hit.kind === 'note' ? 'Note' : 'Passage'}</span>
              <span class="snippet">
                {#each snippetSegments(hit.snippet) as seg, i (i)}{#if seg.match}<mark
                      >{seg.text}</mark
                    >{:else}{seg.text}{/if}{/each}
              </span>
              {#if hit.kind === 'note' && noteWhence(hit.note.title, hit.snippet)}
                <span class="whence">{noteWhence(hit.note.title, hit.snippet)}</span>
              {:else if hit.kind === 'highlight' && where(hit.highlight.chapter, hit.highlight.page)}
                <span class="whence">{where(hit.highlight.chapter, hit.highlight.page)}</span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  .search {
    margin-top: 1.6rem;
    max-width: var(--measure);
  }
  label {
    display: block;
  }
  /* The label is a label and not a placeholder: a placeholder disappears the
     moment the box is used, which is the moment the reader most wants to know
     what they are searching. */
  .what {
    display: block;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    color: var(--ink-dim);
    margin-bottom: 0.3rem;
  }
  input {
    font: inherit;
    font-size: 0.92rem;
    width: 100%;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.45rem 0.7rem;
  }
  /* Stated rather than inherited: WebKit's default placeholder is `#a9a9a9`,
     which measures **2.35:1** on this field — the least legible text in the app,
     and the one the browser picks when nothing here has an opinion. `--ink-dim`
     is 5.29:1 and is the token every other secondary line uses. */
  input::placeholder {
    color: var(--ink-dim);
    opacity: 1;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0.7rem 0 0;
  }
  li {
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  button {
    display: grid;
    grid-template-columns: 4.6rem minmax(0, 1fr);
    gap: 0.1rem 0.6rem;
    width: 100%;
    text-align: start;
    font: inherit;
    font-size: 0.88rem;
    background: none;
    border: 0;
    padding: 0.5rem 0.1rem;
    color: inherit;
    cursor: pointer;
  }
  button:hover .snippet {
    color: var(--accent-text);
  }
  .kind {
    grid-row: 1 / span 2;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ink-dim);
    padding-top: 0.1rem;
  }
  .snippet {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  /* The matched term, and it has to survive both themes — so it is drawn as
     weight and a tinted ground rather than as a colour on its own. */
  mark {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: inherit;
    font-weight: 600;
    border-radius: 2px;
  }
  .whence {
    grid-column: 2;
    font-size: 0.72rem;
    color: var(--ink-dim);
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .hint {
    margin: 0.6rem 0 0;
  }
</style>
