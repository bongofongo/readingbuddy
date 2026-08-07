<script lang="ts">
  /**
   * The passages you kept, and the two different things written against them.
   *
   * ## Two notes, two owners, and the labels are load-bearing
   *
   * `ko_note` is **KOReader's** and is rewritten toward the device on every
   * import; `annotation` is **the reader's** and no import has ever touched it.
   * That split is the whole of `docs/decisions.md`'s highlight-ownership
   * section, and until this screen nothing had ever drawn the two side by side
   * — so the one place a reader could learn that their own words survive a
   * device refresh, and the device's do not, is here. Unlabelled they are two
   * grey paragraphs.
   *
   * ## Citing, which is the gesture a mouse makes available
   *
   * `Cite`/`Uncite`/`CitationsFor` have existed since item 7 and `rb cite`
   * is their only surface. Citing from the passage in front of you is what a
   * pointer and a book view can do that a terminal list could not, so the
   * control lives on the passage rather than in a form somewhere else.
   *
   * It needs a note to cite **into**, so it appears only when one is open.
   *
   * ## The quoted mark, which was the N+1 and is now one call (item 48)
   *
   * This file used to record a refusal here: marking which passages *some other*
   * note cites needed one `CitationsFor` per note in the book, an N+1 with no
   * request behind it, so it was deferred rather than built badly. **Item 46
   * built the request and item 48 draws the mark.** `CitationsForNotes` takes
   * the note ids the route already loaded and answers with *ids* — one entry per
   * id asked, in the order asked, empties included — so the mark is a union over
   * one reply. The loop is still refused, and for the sharper of its two
   * reasons: a `HighlightDto` per citing note would put the reader's private
   * text back on the wire once per tick, on a screen whose entire output is a
   * tick.
   *
   * **The mark and the toggle are two different facts and are drawn apart.** The
   * toggle says *the note I have open quotes this*, and it is a control — it
   * fills with the accent and it is how you undo it. The mark says *a note
   * quotes this*, it is a fact, and it sits on the passage's own metadata line
   * beside the chapter and page. A reader must be able to tell those apart, so
   * one may not be the other's colour.
   *
   * Its scope is honest and narrow: the batch is asked about **this book's
   * notes**, so every mark drawn is backed by a note in the pane beside it. A
   * note filed under no book could quote a passage here and go unmarked — there
   * is no reverse query and this screen claims only what it asked about.
   *
   * ## Taking a word off a passage (item 49)
   *
   * `Capture` is the control and its own doc carries the argument about the
   * word. What belongs here is the row above it: the words already taken off
   * this passage, past tense, no count, and no offer of the ones you have not
   * taken. `highlight_id` is `null` on every card minted before item 45 selected
   * the column, and those are shown against **no** passage rather than guessed
   * onto one.
   */
  import type { FlashcardDto, HighlightDto, NoteDto } from '$lib/api/bindings';
  import { cardWordsLabel, QUOTED } from '$lib/phrasing';
  import Capture from './Capture.svelte';

  let {
    highlights,
    open,
    cited,
    quoted,
    cards,
    oncite,
    onannotate,
    oncapture,
  }: {
    highlights: HighlightDto[];
    /** The note the notes band has open, and therefore the one to cite into. */
    open: NoteDto | null;
    /** Which of these passages that note already cites. One call, for one note. */
    cited: number[];
    /**
     * Which of these passages **some** note quotes — the union of one
     * `CitationsForNotes` reply, never a `CitationsFor` per note.
     */
    quoted: Set<number>;
    /** Every card captured from this book, anchored and unanchored alike. */
    cards: FlashcardDto[];
    oncite: (highlightId: number, on: boolean) => void;
    onannotate: (highlightId: number, text: string | null) => void;
    /** `true` created the card, `false` means it was already there. */
    oncapture: (highlightId: number, word: string, context: string) => Promise<boolean>;
  } = $props();

  let editing = $state<number | null>(null);
  let draft = $state('');

  /**
   * The passage elements, so a selection can be tested for being *inside* one.
   *
   * A plain object rather than `$state`: nothing renders it and it is read only
   * inside an event handler, so making it reactive would re-run the band on
   * every mount for no observer.
   */
  const quotes: Record<number, HTMLElement | undefined> = {};

  /**
   * What the reader has selected within one passage's own text, or `''`.
   *
   * **Both ends must be inside it.** A drag that began in the passage and ended
   * in the annotation under it is not a word from the passage, and
   * `Selection.toString()` would happily hand back both.
   */
  function selectionIn(id: number): string {
    const el = quotes[id];
    const sel = typeof window === 'undefined' ? null : window.getSelection();
    if (!el || !sel || sel.isCollapsed) return '';
    if (!el.contains(sel.anchorNode) || !el.contains(sel.focusNode)) return '';
    return sel.toString().trim();
  }

  /** The words already taken off this passage, in the order the engine listed them. */
  function wordsOn(id: number): string[] {
    return cards.filter((c) => c.highlight_id === id).map((c) => c.word);
  }

  function edit(h: HighlightDto) {
    editing = h.id;
    draft = h.annotation ?? '';
  }

  function commit(h: HighlightDto) {
    editing = null;
    // The empty box means *no annotation*, not an annotation that is blank —
    // and `null` is what clears the column. `''` would leave a row claiming the
    // reader wrote nothing, which is a different thing from not having written.
    onannotate(h.id, draft.trim() === '' ? null : draft);
  }

  /** Chapter and page, and nothing where there is neither. */
  function where(h: HighlightDto): string | null {
    const parts = [h.chapter, h.page !== null ? `p. ${h.page}` : null].filter(Boolean);
    return parts.length === 0 ? null : parts.join(' · ');
  }
