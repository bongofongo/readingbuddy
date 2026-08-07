<script lang="ts">
  /**
   * Taking a word off a passage — the first way a reader can mint a card.
   *
   * Until item 45 `Storage::insert_flashcard` had exactly one production caller:
   * the KOReader import's auto-capture of single-word highlights. So a card
   * could be **minted by an import and by nothing else**, and a reader who
   * wanted one for a word they met in a book they were holding had no move to
   * make. This is that move, on the passage, for the reason the Cite control is
   * on the passage: it is the gesture a pointer makes available that a terminal
   * list could not.
   *
   * ## The word: the selection fills the box, and the box is what is saved
   *
   * `docs/prompts/48-49` proposed the selection *being* the word, with a typed
   * box as its fallback. It is the better half of a real question and it is
   * taken here as an **accelerator rather than as the mechanism**, which is a
   * correction rather than a compromise:
   *
   * - A control that writes different things depending on whether some text
   *   happened to be selected is a **hidden mode**. Cite is conditional too, but
   *   on which note is *open* — visible state, named on the button itself. A
   *   selection is invisible the instant the reader looks at the button, and by
   *   then it is gone from the page as well.
   * - `UNIQUE(book_id, word)` makes the word the card's **identity**. A drag
   *   that catches a trailing space, a following comma or two words instead of
   *   one would mint a card keyed on that, and the reader would never see what
   *   they had saved until they exported it.
   * - Trimming a selection down to "a word" in TypeScript would be a rule about
   *   what a word is, invented above the seam and disagreeing with
   *   `single_word`'s. Showing the reader what is about to be saved needs no
   *   rule at all.
   *
   * So: select and the box arrives holding it, selected, ready to be replaced;
   * select nothing and the box arrives empty and focused. Both end in the same
   * write, and the reader has seen the word either way. The engine still trims —
   * this deliberately does not, because `Engine::create_flashcard` trims before
   * it dedupes and a second copy of that rule is a second thing to keep true.
   *
   * ## The reply has two faces and both are ordinary
   *
   * `true` created it. `false` means *you already had this card*, left exactly
   * as it was — `ON CONFLICT DO NOTHING`, so a later capture of the same word
   * does not repoint it at this passage. Rendering both as "saved" throws away
   * the only thing the write answers, and rendering `false` as an error would
   * make having already done something a failure. It is neither: it is the app
   * telling you what you did.
   *
   * No dialog, at any depth. The book view is one pane at three depths and a
   * modal here would be the thing item 27 refused, arriving through a side door.
   */
  import type { HighlightDto } from '$lib/api/bindings';

  let {
    passage,
    selection,
    oncapture,
  }: {
    passage: HighlightDto;
    /**
     * What the reader has selected **inside this passage**, or `''`.
     *
     * A function rather than a value: it is read at the click, because a
     * selection is live state the browser owns and mirroring it into a prop
     * would mean re-rendering every passage on every drag.
     */
    selection: () => string;
    /** `true` created it, `false` means the card was already there. */
    oncapture: (word: string, context: string) => Promise<boolean>;
  } = $props();

  let open = $state(false);
  let draft = $state('');
  let outcome = $state<'created' | 'already' | null>(null);
  let failure = $state<string | null>(null);
  let box = $state<HTMLInputElement | null>(null);

  function start() {
    draft = selection();
    outcome = null;
    failure = null;
    open = true;
  }

  $effect(() => {
    if (!open) return;
    // Focused **and** selected: an empty box is ready to type into, and a
    // prefilled one arrives with its contents chosen, so typing replaces the
    // selection the reader made rather than appending to it.
    box?.focus();
    box?.select();
  });

  async function keep() {
    const word = draft.trim();
    // Presentation, not validation: an empty word is the engine's
    // `InvalidInput` and stays so. This only declines to send one.
    if (word === '') return;
    try {
      // The whole passage as context, always — it is what makes the card worth
      // reading later, and it is already on screen so nothing is being guessed.
      outcome = (await oncapture(word, passage.text)) ? 'created' : 'already';
      open = false;
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    }
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      keep();
    } else if (e.key === 'Escape') {
      open = false;
    }
  }
</script>

