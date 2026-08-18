<script lang="ts">
  /**
   * The book's index — the left rail, and the thing that makes the centre
   * swappable without anything being modal.
   *
   * Three sections, and each is here for a different reason.
   *
   * **Write, at the top, always.** Note, Reflection, Review. These used to be
   * reachable only when no note was open, which hid the act the page exists for
   * exactly while you were performing a neighbouring one.
   *
   * **What you wrote — one list, never four tabs.** The TUI's ruling and it
   * stays right: a tab holding the single reflection is the wrong shape for a
   * thing there is exactly one of. The row says which kind it is instead.
   *
   * This section is also doing a job nobody assigned it, and it is worth writing
   * down because it constrains what may be moved out of here. A rail that is
   * always there, always the same, and only sometimes relevant gets learned as
   * wallpaper — and **the failure mode of a persistent rail is not clutter, it is
   * that the one time it matters it is invisible.** `Write` and `The book` are
   * constant and will habituate, which is fine and is what a mode selector is
   * for. *What you wrote* is the part that **changes**, and that is what keeps
   * the whole rail alive as a region rather than a fixture. Do not make it the
   * part that gets moved somewhere else.
   *
   * **The book — reference, reachable, not resident.** `Passages`, `Reads`,
   * `About & sources` switch the centre; `Cards →` leaves for another route, and
   * the arrow is what says so.
   */
  import type { NoteDto } from '$lib/api/bindings';
  import { noteKindLabel } from '$lib/phrasing';

  import type { Centre } from './desk';

  let {
    bookId,
    notes,
    centre,
    openNoteId,
    onshow,
    onopen,
    oncompose,
    onanchored,
  }: {
    bookId: number;
    notes: NoteDto[];
    centre: Centre;
    openNoteId: number | null;
    onshow: (centre: Centre) => void;
    onopen: (id: number) => void;
    oncompose: () => void;
    onanchored: (kind: 'reflection' | 'review') => void;
  } = $props();
</script>

<nav class="rail" aria-label="This book">
  <section>
    <h2 class="band-title">Write</h2>
    <div class="acts">
      <button type="button" onclick={oncompose}>Note</button>
      <!-- Open **or mint** — one call, and the engine decides which reading it
           hangs off. A reflection is private and grows as you read; a review is
           the one written for other people, and the only one that carries a
           rating. -->
      <button type="button" onclick={() => onanchored('reflection')}>Reflection</button>
      <button type="button" onclick={() => onanchored('review')}>Review</button>
    </div>
  </section>

  <section>
    <h2 class="band-title">What you wrote</h2>
    {#if notes.length === 0}
      <!-- Idle is not blank, and this is not an apology or a count of zero. The
           moves that fill it are the three buttons directly above. -->
      <p class="hint">Nothing written against this book.</p>
    {:else}
      <ul>
        {#each notes as n (n.id)}
          <li>
            <button
              type="button"
              class="row"
              class:on={n.id === openNoteId}
              aria-current={n.id === openNoteId ? 'true' : undefined}
              onclick={() => onopen(n.id)}
            >
              {#if noteKindLabel(n.kind)}
                <span class="kind" class:reflection={n.kind === 'reflection'}
                  >{noteKindLabel(n.kind)}</span
                >
              {/if}
              <span class="title">{n.title}</span>
            </button>
          </li>
        {/each}
      </ul>
    {/if}
  </section>

  <section>
    <h2 class="band-title">The book</h2>
    <ul class="places">
      <li>
        <button
          type="button"
          class="row"
          class:on={centre === 'passages'}
          aria-current={centre === 'passages' ? 'true' : undefined}
          onclick={() => onshow('passages')}>Passages</button
        >
      </li>
      <li>
        <button
          type="button"
          class="row"
          class:on={centre === 'reads'}
          aria-current={centre === 'reads' ? 'true' : undefined}
          onclick={() => onshow('reads')}>Reads</button
        >
      </li>
      <li>
        <button
          type="button"
          class="row"
          class:on={centre === 'about'}
          aria-current={centre === 'about' ? 'true' : undefined}
          onclick={() => onshow('about')}>About &amp; sources</button
        >
      </li>
      <li>
        <!-- A card carries a passage, and a passage wants a measure this column
             does not have — so the wall of this book's cards is its own route.
             The arrow is the app saying you are leaving this page. -->
        <a class="row leaves" href={`/book/${bookId}/cards`}>Cards →</a>
      </li>
    </ul>
  </section>
</nav>

<style>
  .rail {
    min-width: 0;
  }
  section + section {
    margin-top: 1.9rem;
  }
  .band-title {
    margin-bottom: 0.6rem;
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .acts button {
    font: inherit;
    font-size: 0.78rem;
    line-height: 1.4;
    color: var(--ink-dim);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.2rem 0.65rem;
    cursor: pointer;
  }
  .acts button:hover {
    color: var(--ink);
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .row {
    display: flex;
    align-items: baseline;
    gap: 0.4rem;
    width: 100%;
    text-align: start;
    font: inherit;
    font-size: 0.85rem;
    background: none;
    border: 0;
    /* The inset lives here so selecting a row does not move the text under the
       pointer — the same 2px is transparent when the row is not selected. */
    border-left: 2px solid transparent;
    padding: 0.3rem 0.2rem 0.3rem 0.55rem;
    color: var(--ink-dim);
    cursor: pointer;
  }
  .row:hover {
    color: var(--ink);
  }
  /*
   * Selected: an accent inset, and `--bg-raised` behind it.
   *
   * **The inset is doing all the work and the fill is nearly free.**
   * `--bg-raised` measures Lc 0.0 against `--bg` in both themes, so this is one
   * cue with a second one that is only *almost* visible — which is fine, and is
   * why it is written down: do not add a third state that leans on the fill
   * alone to be distinguishable, because it will not be.
   */
  .row.on {
    color: var(--ink);
    background: var(--bg-raised);
    border-left-color: var(--accent);
  }
  .row .title {
    min-width: 0;
    overflow-wrap: anywhere;
    /* One line. The whole title is on the note itself, which is one click away
       and is where there is room for it. */
    display: -webkit-box;
    -webkit-line-clamp: 1;
    line-clamp: 1;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .kind {
    font-size: 0.66rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--ink-dim);
    flex: none;
  }
  /*
   * The reflection's chip is **not** accent, and that is a change.
   *
   * The accent was doing eight jobs at once — the wordmark, the current page,
   * the selected row, the progress fill, the primary button, an inline literal,
   * this chip, and the state line in the header. A colour that means eight
   * things means none, and the two that most need to be loud are *this is
   * selected* and *this is the action*. The rule adopted instead: **the accent
   * is for state that is true right now and that you can act on.** A note's kind
   * is descriptive, so it is carried by the word, which is what it always was.
   */
  .kind.reflection {
    color: var(--ink-dim);
  }
  .places .row {
    color: var(--ink-dim);
  }
  .leaves {
    display: block;
  }
  .hint {
    margin: 0;
    font-size: 0.82rem;
  }
</style>