</script>

<section class="band">
  <div class="band-head">
    <h2 class="band-title">Highlights</h2>
    {#if highlights.length > 0 && open === null}
      <!-- Names the move that turns these into citations. It is an offer, not a
           task: nothing here counts passages you have not cited. -->
      <span class="hint offer">Open a note to cite a passage into it.</span>
    {/if}
  </div>

  {#if highlights.length === 0}
    <p class="note">None here yet.</p>
    <p class="hint">
      <code>rb ko pull</code> brings across what is on a connected reader, marks and all.
    </p>
  {:else}
    <ul>
      {#each highlights as h (h.id)}
        {@const loc = where(h)}
        {@const isQuoted = quoted.has(h.id)}
        {@const words = wordsOn(h.id)}
        <li>
          <!-- `bind:this` so a selection can be tested for lying inside *this*
               passage rather than merely somewhere on the page. -->
          <blockquote bind:this={quotes[h.id]}>{h.text}</blockquote>
          {#if loc || isQuoted}
            <!-- KOReader does produce a highlight with neither a chapter nor a
                 page, and an empty line still takes space. Absence gets no
                 element rather than a stray separator.

                 `ko_datetime` is deliberately not here: it is the device's own
                 clock in the device's own timezone, and putting it beside the
                 UTC dates this app words elsewhere would be two clocks in one
                 column with nothing saying so.

                 The quoted mark (item 48) shares this line and **not** the cite
                 button's skin: accent *text* for a fact, accent *fill* for the
                 control that changes it. Collapsing the two into one visual is
                 the failure the item names — a reader has to be able to tell
                 "I am citing this into the note I have open" from "a note
                 quotes this". -->
            <p class="where">
              <!-- The separator is CSS and not a text node between two `{#if}`s:
                   Svelte trims the whitespace around markup, so ` · ` written
                   inline arrives as `p. 640·Quoted`. A `::before` cannot be
                   trimmed. -->
              {#if loc}{loc}{/if}{#if isQuoted}<span class="quoted" class:tail={loc !== null}
                  >{QUOTED}</span
                >{/if}
            </p>
          {/if}

          {#if h.ko_note}
            <p class="said theirs"><span class="who">KOReader</span>{h.ko_note}</p>
          {/if}

          {#if editing === h.id}
            <textarea bind:value={draft} rows="3" aria-label="Your note on this passage"></textarea>
            <div class="acts">
              <button type="button" class="primary" onclick={() => commit(h)}>Keep</button>
              <button type="button" onclick={() => (editing = null)}>Cancel</button>
            </div>
          {:else if h.annotation}
            <p class="said ours">
              <span class="who">You</span>{h.annotation}
              <button type="button" class="quiet" onclick={() => edit(h)}>Edit</button>
            </p>
          {/if}

          {#if cardWordsLabel(words)}
            <!-- What you took, never what you have not: no count, no offer, and
                 nothing here about the passages you have captured nothing from.
                 Below the two owners' words because it is neither of theirs —
                 it is a record of a move you made, and it sits next to the
                 control that makes another. -->
            <p class="kept">{cardWordsLabel(words)}</p>
          {/if}

          <div class="acts">
            {#if editing !== h.id && !h.annotation}
              <button type="button" onclick={() => edit(h)}>Write against this</button>
            {/if}
            {#if open !== null}
              <button
                type="button"
                class:on={cited.includes(h.id)}
                aria-pressed={cited.includes(h.id)}
                onclick={() => oncite(h.id, !cited.includes(h.id))}
              >
                {cited.includes(h.id) ? `Cited in “${open.title}”` : `Cite into “${open.title}”`}
              </button>
            {/if}
            <!-- Unconditional, unlike Cite: a card needs no note to be made
                 into, so this is the one control on a passage that is always
                 available. Its box and its confirmation wrap onto their own row
                 of this same flex line — see `Capture`'s stylesheet. -->
            <Capture
              passage={h}
              selection={() => selectionIn(h.id)}
              oncapture={(word, context) => oncapture(h.id, word, context)}
            />
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  /* See `NotePane` — each band owns its spacing. */
  section.band {
    margin-top: 2.2rem;
  }
  .band-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 0.9rem;
  }
  .offer {
    margin: 0;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--measure);
  }
  li {
    /* The accent as a rule beside the passage rather than under text — the one
       use of `--accent` itself that carries no words, so the contrast floor
       `--accent-text` exists for does not apply to it. */
    border-left: 2px solid var(--accent);
    padding-left: 0.85rem;
    margin-bottom: 1.5rem;
  }
  blockquote {
    margin: 0;
    overflow-wrap: anywhere;
  }
  .where {
    font-size: 0.75rem;
    color: var(--ink-dim);
    margin: 0.25rem 0 0;
  }
  /*
   * `--accent-text`, never `--accent`: this is the one of the two accent tokens
   * that carries a contrast floor, and `app.css` names the pair by name.
   *
   * **The weight is not decoration.** Measured in item 49's review, this text
   * is 5.09:1 on the page and the dim location beside it is 5.29:1 — so at
   * equal weight the two are separated by *hue alone*, and desaturated the
   * whole line collapses into one grey run. Colour-blind or not, a mark that
   * only a hue distinguishes from the chapter number is not a mark. The weight
   * is the second cue and the wider gap below is the third.
   */
  .quoted {
    color: var(--accent-text);
    font-weight: 500;
  }
  /* Only when there is a location to be separated *from*. A passage with no
     chapter and no page carries the mark alone, and a leading bullet there is
     the stray separator the `.where` line already refuses.

     A **gap** rather than the `·` the location parts use between themselves:
     the bullet said the mark was a third field of the same run, which is
     exactly what it is not. */
  .quoted.tail {
    margin-left: 1.1em;
  }
  /* Ink rather than `--ink-dim`, which is the whole point: this is the reader's
     own move, said in the reader's own voice, and in dim it measured identical
     to the location line above it and read as a third metadata field. Smaller
     than the owner notes so it stays quieter than what was written. */
  .kept {
    font-size: 0.78rem;
    color: var(--ink);
    margin: 0.5rem 0 0;
    overflow-wrap: anywhere;
  }
  /* Who wrote it, said in a word rather than implied by a shade. Two grey
     paragraphs is the state this screen exists to end. */
  .said {
    font-size: 0.88rem;
    margin: 0.5rem 0 0;
    overflow-wrap: anywhere;
  }
  .who {
    display: inline-block;
    font-size: 0.68rem;
    text-transform: uppercase;
    letter-spacing: 0.07em;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0 0.35rem;
    margin-right: 0.45rem;
    color: var(--ink-dim);
    vertical-align: 0.08em;
  }
  .theirs {
    color: var(--ink-dim);
  }
  .ours .who {
    color: var(--accent-text);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
  }

  .acts {
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
    margin-top: 0.5rem;
  }
  .acts:empty {
    display: none;
  }
  button {
    font: inherit;
    font-size: 0.75rem;
    line-height: 1.4;
    color: var(--ink-dim);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.15rem 0.55rem;
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
  /* A cited passage says so on the control that undoes it, so the state and the
     way out of it are one thing. */
  button.on {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
  }
  button.quiet {
    background: none;
    border: 0;
    padding: 0 0.3rem;
  }
  textarea {
    font: inherit;
    font-size: 0.88rem;
    width: 100%;
    margin-top: 0.5rem;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.5rem 0.6rem;
    resize: vertical;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
</style>