{#if open}
  <div class="box">
    <label>
      <span class="lede">The word</span>
      <input
        bind:this={box}
        bind:value={draft}
        onkeydown={key}
        type="text"
        autocomplete="off"
        spellcheck="false"
      />
    </label>
    <div class="row">
      <button type="button" class="primary" onclick={keep} disabled={draft.trim() === ''}>
        Keep
      </button>
      <button type="button" onclick={() => (open = false)}>Cancel</button>
      <span class="lede">The passage is kept with it.</span>
    </div>
  </div>
{:else}
  <!--
    `preventDefault` on **mousedown**, which is the whole reason the prefill
    works at all: pressing a button outside the selection collapses it before
    `click` ever fires, so reading `window.getSelection()` in the handler would
    find nothing every time. Preventing the default only costs the button its
    mouse focus; Tab and Enter are untouched.
  -->
  <button type="button" class="quiet" onmousedown={(e) => e.preventDefault()} onclick={start}>
    Make a card
  </button>
{/if}

{#if failure}
  <!-- A refusal says what was refused and names the thing that works. A
       highlight this book does not own is `InvalidInput` and one that is gone is
       `NotFound`; both mean the row moved under the page, and re-opening it is
       the move. -->
  <p class="outcome">That card was not made: {failure}. Re-open the book and try again.</p>
{:else if outcome === 'created'}
  <p class="outcome">Kept. <code>rb cards export</code> takes it to Anki.</p>
{:else if outcome === 'already'}
  <!-- Not an error, not styled as one, and no "already" scolding: the card is
       there and unchanged, which is a true and unremarkable thing to report. -->
  <p class="outcome">You already had that one, and it is unchanged.</p>
{/if}

<style>
  /*
   * The box and the outcome are full-width rows of the passage's own `.acts`
   * flex line, so the trigger sits **beside** Write-against-this and Cite rather
   * than starting a second control row under them. `flex: 1 0 100%` is what
   * wraps them onto their own line without a second container in `Passages`.
   */
  .box,
  .outcome {
    flex: 1 0 100%;
  }
  .box {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.4rem 0.6rem;
    margin-top: 0.15rem;
  }
  label {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    min-width: 0;
  }
  /*
   * A field label in **sentence case**, and that is a correction rather than a
   * default. It was uppercase and letterspaced, which is this app's *band
   * heading* dress (`HIGHLIGHTS`, `READ`, `ABOUT`) and also its *owner chip*
   * dress (`KOREADER`, `YOU`, boxed) — at narrow width all three appeared
   * within 200px of each other, three jobs in one typographic class. The label
   * is doing real work (a bare input says nothing about what goes in it) and it
   * is not a heading, so it is dressed as neither.
   */
  .lede {
    font-size: 0.75rem;
    color: var(--ink-dim);
  }
  input {
    font: inherit;
    font-size: 0.88rem;
    width: 14ch;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.15rem 0.45rem;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    flex-wrap: wrap;
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
  /*
   * The trigger is **borderless**, which is the one thing both screenshot
   * reviews of item 49 asked for independently. Every passage carries this
   * control, so as a bordered pill it put a second one on every row: six
   * identical `Write against this | Make a card` pairs down a reread's band,
   * wider and heavier than the one-sentence passages they belong to, and three
   * equal-weight pills in a row once a note was open. The precedent is on this
   * exact screen — `Edit` beside a `YOU` annotation — and the hierarchy it
   * states is the true one: writing against a passage and citing it are the
   * band's moves, and taking a word off it is the rarer one.
   */
  button.quiet {
    background: none;
    border: 0;
    padding: 0 0.3rem;
  }
  button.primary {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
    font-weight: 600;
  }
  button:disabled {
    cursor: default;
    opacity: 0.5;
  }
  button.primary:disabled {
    /* A disabled primary must not keep shouting: at 50% opacity the accent fill
       still reads as the thing to press, so it drops back to the quiet skin. */
    color: var(--ink-dim);
    background: var(--bg-raised);
    border-color: var(--line);
    font-weight: inherit;
  }
  .outcome {
    font-size: 0.78rem;
    color: var(--ink-dim);
    margin: 0.15rem 0 0;
  }
  code {
    font-size: 0.95em;
  }
</style>
