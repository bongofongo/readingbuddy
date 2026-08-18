<script lang="ts">
  /**
   * The passages you kept, and the two different things written against them.
   *
   * ## Two notes, two owners, and the labels are load-bearing
   *
   * `ko_note` is **KOReader's** and is rewritten toward the device on every
   * import; `annotation` is **the reader's** and no import has ever touched it.
   * That split is the whole of `docs/decisions.md`'s highlight-ownership
   * section, and this screen is the one place a reader can learn that their own
   * words survive a device refresh and the device's do not. Unlabelled they are
   * two grey paragraphs.
   *
   * ## The mark and the toggle are different facts and are drawn apart
   *
   * The toggle says *the note I have open quotes this*, and it is a control — it
   * fills with the accent and it is how you undo it. The mark says *a note
   * quotes this*, it is a fact, and it sits on the passage's own metadata line.
   * Collapsing them into one visual passes every assertion and leaves the reader
   * unable to tell which of the two they are looking at.
   *
   * The mark is one call for the whole page of notes (`citationsForNotes`),
   * never one per note — and its scope is honest: the batch is asked about
   * **this book's** notes, so a note filed under no book can quote a passage
   * here and go unmarked.
   *
   * ## The controls are hidden, and the tab stops are the reason it is safe
   *
   * `Annotate` / `Make a card` / `Cite` on forty passages is a hundred and
   * twenty controls, which is exactly the "too much happening" the brief
   * objects to — the passage is the content and the controls are not. So they
   * are revealed on hover, on focus, and on the active passage, and they are
   * unconditionally visible where there is no hover to speak of.
   *
   * **`opacity: 0` does not remove anything from the tab order**, and that is
   * the defect the reveal would otherwise ship: a keyboard user tabbing down the
   * list would land on invisible buttons, three per passage, most of them with
   * no visible focus indicator because the button itself is transparent. That is
   * SC 2.4.7 failing in substance if not in letter.
   *
   * **The fix is to the list, not to the reveal.** This is a composite widget:
   * the list contributes **one** tab stop, arrow keys move between passages, and
   * only the active passage's own controls are tabbable. Forty passages went
   * from ~120 stops to ~4, the reveal became correct for free (the controls are
   * properties of the active passage rather than independent controls), and the
   * same move is what the power-user path is made of — `j`/`k` through a list
   * *is* roving tabindex.
   *
   * **It deliberately does not claim `role="listbox"`.** A role is a promise:
   * listbox tells assistive technology that arrow keys, Home/End, type-ahead and
   * a selection model all work, and it forbids interactive children — which
   * these rows are full of. Shipping the role without the contract is worse than
   * shipping no role, because it removes the user's fallback expectations. This
   * is an ordinary list whose items are focusable, which is what it is.
   */
  import type { FlashcardDto, HighlightDto, NoteDto } from '$lib/api/bindings';
  import { cardWordsLabel, QUOTED } from '$lib/phrasing';

  import Capture from './Capture.svelte';

  let {
    highlights,
    open,
    cited,
    quoted,
    found,
    cards,
    oncite,
    onannotate,
    oncapture,
  }: {
    highlights: HighlightDto[];
    /** The note the centre column has open, and therefore the one to cite into. */
    open: NoteDto | null;
    /** Which of these passages that note already cites. One call, for one note. */
    cited: number[];
    /**
     * Which of these passages **some** note quotes — the union of one
     * `CitationsForNotes` reply, never a `CitationsFor` per note.
     */
    quoted: Set<number>;
    /**
     * The passage a search hit sent the reader to, or `null` (item 50).
     *
     * A mark on the passage and **not** a filter of the list: a band that showed
     * only the hit would have thrown away where the passage sits, which is half
     * of why a reader searches for it.
     */
    found: number | null;
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
   * Which passage owns the list's single tab stop.
   *
   * Index rather than id, because it is a *position* in the list — the thing
   * arrow keys move — and an id would have to be resolved back to one on every
   * press. It survives a re-render of the same list and resets when the book
   * changes, which is the same frame the list itself is replaced on.
   */
  let active = $state(0);

  /**
   * The passage elements, by index.
   *
   * A plain object rather than `$state`: it is written during render and read
   * only inside event handlers, so making it reactive would re-run the band on
   * every mount for no observer. `quotes` holds the blockquotes for the
   * selection test; `rows` holds the list items for focus.
   */
  const quotes: Record<number, HTMLElement | undefined> = {};
  const rows: Record<number, HTMLElement | undefined> = {};

  /**
   * Arrow keys move within the list; Tab moves out of it.
   *
   * Home and End are here because a list you can only walk one row at a time is
   * a list you stop using at forty rows. Nothing single-key is bound — no `j`,
   * no `k`, no `/` — and that is deliberate for now: WCAG 2.1.4 requires a
   * single-character shortcut to be switchable off, remappable, or scoped to
   * focus, and the honest version of the third is worth building on purpose
   * rather than inheriting from a convenience.
   */
  function key(e: KeyboardEvent) {
    const last = highlights.length - 1;
    let next = active;
    if (e.key === 'ArrowDown') next = Math.min(active + 1, last);
    else if (e.key === 'ArrowUp') next = Math.max(active - 1, 0);
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = last;
    else return;
    e.preventDefault();
    active = next;
    rows[next]?.focus();
  }

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

{#if highlights.length === 0}
  <p class="note">No passages kept from this book.</p>
  <p class="hint">
    <code>rb ko pull</code> brings across what is on a connected reader, marks and all.
  </p>
{:else}
  {#if open === null}
    <!-- Names the move that turns these into citations. It is an offer, not a
         task: nothing here counts passages you have not cited. -->
    <p class="hint offer">Open a note to cite a passage into it.</p>
  {/if}
  <!--
    Two warnings silenced, and the reason is the header's: a list whose items are
    focusable is a **composite widget**, which is what takes this region from
    ~120 tab stops to one — and the linter's two rules describe a list that is
    only ever read. The alternative it would accept is `role="listbox"`, and a
    role is a promise: listbox commits to type-ahead, a selection model and
    non-interactive children, all three of which are false here. Shipping the
    role without the contract removes the user's fallback expectations, which is
    worse than shipping none.
  -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <ul class="passages" aria-label="Passages" onkeydown={key}>
    {#each highlights as h, i (h.id)}
      {@const loc = where(h)}
      {@const isQuoted = quoted.has(h.id)}
      {@const words = wordsOn(h.id)}
      {@const on = i === active}
      <!-- The id is the search's anchor (item 50): the page scrolls to it by
           `getElementById` from the click, so a second click on the same hit
           takes the reader back rather than doing nothing. -->
      <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
      <li
        id={`passage-${h.id}`}
        bind:this={rows[i]}
        class:found={found === h.id}
        class:active={on}
        tabindex={on ? 0 : -1}
        onfocusin={() => (active = i)}
      >
        <!-- `bind:this` so a selection can be tested for lying inside *this*
             passage rather than merely somewhere on the page. -->
        <blockquote bind:this={quotes[h.id]}>{h.text}</blockquote>
        {#if loc || isQuoted}
          <!-- KOReader does produce a highlight with neither a chapter nor a
               page, and an empty line still takes space. Absence gets no element
               rather than a stray separator.

               `ko_datetime` is deliberately not here: it is the device's own
               clock in the device's own timezone, and putting it beside the UTC
               dates this app words elsewhere would be two clocks in one column
               with nothing saying so. -->
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
          <div class="acts shown">
            <button type="button" class="primary" onclick={() => commit(h)}>Keep</button>
            <button type="button" onclick={() => (editing = null)}>Cancel</button>
          </div>
        {:else if h.annotation}
          <p class="said ours">
            <span class="who">You</span>{h.annotation}
            <button type="button" class="quiet" tabindex={on ? 0 : -1} onclick={() => edit(h)}>
              Edit
            </button>
          </p>
        {/if}

        {#if cardWordsLabel(words)}
          <!-- What you took, never what you have not: no count, no offer, and
               nothing here about the passages you have captured nothing from. -->
          <p class="kept">{cardWordsLabel(words)}</p>
        {/if}

        {#if editing !== h.id}
          <div class="acts">
            {#if !h.annotation}
              <button type="button" tabindex={on ? 0 : -1} onclick={() => edit(h)}>
                Write against this
              </button>
            {/if}
            {#if open !== null}
              <button
                type="button"
                tabindex={on ? 0 : -1}
                class:on={cited.includes(h.id)}
                aria-pressed={cited.includes(h.id)}
                onclick={() => oncite(h.id, !cited.includes(h.id))}
              >
                {cited.includes(h.id) ? `Cited in “${open.title}”` : `Cite into “${open.title}”`}
              </button>
            {/if}
            <!-- Unconditional, unlike Cite: a card needs no note to be made
                 into, so this is the one control on a passage that is always
                 available. -->
            <Capture
              passage={h}
              tabbable={on}
              selection={() => selectionIn(h.id)}
              oncapture={(word, context) => oncapture(h.id, word, context)}
            />
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .offer {
    margin: 0 0 1rem;
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    max-width: var(--passages);
  }
  li {
    /* The accent as a rule beside the passage rather than under text — the one
       use of `--accent` itself that carries no words, so the contrast floor
       `--accent-text` exists for does not apply to it. */
    border-left: 2px solid var(--line);
    padding-left: 0.85rem;
    margin-bottom: 1.6rem;
    /* The list item is the tab stop, so it must show that it has focus. The ring
       is `app.css`'s and is not overridden here. */
    outline-offset: 4px;
  }
  /* A passage some note quotes gets the accent rule; the rest get a hairline.
     Two states of one edge, which is cheaper than a second mark. */
  li:has(.quoted) {
    border-left-color: var(--accent);
  }
  /*
   * The passage a search hit sent the reader to (item 50).
   *
   * Every passage already carries a rule down its left, so a mark made of the
   * same colour again would be one hue against the same hue. This differs by
   * **shape and ground**: a thicker rule and a faint tint behind the row, both
   * of which survive a desaturated screen and neither of which puts colour on
   * text. The padding is stated on both states so arriving at a passage does not
   * shift the page under the reader's eye.
   */
  li.found {
    border-left-width: 5px;
    padding-left: 0.7rem;
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    border-radius: 0 var(--radius) var(--radius) 0;
  }
  blockquote {
    margin: 0;
    font-size: 0.95rem;
    line-height: 1.65;
    overflow-wrap: anywhere;
  }
  .where {
    font-size: 0.75rem;
    color: var(--ink-dim);
    margin: 0.25rem 0 0;
  }
  /*
   * `--accent-text`, never `--accent`: this is the one of the two accent tokens
   * that carries a contrast floor.
   *
   * **The weight is not decoration.** At equal weight this text and the dim
   * location beside it are separated by *hue alone*, and desaturated the whole
   * line collapses into one grey run. The weight is the second cue and the wider
   * gap is the third.
   */
  .quoted {
    color: var(--accent-text);
    font-weight: 500;
  }
  /* Only when there is a location to be separated *from*, and a **gap** rather
     than the `·` the location parts use between themselves: the bullet said the
     mark was a third field of the same run, which is what it is not. */
  .quoted.tail {
    margin-left: 1.1em;
  }
  /* Ink rather than `--ink-dim`, which is the point: this is the reader's own
     move, said in the reader's own voice. Smaller than the owner notes so it
     stays quieter than what was written. */
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
    /*
     * Hidden until the passage is under the pointer or has focus inside it.
     *
     * **Not on the active row.** The active row is where the list's single tab
     * stop currently is, which starts at the first passage — revealing on it
     * would put three controls on screen at rest on every book page, which is
     * the thing the reveal exists to prevent. Focus is the keyboard's reveal and
     * `:focus-within` already covers it: arrowing to a row focuses it.
     *
     * **`opacity` alone, not `visibility`.** A transparent button still takes a
     * click, which sounds like the defect and is not: the pointer can only be
     * over one of these while it is over the passage, and hovering the passage
     * is what reveals them. `visibility: hidden` would instead make the controls
     * unreachable to anything that checks visibility before acting — a test
     * driver, a screen-reader's mouse emulation — while buying nothing the
     * roving tabindex above has not already bought. The tab-order defect is
     * fixed by the list being one stop, not by hiding harder.
     */
    opacity: 0;
  }
  .acts.shown,
  li:hover .acts,
  li:focus-within .acts {
    opacity: 1;
  }
  /* Where there is no hover there is nothing to reveal on, so nothing is
     hidden. A touch screen must not have to guess. */
  @media (hover: none) {
    .acts {
      opacity: 1;
    }
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
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
