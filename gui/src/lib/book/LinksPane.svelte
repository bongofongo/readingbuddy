<script lang="ts">
  /**
   * What one note says, and what says it.
   *
   * It **replaces the note it is about, in place** — no dialog, no overlay, no
   * second window. That is the axiom's *nothing is modal-by-default* doing real
   * work rather than being agreed with, and it is the TUI's own choice
   * (`app.links` swaps out the notes list rather than opening a layer over it).
   * The way back is always on screen.
   *
   * The direction is in the **text**, not in a colour: an arrow survives a
   * theme, a high-contrast mode and a colour-blind reader, and it is what a
   * test can assert on. Same argument as `book.rs`'s `link_line`.
   */
  import type { NoteDto } from '$lib/api/bindings';
  import { isForwardReference, type LinkPane } from './links';
  import { noteKindLabel } from '$lib/phrasing';

  let { pane, onopen }: { pane: LinkPane; onopen: (note: NoteDto) => void } = $props();
</script>

<p class="counts">
  <!-- Counts of edges that exist. Past tense, on a page you chose to open, and
       there is nothing here counting a link you have not written. -->
  {pane.outgoing} out · {pane.incoming} in
</p>

{#if pane.rows.length === 0}
  <p class="hint">
    Nothing links here. A <code>[[wikilink]]</code> in any note's body makes an edge — and it
    works before the other note exists.
  </p>
{:else}
  <ul>
    {#each pane.rows as row, i (`${row.dir}-${i}-${row.title}`)}
      <li>
        <span class="arrow" aria-hidden="true">{row.dir === 'out' ? '→' : '←'}</span>
        {#if row.note}
          <!-- Nothing is a dead end: an edge you can see is an edge you can follow. -->
          <button type="button" class="target" onclick={() => onopen(row.note!)}>
            {row.note.title}
          </button>
          {#if noteKindLabel(row.note.kind)}
            <span class="kind">{noteKindLabel(row.note.kind)}</span>
          {/if}
        {:else}
          <span class="target dangling">{row.title}</span>
        {/if}
        {#if isForwardReference(row)}
          <!-- Not an error and not a broken link. It resolves itself the day
               that note is written, which is the vault's best trick. -->
          <span class="pending">no note</span>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .counts {
    color: var(--ink-dim);
    font-size: 0.8rem;
    margin: 0 0 0.75rem;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--column);
  }
  li {
    display: flex;
    align-items: baseline;
    gap: 0.5rem;
    padding: 0.3rem 0;
    border-bottom: 1px solid var(--line);
  }
  li:last-child {
    border-bottom: 0;
  }
  .arrow {
    color: var(--ink-dim);
    font-size: 0.85rem;
    width: 1em;
    flex: none;
  }
  .target {
    font: inherit;
    text-align: start;
    background: none;
    border: 0;
    padding: 0;
    color: inherit;
    cursor: pointer;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  button.target:hover {
    color: var(--accent-text);
  }
  /* Text, because that is exactly what it still is. */
  .dangling {
    color: var(--ink-dim);
    cursor: default;
  }
  .kind,
  .pending {
    font-size: 0.72rem;
    color: var(--ink-dim);
    flex: none;
  }
</style>
