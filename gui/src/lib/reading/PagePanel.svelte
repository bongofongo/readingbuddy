<script lang="ts">
  /**
   * Where you have got to.
   *
   * ## It writes the page and then reads the book back
   *
   * `update_progress` answers with the book, and this hands that answer up
   * rather than the number it sent. They are different values: the reader types
   * `214` and the engine decides whether that is `p. 214 of 1408 · 15%` or just
   * `p. 214`, depending on whether the book has an honest length. A panel that
   * echoed its own input would be right about the fixture and wrong about the
   * two books in the dev library whose `page_count` is zero or absent.
   *
   * ## It refuses before it sends, and the refusal names the move
   *
   * `parsePage` is not standing in for the engine's validation — the engine
   * takes an `i64` and will store whatever it is given. It is the frontend
   * declining to write a value the reader plainly did not mean, and every
   * refusal says what would work instead, which is this codebase's shape for
   * one.
   *
   * ## No slider, no percentage box, no *mark as finished*
   *
   * A page is the number a reader can actually read off the thing in their
   * hands. A percentage is the engine's derivation of it and is not an input.
   * Closing a read is a real act with real consequences on the wall of cards and
   * it belongs on the book's own page, not one keystroke from the surface you
   * leave open while reading.
   */
  import type { StoredBook } from '$lib/api/client';
  import { client } from '$lib/api/client';
  import { progressDetail } from '$lib/phrasing';

  import { parsePage } from './mode';

  let {
    book,
    onturned,
    oncancel,
  }: {
    book: StoredBook;
    /** What the engine now says, worded — the caller shows it and re-reads. */
    onturned: (said: string) => Promise<void>;
    oncancel: () => void;
  } = $props();

  /**
   * The box starts at the page the record already holds — **seeded, not
   * derived**, which is the book page's own distinction and the same reason.
   * The value is a starting point the reader is about to overwrite; deriving it
   * would put the record back into the box under a half-typed number every time
   * the book was re-read.
   *
   * A plain function rather than the expression inline, so that *seeds state* is
   * what the code says as well as what it means.
   */
  function seed(): string {
    return book.current_page === null ? '' : String(book.current_page);
  }

  let raw = $state(seed());
  let refusal = $state<string | null>(null);
  let sending = $state(false);

  async function send() {
    const parsed = parsePage(raw);
    if ('refusal' in parsed) {
      refusal = parsed.refusal;
      return;
    }
    refusal = null;
    sending = true;
    try {
      const fresh = await client().updateProgress(book.id, parsed.page);
      // `null` means the book is gone — deleted in another window while this was
      // open. Saying so beats a silent no-op, and the caller's re-read will find
      // the same thing and fall back to another open read.
      const said = fresh === null ? null : progressDetail(fresh.progress);
      await onturned(said === null ? `Page ${parsed.page}.` : `You are at ${said}.`);
    } catch (e) {
      refusal = `That did not go through: ${e instanceof Error ? e.message : String(e)}`;
    } finally {
      sending = false;
    }
  }
</script>

<form
  class="page"
  onsubmit={(e) => {
    e.preventDefault();
    void send();
  }}
>
  <h2 class="band-title">Where you are</h2>

  <div class="row">
    <!-- `inputmode` rather than `type="number"`: a number input brings spinners
         nobody wants on a page count and, on some engines, silently discards a
         value it dislikes — which would turn a refusal this panel can word into
         an empty box the reader has to guess about. -->
    <input
      bind:value={raw}
      aria-label="Page you are on"
      inputmode="numeric"
      autocomplete="off"
      placeholder="Page"
      disabled={sending}
    />
    <button type="submit" class="primary" disabled={sending}>Say so</button>
    <button type="button" onclick={oncancel} disabled={sending}>Leave it</button>
  </div>

  <!--
    The line under the box says the thing the surface above it could not.

    It used to say *The record says p. 500 of 1408 · 35%* whenever there was a
    record — which is the exact string already on screen three lines up, in the
    book row, in the accent. The working-state screenshot showed the two
    stacked. On a surface whose whole claim is that it shows you only what you
    need, saying one fact twice is the defect, not the redundancy being
    harmless.

    So: the refusal when there is one, and otherwise a line only in the case the
    row above renders nothing — a book with no page recorded, where the box would
    otherwise be an empty field with no explanation.
  -->
  {#if refusal}
    <p class="refusal" role="alert">{refusal}</p>
  {:else if progressDetail(book.progress) === null}
    <!-- Not framed as a gap: a book whose device never reported a page is an
         ordinary book. -->
    <p class="hint">Nothing about this book’s pages is recorded. Typing one starts that.</p>
  {/if}
</form>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 0.7rem;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }
  input {
    font: inherit;
    font-variant-numeric: tabular-nums;
    width: 7rem;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.4rem 0.55rem;
  }
  button {
    font: inherit;
    font-size: var(--t-fine);
    color: var(--ink-dim);
    background: none;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.4rem 0.8rem;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    color: var(--ink);
  }
  button:disabled {
    cursor: default;
    opacity: 0.6;
  }
  /* The primary act, in the accent — a thing you can do right now. */
  .primary {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-on);
  }
  .primary:hover:not(:disabled) {
    color: var(--accent-on);
  }
  .refusal {
    margin: 0;
    font-size: var(--t-fine);
  }
  .hint {
    margin: 0;
    color: var(--ink-dim);
    font-size: var(--t-fine);
  }
</style>
