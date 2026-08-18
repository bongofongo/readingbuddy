<script lang="ts">
  /**
   * A new note, before it exists.
   *
   * Its own component rather than a fourth branch inside the editor, because the
   * two are different objects: this one has a title you *can* set (the create
   * request takes one; nothing renames afterwards — see `Editor`), no body on
   * disk, no rating, and no links to show. The only thing they share is a
   * textarea, and sharing a textarea is not a reason to share a component.
   *
   * **Nothing is written until the button is pressed.** The library's *Write*
   * link and the rail's *Note* button both land here rather than minting a row,
   * so navigation never leaves anything in the vault.
   */
  import { client } from '$lib/api/client';

  let {
    bookId,
    onwritten,
    oncancel,
  }: {
    bookId: number;
    /** The note that now exists, so the page can open it. */
    onwritten: (id: number) => Promise<void>;
    oncancel: () => void;
  } = $props();

  let title = $state('');
  let body = $state('');
  let failure = $state<string | null>(null);

  async function write() {
    try {
      const created = await client().createNote({
        book_id: bookId,
        reading_id: null,
        highlight_id: null,
        page: null,
        location: null,
        kind: 'note',
        // Empty means the engine takes the body's first six words, which is what
        // the field's own placeholder promises. Not the empty string: that is a
        // title, and it is blank.
        title: title.trim() === '' ? null : title.trim(),
        body,
      });
      await onwritten(created.id);
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="composer">
  <h2>A new note</h2>

  {#if failure}
    <p class="note">That did not go through: {failure}</p>
    <p class="hint">
      Your notes are plain markdown in the vault regardless — <code>rb note</code> writes one from the
      terminal.
    </p>
  {/if}

  <input
    bind:value={title}
    aria-label="Note title"
    placeholder="Title — or leave it, and the first few words become one"
  />
  <textarea
    bind:value={body}
    aria-label="Note body"
    placeholder={'Write. A [[wikilink]] makes an edge — even to a note that does not exist.'}
  ></textarea>
  <div class="bar">
    <button type="button" onclick={oncancel}>Cancel</button>
    <button type="button" class="primary" onclick={write}>Write it</button>
  </div>
</div>

<style>
  .composer {
    max-width: var(--editor);
  }
  h2 {
    font-size: 1.05rem;
    margin-bottom: 0.7rem;
  }
  input,
  textarea {
    font: inherit;
    width: 100%;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.6rem 0.7rem;
    margin-bottom: 0.6rem;
  }
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 1rem;
    line-height: 1.75;
    min-height: 18rem;
    padding: 0.9rem 1rem;
    resize: vertical;
  }
  .bar {
    display: flex;
    gap: 0.5rem;
  }
  button {
    font: inherit;
    font-size: 0.78rem;
    line-height: 1.4;
    color: var(--ink-dim);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.25rem 0.75rem;
    cursor: pointer;
  }
  button:hover {
    color: var(--ink);
  }
  button.primary {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
    font-weight: 600;
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
