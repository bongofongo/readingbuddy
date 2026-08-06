<script lang="ts">
  /**
   * The moment — transient, and it ends in a move.
   *
   * ## It is a band in the page, and that is the whole design
   *
   * Not a dialog, not a toast, not an overlay. `gui-vision.md:121`: *"a moment
   * that ended in a dismissable dialog would be a task-completion popup wearing
   * a costume. A moment that ends with a cursor in an empty reflection is the
   * app doing what it exists for."* So there is no close button anywhere in this
   * file — the moment ends by being acted on, or by being read and left alone,
   * and either way it does not come back. Item 27 refused a modal on a *page*
   * for the smaller version of this reason; a ceremony is where it would have
   * been most tempting.
   *
   * ## Polled, and only ever one at a time
   *
   * There is no push channel — every reply carries the id it answers — so this
   * asks, on mount. It asks for **one**. That is not a page size: a second
   * moment queued behind the first would make this an inbox with a ceremony's
   * manners, and `docs/decisions.md` bans an inbox by name. One moment, or none.
   *
   * Nothing here renders `moments.length`, and there is nothing on the wire to
   * render — `the_wire_states_no_number_of_moments` asserts that absence in the
   * engine deliberately, and a `3` beside a ceremony here would put the badge
   * back one layer up.
   *
   * It lives on the **shelf**, which is where the app greets you and where the
   * wall of finished books will eventually be. That also gives "poll after a
   * write that could mint one" for free and without a cross-route store: the
   * writes are on the book view, and coming back to the shelf remounts this and
   * asks again.
   *
   * ## Acknowledged when shown, held until you leave
   *
   * `surfaced_at` means *shown*, so it is written when this renders rather than
   * when it is clicked — a moment you read and walked away from has fired. The
   * DTO is then held in local state and this does not poll again, so it cannot
   * vanish out from under the sentence you are reading. Acknowledging is
   * idempotent through both doors, so a double render costs nothing.
   */
  import { goto } from '$app/navigation';
  import type { MomentDto } from '$lib/api/bindings';
  import { client, type StoredBook } from '$lib/api/client';
  import Jacket from '$lib/components/Jacket.svelte';
  import { titleLabel } from '$lib/phrasing';
  import { momentSentence, type MomentMove } from './sentence';

  let moment = $state<MomentDto | null>(null);
  /** Titles for the books a sentence names. Missing reads as *a book*, never blank. */
  let titles = $state<Record<number, string>>({});
  /** The book the moment is *of* — the one whose jacket it shows. Absent for a run. */
  let subject = $state<StoredBook | null>(null);
  let going = $state(false);

  const sentence = $derived(
    moment === null ? null : momentSentence(moment, (id) => titles[id] ?? null),
  );

  $effect(() => {
    const api = client();
    // One. See the module doc — this is the line that keeps it a ceremony.
    api
      .pendingMoments(1)
      .then(async ([m]) => {
        if (m === undefined) return;
        moment = m;
        // Shown is acknowledged. Failing to record it is not worth taking the
        // moment away from the reader, so the ceremony stands either way.
        api.acknowledgeMoment(m.id).catch(() => {});
        await loadTitles(m);
      })
      // A moment nobody could fetch is simply no moment. It must never be able
      // to replace the shelf underneath it — item 26 made that call for the
      // reading strip and an ornament above the library is the same call.
      .catch(() => {});
  });

  async function loadTitles(m: MomentDto) {
    const api = client();
    const wanted = new Set<number>();
    if (m.book_id !== null) wanted.add(m.book_id);
    if (m.kind.kind === 'reflection_reached') wanted.add(m.kind.reached_book_id);
    const found = await Promise.all([...wanted].map((id) => api.getBook(id).catch(() => null)));
    const next: Record<number, string> = {};
    for (const b of found) if (b !== null) next[b.id] = titleLabel(b.title);
    titles = next;
    subject = found.find((b) => b !== null && b.id === m.book_id) ?? null;
  }

  /**
   * The move, taken.
   *
   * `reflect` opens (or mints) the reflection and then lands on the book with
   * that note **already open** — which is what "a cursor in an empty reflection"
   * has to mean in a frontend that already has a note editor. Building a second
   * editor for the ceremony would be two places to fix the day the first one
   * changes, and item 27's pane is the one that knows about links, citations and
   * saving.
   *
   * `openReflection` answers a `CreatedNoteDto` rather than a `NoteDto`, so the
   * id is all this needs — the book view fetches the record. That is entry 27's
   * recorded gap in `crates/api` being paid for a second time, which is worth
   * saying out loud rather than working around here.
   */
  async function go(move: MomentMove) {
    if (going) return;
    going = true;
    try {
      if (move.move === 'life') {
        await goto('/life');
        return;
      }
      if (move.move === 'note') {
        await goto(`/book/${move.bookId}?note=${move.noteId}`);
        return;
      }
      // The engine's own `reading_id`, relayed. Not a guess — see `client.ts`.
      const created = await client().openReflection(move.bookId, move.readingId);
      await goto(`/book/${move.bookId}?note=${created.id}`);
    } catch {
      // The reflection did not open. The book still exists and is still the
      // place to write, so this goes there rather than becoming a dead end with
      // an error in it.
      if (move.move !== 'life') await goto(`/book/${move.bookId}`);
    } finally {
      going = false;
    }
  }
