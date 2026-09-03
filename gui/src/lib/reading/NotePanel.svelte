<script lang="ts">
  /**
   * A note, written where you are.
   *
   * ## Why this is not `book/Composer.svelte`
   *
   * The composer on the book page mints a note about a *book*, from a desk where
   * you are already thinking about that book. This one mints a note about a
   * **moment in a read**, and it carries three anchors the composer has no way
   * to know: the reading, the page, and — through them — which of a reread's two
   * passes this thought belongs to.
   *
   * That is a different object, not a smaller one. `reading_id` is what puts the
   * note on the right card (`notes_for_reading` is scoped to the read and
   * deliberately does **not** fall back to the book's unanchored notes), and
   * `page` is what `noteAnchorLabel` draws as `p. 214` wherever the note is
   * shown afterwards. A shared component taking six optional props would make
   * both call sites read as configuration of one thing when they are two.
   *
   * ## No title field
   *
   * The composer has one because a note you sit down to write has a name you
   * mean. A thought you had at page 214 does not, and asking for one before the
   * thought is the surest way to lose it. `title: null` means the engine takes
   * the body's first six words, which is exactly right for this case — and is
   * why the field is absent rather than present-and-optional.
   *
   * ## Nothing is written until the button is pressed
   *
   * The composer's rule and it holds here: opening the panel and closing it
   * leaves nothing in the vault.
   */
  import type { ReadingDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';

  let {
    book,
    reading,
    onwritten,
  }: {
    book: StoredBook;
    reading: ReadingDto;
    onwritten: () => void;
  } = $props();

  let body = $state('');
  let failure = $state<string | null>(null);
  let sending = $state(false);

  async function write() {
    if (body.trim() === '') {
      failure = 'Write something first — even a line.';
      return;
    }
    failure = null;
    sending = true;
    try {
      await client().createNote({
        book_id: book.id,
        // The read, so the note lands on this pass rather than on the book at
        // large. A reread has two, and this route knows which one is open
        // because the engine told it — `currently_reading` returns the reading
        // beside the book precisely so nothing above the seam has to choose.
        reading_id: reading.id,
        highlight_id: null,
        // Where you were. `current_page` is the book's own column, written by
        // `update_progress` and by every device pull — so a note taken here is
        // anchored to the page the record already believes you are on, and not
        // to one this panel asked for again.
        page: book.current_page,
        location: null,
        kind: 'note',
        title: null,
        body,
      });
      body = '';
      onwritten();
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    } finally {
      sending = false;
    }
  }
</script>

<div class="note">
  <h2 class="band-title">A note, from here</h2>

  {#if failure}
    <p class="refusal" role="alert">{failure}</p>
    <p class="hint">
      Your notes are plain markdown in the vault regardless — <code>rb note</code> writes one from the
      terminal.
    </p>
  {/if}

  <!-- svelte-ignore a11y_autofocus -->
  <textarea
    bind:value={body}
    aria-label="Note"
    autofocus
    disabled={sending}
    placeholder={'What you are thinking. A [[wikilink]] makes an edge — even to a note that does not exist.'}
  ></textarea>

  <div class="bar">
    {#if book.current_page !== null}
      <!-- Stated rather than asked for. The reader is not filling in a form; they
           are being told what this note will remember about where they were. -->
      <p class="hint">It will remember p. {book.current_page}.</p>
    {/if}
    <button type="button" class="primary" onclick={write} disabled={sending}>Write it</button>
  </div>
</div>

<style>
  .note {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  textarea {
    font: inherit;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--t-fine);
    /* Under the 80-character cap SC 1.4.8 sets, in the one place `ch` is honest
       — see `--editor` in `app.css`. */
    max-width: var(--editor);
    min-height: 7.5rem;
    resize: vertical;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.6rem 0.7rem;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 1rem;
    max-width: var(--editor);
  }
  .bar .hint {
    margin: 0;
    font-size: var(--t-fine);
  }
  button {
    font: inherit;
    font-size: var(--t-fine);
    margin-left: auto;
    border-radius: var(--radius);
    padding: 0.4rem 0.9rem;
    cursor: pointer;
    background: var(--accent);
    border: 1px solid var(--accent);
    color: var(--accent-on);
  }
  button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  .refusal {
    margin: 0;
    font-size: var(--t-fine);
  }
  .hint {
    color: var(--ink-dim);
    font-size: var(--t-fine);
    margin: 0;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.9em;
  }
</style>
