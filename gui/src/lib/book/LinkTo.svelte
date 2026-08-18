<script lang="ts">
  /**
   * Find a note anywhere in the vault and put a `[[wikilink]]` to it at the
   * cursor.
   *
   * **This is the most valuable single region in the redesign and it needed no
   * engine work**: `searchMarks(query, null, limit, 'note')` already takes a null
   * book id, so the vault-wide half has been buildable the whole time.
   *
   * It is an *instrument acting on the editor*, not reference material sitting
   * beside it — writing a note and finding the note to link to is one operation,
   * and that is what justifies a permanent column rather than a button that
   * opens something. A graph you can see while writing into it is a different
   * tool from a graph you have to go and look at.
   *
   * Scoped to `source: 'note'` **below the seam**, which is the difference
   * between this and a filter: the engine spends the whole `limit` on notes
   * rather than handing back a mixed list this file would have to thin out and
   * silently under-report.
   */
  import { client } from '$lib/api/client';
  import type { NoteDto } from '$lib/api/bindings';
  import { noteKindLabel } from '$lib/phrasing';

  let {
    exclude,
    oninsert,
  }: {
    /** The note being written. A note that links to itself is an edge to nowhere. */
    exclude: number;
    /** `null` while no editor is mounted — the box says so rather than lying. */
    oninsert: ((text: string) => void) | null;
  } = $props();

  /** A ceiling on a rail this narrow, not a page size: there is no cursor. */
  const LIMIT = 12;

  let query = $state('');
  let hits = $state<NoteDto[]>([]);

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
    if (q.trim() === '') {
      hits = [];
      answered = '';
      return;
    }
    const handle = setTimeout(() => {
      client()
        .searchMarks(q, null, LIMIT, 'note')
        .then((found) => {
          if (seq !== asked) return;
          hits = found.flatMap((h) => (h.kind === 'note' && h.note.id !== exclude ? [h.note] : []));
          answered = q;
        })
        .catch(() => {
          if (seq !== asked) return;
          hits = [];
          answered = q;
        });
      // One search per pause rather than one per keystroke.
    }, 140);
    return () => clearTimeout(handle);
  });

  function link(note: NoteDto) {
    // The title, because the title is what a `[[wikilink]]` resolves on — the id
    // is this app's and the vault's edges are readable in Obsidian.
    oninsert?.(`[[${note.title}]]`);
  }
</script>

<section>
  <h3 class="band-title">Link to…</h3>
  <input
    type="search"
    bind:value={query}
    placeholder="any note in the vault"
    aria-label="Find a note to link to"
  />

  {#if oninsert === null}
    <p class="hint">Open a note to write a link into it.</p>
  {:else if answered !== '' && hits.length === 0}
    <!-- No count, ever — not *12 results* and not *0 results*. A search that
         matched nothing says so in words, and names what a link still does. -->
    <p class="hint">
      Nothing matched. A <code>[[wikilink]]</code> works before the note exists, so it can be typed
      anyway.
    </p>
  {:else if hits.length > 0}
    <ul>
      {#each hits as n (n.id)}
        <li>
          <button type="button" onclick={() => link(n)}>
            <span class="title">{n.title}</span>
            {#if noteKindLabel(n.kind)}
              <span class="kind">{noteKindLabel(n.kind)}</span>
            {/if}
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  input {
    font: inherit;
    font-size: 0.82rem;
    width: 100%;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.3rem 0.5rem;
    margin: 0.5rem 0 0.4rem;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  button {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    width: 100%;
    text-align: start;
    font: inherit;
    font-size: 0.82rem;
    background: none;
    border: 0;
    padding: 0.3rem 0.1rem;
    color: inherit;
    cursor: pointer;
  }
  button:hover .title {
    color: var(--accent-text);
  }
  .title {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .kind {
    font-size: 0.66rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-dim);
    flex: none;
    margin-left: auto;
  }
  .hint {
    margin: 0;
    font-size: 0.8rem;
  }
</style>
