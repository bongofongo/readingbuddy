<script lang="ts">
  /**
   * The four things you can do, and the two ways out.
   *
   * ## Why the exits sit in the same row as the verbs
   *
   * They are separated by a rule and by colour, not by distance, and that is
   * deliberate: the axiom's *nothing is a dead end* is only satisfied if the way
   * out is visible **while a panel is open**, and a footer that scrolled away
   * with the panel would satisfy it only at rest. One row, always at the same
   * place, holding both kinds of move.
   *
   * ## The keys are shown, and that is not decoration
   *
   * This surface is meant to be left open beside a terminal, so it is a
   * keyboard surface, and a keyboard shortcut nobody can discover is a shortcut
   * nobody uses. The letter is drawn dim and small beside the word rather than
   * in a tooltip, because a tooltip requires the mouse the shortcut exists to
   * avoid. `aria-keyshortcuts` says the same thing to a screen reader.
   *
   * ## No number on any of them
   *
   * Not a passage count, not an unsynced count, not a badge. `mode.ts` asserts
   * the labels carry no digit, which is the checkable half; this comment is the
   * half that says why the assertion is there. A count here would be the library
   * counting itself at the one moment it is supposed to be out of the way.
   */
  import { LIBRARY } from '$lib/nav';

  import { type Panel, VERBS } from './mode';

  let {
    panel,
    bookId,
    onshow,
  }: {
    panel: Panel;
    bookId: number;
    onshow: (next: Panel) => void;
  } = $props();
</script>

<div class="verbs">
  <div class="acts" role="group" aria-label="What you can do">
    {#each VERBS as v (v.panel)}
      <button
        type="button"
        aria-pressed={panel === v.panel}
        aria-keyshortcuts={v.key}
        onclick={() => onshow(v.panel)}
      >
        {v.label}<span class="key" aria-hidden="true">{v.key}</span>
      </button>
    {/each}
  </div>

  <div class="ways">
    <a href="/book/{bookId}">The book</a>
    <!-- `/library` and not `/`: the entrance is *Reading now*, which is the page
         listing the books this surface is already showing one of. The way out of
         a book is the whole collection. -->
    <a href={LIBRARY}>The library</a>
  </div>
</div>

<style>
  .verbs {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 0.4rem 1.25rem;
  }
  .acts {
    display: flex;
    gap: 0.4rem;
  }

  button {
    font: inherit;
    font-size: 0.9rem;
    color: var(--ink-dim);
    background: none;
    border: 1px solid transparent;
    border-radius: var(--radius);
    padding: 0.35rem 0.7rem;
    cursor: pointer;
    display: inline-flex;
    align-items: baseline;
    gap: 0.4rem;
  }
  button:hover {
    color: var(--ink);
    border-color: var(--line);
  }
  /*
   * Which panel is up, in the accent — the rule the layout rework settled: the
   * accent is for state that is true right now and that you can act on, and
   * *this one is open* is exactly that. The border is on every state so lighting
   * one does not move the row.
   */
  button[aria-pressed='true'] {
    color: var(--accent-text);
    border-color: var(--accent);
  }
  .key {
    font-size: 0.7rem;
    opacity: 0.65;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
  }

  .ways {
    display: flex;
    gap: 1rem;
    font-size: 0.85rem;
    padding-left: 1.25rem;
    border-left: 1px solid var(--line);
  }
  .ways a {
    color: var(--ink-dim);
    border-bottom: 1px solid transparent;
    padding-bottom: 1px;
  }
  .ways a:hover {
    color: var(--ink);
    border-bottom-color: var(--line);
  }

  /* Below this the row wraps, and a left border on a wrapped group is a rule
     hanging in the middle of nothing. */
  @media (max-width: 520px) {
    .ways {
      padding-left: 0;
      border-left: none;
    }
  }
</style>
