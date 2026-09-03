<script lang="ts">
  /**
   * Writing — the three kinds of thing you can write against a book, behind one
   * control.
   *
   * ## Why the three arrived here
   *
   * The rail offered *Note*, *Reflection* and *Review* as three buttons, always
   * on screen, on a page that also carried four destinations, a note list, a
   * search box, a link search and a citation list. Three of those twelve
   * controls were one decision — *write something* — spread across the page's
   * interface so that the page had to state the whole taxonomy before the reader
   * had said they wanted to write at all.
   *
   * One `Write` on the page, and the kind is chosen **here**, is the same
   * capability with a third of the interface. That is the whole trade: a narrow
   * control over a component that is allowed to be deep.
   *
   * ## The three are not three of the same thing, and it shows
   *
   * A **note** does not exist until the button is pressed — this component holds
   * a title and a body and nothing is in the vault until *Write it*. Navigation
   * therefore never leaves a row behind, which is why the library's *Write* link
   * lands here rather than minting one.
   *
   * A **reflection** and a **review** are open-**or**-mint: one call, and the
   * engine decides which reading it hangs off and whether there is already one.
   * There is at most one of each per reading, so there is no form to fill in —
   * picking the kind and opening it are the same act. Drawing them as a form
   * with a title field would be this component pretending three things are one
   * thing because they share a control.
   *
   * So the kind switch shows a form for the first and a sentence with one button
   * for the other two. A reader can see, before committing, that *Reflection*
   * will take them to the reflection they already have.
   */
  import { client } from '$lib/api/client';

  type Kind = 'note' | 'reflection' | 'review';

  let {
    bookId,
    onwritten,
    onanchored,
    oncancel,
  }: {
    bookId: number;
    /** The note that now exists, so the page can open it. */
    onwritten: (id: number) => Promise<void>;
    /** Open or mint the one reflection/review, which the page's client call does. */
    onanchored: (kind: 'reflection' | 'review') => Promise<void>;
    oncancel: () => void;
  } = $props();

  let kind = $state<Kind>('note');
  let title = $state('');
  let body = $state('');
  let failure = $state<string | null>(null);

  /**
   * What each kind is, said before it is chosen.
   *
   * Written here rather than in the markup because it is the same sentence
   * shape three times and because the wording is the only thing that tells a
   * reader why *Reflection* has no title field.
   */
  const SAYS: Record<Kind, string> = {
    note: 'A note is anything you want to keep — as many as you like, each with its own title.',
    reflection:
      'A reflection is private and grows as you read. There is one per read, so this opens the one you have.',
    review:
      'A review is the one written for other people, and the only kind that carries a rating. There is one per read, so this opens the one you have.',
  };

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

  async function anchored() {
    if (kind === 'note') return;
    try {
      await onanchored(kind);
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    }
  }
</script>

<div class="composer">
  <h2>Write</h2>

  <!-- The kind, as the app's one way of saying *this one of these*. Three
       members and no fill: the fill on this surface belongs to the button that
       writes. -->
  <div class="choices" role="group" aria-label="What to write">
    <button
      class="choice"
      type="button"
      aria-pressed={kind === 'note'}
      onclick={() => (kind = 'note')}
    >
      Note
    </button>
    <button
      class="choice"
      type="button"
      aria-pressed={kind === 'reflection'}
      onclick={() => (kind = 'reflection')}
    >
      Reflection
    </button>
    <button
      class="choice"
      type="button"
      aria-pressed={kind === 'review'}
      onclick={() => (kind = 'review')}
    >
      Review
    </button>
  </div>

  <p class="hint">{SAYS[kind]}</p>

  {#if failure}
    <p class="note">That did not go through: {failure}</p>
    <p class="hint">
      Your notes are plain markdown in the vault regardless — <code>rb note</code> writes one from the
      terminal.
    </p>
  {/if}

  {#if kind === 'note'}
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
      <button class="act" type="button" onclick={oncancel}>Cancel</button>
      <button class="primary" type="button" onclick={write}>Write it</button>
    </div>
  {:else}
    <div class="bar">
      <button class="act" type="button" onclick={oncancel}>Cancel</button>
      <!-- *Open* and not *Create*: there may already be one, and the request is
           the same call either way. A verb that promised a new row and returned
           an old one would be the control lying about what it did. -->
      <button class="primary" type="button" onclick={anchored}>
        Open the {kind}
      </button>
    </div>
  {/if}
</div>

<style>
  .composer {
    max-width: var(--editor);
  }
  h2 {
    font-size: var(--t-lead);
    margin-bottom: var(--s-3);
  }
  .choices {
    margin-bottom: var(--s-2);
  }
  input,
  textarea {
    font: inherit;
    width: 100%;
    color: var(--ink);
    background: none;
    border: 0;
    border-bottom: 1px solid var(--line);
    padding: var(--s-2) 0;
    margin-bottom: var(--s-3);
  }
  /* The one box that stays a box. A composition surface needs an edge — it is
     the only thing on the page saying *your text goes in here* — where a title
     field directly under a heading does not, and reads as a rule under the
     heading instead. */
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--t-body);
    line-height: 1.75;
    min-height: 18rem;
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: var(--s-3);
    resize: vertical;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: var(--s-4);
  }
  .note {
    max-width: var(--column);
    margin: 0 0 var(--s-2);
  }
</style>
