<script lang="ts">
  /**
   * The right rail — the desk's inspector, and the layout's answer to
   * *"connecting notes with other notes"*.
   *
   * **The links pane stopped being a depth and became a region.** It used to be
   * somewhere you navigated *into* by pressing a button, which replaced the note
   * list in a single narrow column — the honest way to avoid a modal when there
   * were only two columns to spend. With a third there is nothing to trade off,
   * and a graph you can see while writing into it is a different tool from one
   * you have to go and look at.
   *
   * Its contents depend on what the centre is doing, which is what makes it an
   * inspector rather than a fourth thing to read. That also bounds the attention
   * cost: header, rail, centre, rail is four regions and convergent evidence puts
   * attended regions at about three plus periphery — the mitigation is that at
   * any moment you are attending the centre plus *one* rail, and this is the one.
   */
  import type { HighlightDto, NoteDto } from '$lib/api/bindings';
  import { client } from '$lib/api/client';

  import { linkPane, type LinkPane } from './links';
  import LinksPane from './LinksPane.svelte';
  import LinkTo from './LinkTo.svelte';
  import MarkSearch from './MarkSearch.svelte';

  let {
    bookId,
    note,
    cited,
    marks,
    reads,
    oninsert,
    onopennote,
    onshowpassage,
  }: {
    bookId: number;
    /** The note the centre has open, or `null` when it is showing the book. */
    note: NoteDto | null;
    /** The passages that note quotes — resolved by the page, which has them all. */
    cited: HighlightDto[];
    /** How many notes and passages this book has, so the box can not offer itself. */
    marks: number;
    /** The reading rows, as a two-line readout rather than a table. */
    reads: string[];
    oninsert: ((text: string) => void) | null;
    onopennote: (id: number) => void;
    onshowpassage: (id: number) => void;
  } = $props();

  let pane = $state<LinkPane | null>(null);

  // Loaded whenever a note is open, rather than when a button is pressed. Both
  // requests are the note's own edges: `Backlinks` is a `WHERE to_note = ?` and
  // `OutgoingLinks` reads what the note wrote, which is why the engine keeps
  // them apart and `links.ts` puts them in one list here.
  $effect(() => {
    const open = note;
    if (open === null) {
      pane = null;
      return;
    }
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

<aside class="rrail" aria-label={note ? 'This note' : 'This book'}>
  {#if note}
    <section>
      <h3 class="band-title">Links</h3>
      {#if pane}
        <LinksPane {pane} onopen={(n) => onopennote(n.id)} />
      {:else}
        <p class="hint">Reading the graph…</p>
      {/if}
    </section>

    <LinkTo exclude={note.id} {oninsert} />

    <section>
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
    </section>
  {:else}
    <!-- The search over this book's notes and passages, moved here from above
         the bands (item 50). It belongs to the column, and the column it belongs
         to is the one that answers about the centre. -->
    <MarkSearch {bookId} {marks} {onopennote} {onshowpassage} />

    {#if reads.length > 0}
      <section>
        <h3 class="band-title">Reads</h3>
        <ul class="reads">
          {#each reads as line, i (i)}
            <li>{line}</li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</aside>

<style>
  .rrail {
    min-width: 0;
  }
  .rrail :global(section + section),
  .rrail > :global(* + *) {
    margin-top: 1.7rem;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .cited li {
    border-bottom: 1px solid var(--line);
  }
  .cited li:last-child {
    border-bottom: 0;
  }
  .cited button {
    display: block;
    width: 100%;
    text-align: start;
    font: inherit;
    font-size: 0.8rem;
    background: none;
    border: 0;
    padding: 0.35rem 0.1rem;
    color: inherit;
    cursor: pointer;
  }
  .cited .where {
    display: block;
    font-size: 0.72rem;
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
  .reads {
    font-size: 0.8rem;
    color: var(--ink-dim);
  }
  .reads li {
    padding: 0.25rem 0;
  }
  .hint {
    margin: 0.4rem 0 0;
    font-size: 0.8rem;
  }
</style>
