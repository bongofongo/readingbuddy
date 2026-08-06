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
   * It needs a note to cite **into**, so it appears only when one is open. What
   * is deliberately *not* here is a mark saying which passages are cited by
   * some *other* note: that needs one `CitationsFor` per note in the book, an
   * N+1 with no request behind it, and it is recorded as a later item rather
   * than built badly.
   */
  import type { HighlightDto, NoteDto } from '$lib/api/bindings';

  let {
    highlights,
    open,
    cited,
    oncite,
    onannotate,
  }: {
    highlights: HighlightDto[];
    /** The note the notes band has open, and therefore the one to cite into. */
    open: NoteDto | null;
    /** Which of these passages that note already cites. One call, not one per note. */
    cited: number[];
    oncite: (highlightId: number, on: boolean) => void;
    onannotate: (highlightId: number, text: string | null) => void;
  } = $props();

  let editing = $state<number | null>(null);
  let draft = $state('');

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
        <li>
          <blockquote>{h.text}</blockquote>
          {#if where(h)}
            <!-- KOReader does produce a highlight with neither a chapter nor a
                 page, and an empty line still takes space. Absence gets no
                 element rather than a stray separator.

                 `ko_datetime` is deliberately not here: it is the device's own
                 clock in the device's own timezone, and putting it beside the
                 UTC dates this app words elsewhere would be two clocks in one
                 column with nothing saying so. -->
            <p class="where">{where(h)}</p>
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
