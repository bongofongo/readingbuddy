<script lang="ts">
  /**
   * What this note connects to — its links, the note to link to next, and the
   * passages it quotes.
   *
   * ## It was the right rail, and it is now part of the editor
   *
   * The rail's own argument was that this is an **inspector**: the *Link to…*
   * search that writes `[[Title]]` at the cursor is an instrument acting on the
   * editor, not reference material beside it, and writing a note and finding the
   * note to link to is one operation.
   *
   * That argument was right and it argues for this, not for a column. A tool
   * that acts on one object belongs **with that object** — so it moved inside
   * `Editor`, where it is drawn under the note it is about, appears exactly when
   * a note is open, and disappears with it. The rail version had to be told
   * which note it was inspecting and had to be blanked when the centre was
   * showing something else; there is nothing to blank now, because there is no
   * inspector without something to inspect.
   *
   * Two things left when it moved, and neither was ever this note's:
   *
   * - **the search over the book's notes and passages** went to the passage
   *   list, which is the thing it searches;
   * - **the reads readout** was a second, quieter copy of the `Reads` place the
   *   selector already offers, and it went entirely.
   *
   * `oninsert` comes from the editor directly now rather than through the page.
   * The editor hands out a writer while it is mounted, so this cannot write into
   * a box that has gone, and the page no longer carries a callback between two
   * components that are now one.
   */
  import type { HighlightDto, NoteDto } from '$lib/api/bindings';
  import { client } from '$lib/api/client';

  import { linkPane, type LinkPane } from './links';
  import LinksPane from './LinksPane.svelte';
  import LinkTo from './LinkTo.svelte';

  let {
    note,
    cited,
    oninsert,
    onopennote,
    onshowpassage,
  }: {
    note: NoteDto;
    /** The passages this note quotes — resolved by the page, which has them all. */
    cited: HighlightDto[];
    oninsert: ((text: string) => void) | null;
    onopennote: (id: number) => void;
    onshowpassage: (id: number) => void;
  } = $props();

  let pane = $state<LinkPane | null>(null);

  // Loaded whenever the note changes, rather than when a button is pressed. Both
  // requests are the note's own edges: `Backlinks` is a `WHERE to_note = ?` and
  // `OutgoingLinks` reads what the note wrote, which is why the engine keeps
  // them apart and `links.ts` puts them in one list here.
  $effect(() => {
    const open = note;
    const api = client();
    let live = true;
    Promise.all([api.outgoingLinks(open.id), api.backlinks(open.id)])
      .then(([out, back]) => {
        if (live) pane = linkPane(out, back);
      })
      .catch(() => {
        if (live) pane = null;
      });
    return () => {
      live = false;
    };
  });
</script>

<section class="connections" aria-label="This note's connections">
  <div class="pane">
    <h3 class="band-title">Links</h3>
    {#if pane}
      <LinksPane {pane} onopen={(n) => onopennote(n.id)} />
    {:else}
      <p class="hint">Reading the graph…</p>
    {/if}
  </div>

  <div class="pane">
    <LinkTo exclude={note.id} {oninsert} />
  </div>

  <div class="pane">
    <h3 class="band-title">Cited passages</h3>
    {#if cited.length === 0}
      <!-- Not an empty list and not a prompt: it says where the gesture is,
           because citing happens *on the passage*, which is the surface that
           knows which one you mean. -->
      <p class="hint">Cite a passage from the passage list, and it appears here.</p>
    {:else}
      <ul class="cited">
        {#each cited as h (h.id)}
          <li>
            <button type="button" onclick={() => onshowpassage(h.id)}>
              {#if h.page !== null}<span class="where">p. {h.page}</span>{/if}
              <span class="text">{h.text}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  /*
   * Three panes across the editor's width, wrapping to one at a narrow window.
   *
   * A rule above them and nothing else: this is the note's own material sitting
   * under the note, and the separation it needs is the one that says *the
   * writing stops here*. `minmax(0, 1fr)` rather than `auto` so a long link
   * title wraps inside its pane instead of widening the track it is in.
   */
  .connections {
    margin-top: var(--s-6);
    padding-top: var(--s-4);
    border-top: 1px solid var(--line);
    max-width: var(--editor);
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(15rem, 100%), 1fr));
    gap: var(--s-5) var(--s-4);
    align-items: start;
  }
  .pane {
    min-width: 0;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .cited li + li {
    border-top: 1px solid var(--line);
  }
  .cited button {
    display: block;
    width: 100%;
    text-align: start;
    font: inherit;
    font-size: var(--t-micro);
    background: none;
    border: 0;
    padding: var(--s-2) 0;
    color: inherit;
    cursor: pointer;
  }
  .cited .where {
    display: block;
    color: var(--ink-dim);
  }
  .cited .text {
    font-style: italic;
    overflow-wrap: anywhere;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .cited button:hover .text {
    color: var(--accent-text);
  }
  .hint {
    margin: var(--s-2) 0 0;
    font-size: var(--t-micro);
  }
</style>
