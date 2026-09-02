<script lang="ts">
  /**
   * What you wrote against this book — the note list, as a work surface.
   *
   * ## It was the left rail's middle section
   *
   * The rail argued that this list was the part of it that **changed**, and that
   * the changing part is what keeps a persistent rail from habituating into
   * wallpaper: the failure mode of a permanent column is not clutter, it is that
   * the one time it matters it is invisible. That was a good argument for keeping
   * it in the rail *given* a rail. It is also the reason it is the section that
   * survives the rail's removal intact — a list that is worth noticing is worth a
   * surface, and here it gets the width to show a title without truncating it.
   *
   * ## One list, never four tabs
   *
   * The TUI's ruling and it stays right: a tab holding the single reflection is
   * the wrong shape for a thing there is exactly one of. The row says which kind
   * it is instead, as a **left prefix in a fixed gutter** so the titles align
   * whether or not their row has a kind — the card's arrangement, and the same
   * one for the same reason.
   *
   * No count, and no heading over the list. The selector above already says
   * *Notes*, and a number beside it would be the one thing this page is for
   * turned into a tally.
   */
  import type { NoteDto } from '$lib/api/bindings';
  import { noteKindLabel } from '$lib/phrasing';

  let {
    notes,
    openNoteId,
    onopen,
  }: {
    notes: NoteDto[];
    /** Marked in the list, so returning from the editor lands on the row you left. */
    openNoteId: number | null;
    onopen: (id: number) => void;
  } = $props();
</script>

{#if notes.length === 0}
  <!-- Idle is not blank, and this is not an apology or a count of zero. The move
       that fills it is the *Write* button in the row above, which is on screen
       while this is. -->
  <p class="hint">Nothing written against this book. <em>Write</em> starts a note.</p>
{:else}
  <ul>
    {#each notes as n (n.id)}
      <li>
        <button
          type="button"
          class:on={n.id === openNoteId}
          aria-current={n.id === openNoteId ? 'true' : undefined}
          onclick={() => onopen(n.id)}
        >
          <span class="kind">{noteKindLabel(n.kind) ?? ''}</span>
          <span class="title">{n.title}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--column);
  }
  li + li {
    border-top: 1px solid var(--line);
  }
  button {
    display: flex;
    gap: var(--s-3);
    align-items: baseline;
    width: 100%;
    text-align: start;
    font: inherit;
    background: none;
    border: 0;
    padding: var(--s-2) 0;
    color: inherit;
    cursor: pointer;
  }
  button:hover .title {
    color: var(--accent-text);
  }
  /* The note you have open, marked where you left it. `--ink` and weight rather
     than a fill: the one accent fill a surface is allowed is spent on the action,
     and this is a position rather than something to act on. */
  button.on .title {
    font-weight: 600;
  }
  /* A gutter, so the titles align whether or not their row has a kind. Labelling
     every plain note would be a column of the same word. */
  .kind {
    flex: 0 0 5rem;
    text-align: right;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  .title {
    overflow-wrap: anywhere;
  }
</style>