</script>

{#if moment && sentence}
  <!-- `aria-live` off: this is not a status update and a screen reader
       interrupting a page to announce a ceremony is the popup behaviour in
       another medium. It is a region you arrive at by reading the page. -->
  <section class="moment" aria-labelledby="moment-said">
    {#if subject}
      <!-- The jacket, which the band did not have and needed most.
           Every other reference to a book on this surface is a tile with its
           own jacket and accent; a ceremony *about a book* that showed no book
           was the single thing making this read as a banner rather than as a
           moment. Small, and the same three states `Jacket` gives everywhere. -->
      <a class="art" href={`/book/${subject.id}`} aria-hidden="true" tabindex="-1">
        <Jacket src={client().coverSrc(subject)} accent={subject.cover_accent} />
      </a>
    {/if}
    <p class="said" id="moment-said">{sentence.said}</p>
    <button type="button" onclick={() => go(sentence.move)} disabled={going}>
      {sentence.move.invitation}
    </button>
  </section>
{/if}

<style>
  /*
   * Its own ground, like the Reading strip beneath it, and for the same finding:
   * size alone does not carry hierarchy. This is quieter than that strip rather
   * than louder — the ceremony is a sentence, and a sentence set in a coloured
   * slab reads as an advertisement for itself.
   */
  .moment {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1.4rem;
    padding: 0.85rem 1rem;
    /*
     * Capped rather than spanning the window, which the screenshot settled.
     * Full width put the invitation nine hundred pixels from the sentence it
     * belongs to, and a coloured bar across the top of a page with a button in
     * its far corner is the shape of a promotional banner. A moment is a note to
     * you; keeping the two halves within reading distance of each other is what
     * makes it read as one.
     */
    max-width: calc(var(--measure) + 12rem);
    background: var(--bg-raised);
    border-radius: var(--radius);
    /* One accent edge. The moment is the only thing on the shelf that is about
       a change rather than about what is there, and this is the whole of how it
       says so. */
    border-left: 3px solid var(--accent);
  }
  .art {
    flex: none;
    width: 38px;
    aspect-ratio: 2 / 3;
    border-radius: 2px;
    overflow: hidden;
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--ink) 14%, transparent);
  }
  .said {
    margin: 0;
    max-width: var(--measure);
    /* Slightly larger than body copy — it is one sentence and it is the point
       of the band. Not a heading: it is prose, and headings name places. */
    font-size: 1.02rem;
    line-height: 1.4;
    /* A 220-character title is in the fixture on purpose and this sentence
       interpolates one inline, unbounded. It wraps rather than pushing the
       invitation off the band. */
    overflow-wrap: anywhere;
    flex: 1 1 16rem;
  }
  /*
   * A ghost control, not a filled pill.
   *
   * Filled, this was the **only** solid colour block above the fold on the home
   * surface — louder than any book on a screen whose subject is books, and the
   * exact shape of a cookie or newsletter bar. The moment's payload is the
   * invitation, so it stays a real control and stays accent-coloured; what it
   * gives up is being the loudest thing on the shelf. `--accent-text` rather
   * than `--accent` because it is now text on the page's own ground, which is
   * the rule `app.css` states for exactly this case.
   */
  button {
    flex: none;
    font: inherit;
    font-size: 0.85rem;
    padding: 0.3rem 0.75rem;
    border: 1px solid color-mix(in srgb, var(--accent) 55%, transparent);
    border-radius: var(--radius);
    background: transparent;
    color: var(--accent-text);
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  button:disabled {
    cursor: default;
    opacity: 0.7;
  }
</style>
