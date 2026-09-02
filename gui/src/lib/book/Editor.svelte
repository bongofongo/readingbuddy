<script lang="ts">
  /**
   * One note, open on the work surface — the surface this whole redesign is for.
   *
   * What it replaces: a `rows="10"` textarea inside a 68ch column inside a band
   * inside the left half of a two-column page. Same component, three times the
   * room, and the connections now sit *beside* it instead of behind a button.
   *
   * ## Markdown as markdown, and the type is bigger than it was
   *
   * Never a rich-text editor: the file in the vault is the origin and Obsidian is
   * the other thing editing it, so a WYSIWYG surface here would be a second
   * opinion about the bytes. **Monospace stays on that argument** rather than on
   * the aesthetic one — the general case is genuinely arguable (monospace costs
   * 20–30% horizontal space and is measurably slower to read, and notes get read
   * more often than they get written), and the file format is the local fact
   * that settles it.
   *
   * The size is a straight correction. This was `0.9rem` — ~14.4px, the
   * **smallest body text in the application**, on the surface the page exists
   * for. That is a dense-UI size applied to a long-form composition surface, and
   * the two have opposite requirements: eye-tracked reading studies find fixation
   * duration falling continuously up to 22pt and comprehension worse at 10–12pt
   * than at 18pt. 1rem with the leading at 1.75 costs nothing here, because the
   * column is capped anyway.
   *
   * ## The title is read-only, and that is an API gap rather than a decision
   *
   * `updateNoteBody` exists; **nothing on the wire sets a title.** The redesign
   * drafted an inline editable title and it is not buildable today. So the title
   * is stated rather than offered — a control that silently did nothing would be
   * worse — and renaming a note is an engine item.
   *
   * ## Saving says where the file went
   *
   * `Saved · in your vault as trees-as-a-time-scale.md` — naming the file is the
   * app telling you it did not capture your writing. The failure state below says
   * the same thing in words when a write is refused; this says it in the ordinary
   * case too, which is the case a reader is actually in.
   *
   * ## The connections came inside
   *
   * `Connections` used to be the page's right rail and is now drawn under this
   * editor, because the rail's own justification was that the *Link to…* search
   * is **an instrument acting on this box** rather than reference material beside
   * it. An instrument belongs with the thing it acts on: it appears when a note
   * is open, it is about *this* note without being told which, and it goes when
   * the note does.
   *
   * The plumbing got shorter with it. The editor used to hand a writer out
   * through `onready` so the page could give it to the rail; now it holds the
   * writer itself and passes it one component down. Nothing above this file
   * knows how the text is held, which was the point of the callback in the first
   * place.
   */
  import type { HighlightDto, NoteDto, RatingDto, RatingScaleDto } from '$lib/api/bindings';
  import { client } from '$lib/api/client';
  import { noteKindLabel, ratingLabel, trimNumber } from '$lib/phrasing';

  import Connections from './Connections.svelte';
  import { ratingSteps } from './rating';

  let {
    note,
    cited,
    onreload,
    onclose,
    onopennote,
    onshowpassage,
  }: {
    note: NoteDto;
    /** The passages this note quotes — handed straight down to `Connections`. */
    cited: HighlightDto[];
    onreload: () => Promise<void>;
    onclose: () => void;
    onopennote: (id: number) => void;
    onshowpassage: (id: number) => void;
  } = $props();

  let body = $state('');
  let savedBody = $state('');
  let rating = $state<RatingDto | null>(null);
  let scale = $state<RatingScaleDto | null>(null);
  let failure = $state<string | null>(null);
  let box = $state<HTMLTextAreaElement | null>(null);

  const dirty = $derived(body !== savedBody);
  const steps = $derived(ratingSteps(scale));
  /** The file, not the path: the vault root is on the settings surface, not here. */
  const filename = $derived(note.file_path.split('/').pop() ?? note.file_path);

  function failed(e: unknown) {
    failure = e instanceof Error ? e.message : String(e);
  }

  /**
   * Which note's body is in the box.
   *
   * A plain `let` and **not** `$state`, on purpose: it is a guard against
   * reloading, so an effect that read it as a dependency would re-run itself
   * every time it wrote it. Nothing renders it.
   */
  let loadedFor: number | null = null;

  $effect(() => {
    const id = note.id;
    if (id === loadedFor) return;
    loadedFor = id;
    failure = null;
    client()
      .noteBody(id)
      .then((text) => {
        // Only if it is still the note being asked about — a fast second click
        // must not drop an older body into a newer note's box.
        if (loadedFor !== id) return;
        body = savedBody = text;
      })
      .catch(failed);
  });

  $effect(() => {
    // A rating belongs to a **review**, never to a book and never to a note of
    // another kind. Asking for one anywhere else would be this screen inventing
    // a place for it that the schema does not have.
    if (note.kind !== 'review') {
      rating = null;
      return;
    }
    const api = client();
    Promise.all([api.reviewRating(note.id), api.activeRatingScale()])
      .then(([r, s]) => {
        rating = r;
        scale = s;
      })
      .catch(failed);
  });

  /**
   * How `Connections` writes a `[[wikilink]]` into this box.
   *
   * `null` until the textarea is in the DOM, so the *Link to…* search cannot
   * write into a box that is not there — the same guard the `onready` callback
   * used to make across a component boundary, now a derived value inside the one
   * component that owns the element.
   */
  const writer = $derived.by(() => {
    // Bound to a local rather than closing over `box`: the closure re-reads the
    // state on every call, so narrowing it at construction says nothing about
    // what it holds when the link search fires — and svelte-check is right to
    // say so. The element captured here is the one that was mounted when this
    // writer was made, which is exactly the guarantee wanted.
    const el = box;
    return el === null ? null : (text: string) => insert(el, text);
  });

  /**
   * Put text at the cursor, or over the selection.
   *
   * Through the element rather than by rebuilding the string, because the caret
   * has to end up *after* what was inserted — a plain `body = a + text + b`
   * leaves the caret where the browser last put it, which on every insert but
   * the first is the wrong place.
   */
  function insert(el: HTMLTextAreaElement, text: string) {
    const start = el.selectionStart;
    const end = el.selectionEnd;
    body = body.slice(0, start) + text + body.slice(end);
    el.focus();
    // After the write has landed in the DOM; setting it now would be overwritten
    // by Svelte's own update of `value`.
    queueMicrotask(() => el.setSelectionRange(start + text.length, start + text.length));
  }

  async function save() {
    const text = body;
    try {
      // Rewrites the file **and** reindexes the wikilink edges, which is why the
      // links rail beside it can be trusted right after an edit.
      await client().updateNoteBody(note.id, text);
      savedBody = text;
      await onreload();
    } catch (e) {
      failed(e);
    }
  }

  async function remove() {
    try {
      await client().deleteNote(note.id);
      onclose();
      await onreload();
    } catch (e) {
      failed(e);
    }
  }

  async function rate(value: number) {
    try {
      await client().setRating(note.id, value);
      rating = await client().reviewRating(note.id);
    } catch (e) {
      failed(e);
    }
  }

  async function unrate() {
    try {
      await client().clearReviewRating(note.id);
      rating = null;
    } catch (e) {
      failed(e);
    }
  }
</script>

<div class="editor">
  <div class="head">
    <h2>{note.title}</h2>
    {#if noteKindLabel(note.kind)}
      <span class="kind">{noteKindLabel(note.kind)}</span>
    {/if}
  </div>

  {#if failure}
    <!-- A failure says what was refused and names the thing that still works.
         The vault is markdown on disk either way, which is the whole point of it
         being markdown on disk. -->
    <p class="note">That did not go through: {failure}</p>
    <p class="hint">
      Your notes are plain markdown in the vault regardless — <code>rb notes</code> lists them and
      <code>rb note</code> writes one.
    </p>
  {/if}

  <textarea
    bind:this={box}
    bind:value={body}
    spellcheck="true"
    aria-label="Note body"
    placeholder={'Write. A [[wikilink]] makes an edge — even to a note that does not exist.'}
  ></textarea>

  <div class="bar">
    <p class="saved">
      {#if dirty}
        <!-- Not "unsaved changes", which is a warning about a state that is
             perfectly ordinary. It says where the file is and that this is not
             in it. -->
        Not saved to <code>{filename}</code>
      {:else}
        Saved · in your vault as <code>{filename}</code>
      {/if}
    </p>
    <button type="button" onclick={remove}>Delete</button>
    <button type="button" class="primary" disabled={!dirty} onclick={save}>
      {dirty ? 'Save' : 'Saved'}
    </button>
  </div>

  {#if note.kind === 'review'}
    <!-- A review is the one kind that carries a rating: it is written for other
         people, and the reflection deliberately never gets one. -->
    <div class="rating">
      <span class="rating-label">Rating</span>
      {#if steps.length > 0}
        {#each steps as v (v)}
          <button
            type="button"
            class="point"
            class:on={rating !== null && rating.value === v}
            aria-pressed={rating !== null && rating.value === v}
            onclick={() => rate(v)}
          >
            {trimNumber(v)}
          </button>
        {/each}
        {#if rating !== null}
          <button type="button" class="clear" onclick={unrate}>Clear</button>
        {/if}
      {:else if rating !== null}
        <!-- A scale nothing can draw as a row still has a value to report.
             Saying so beats offering four hundred boxes. -->
        <span class="recorded">{ratingLabel(rating)}</span>
      {:else}
        <span class="hint">
          No scale this app can offer as a row — <code>rb rating scale</code> defines one.
        </span>
      {/if}
    </div>
  {/if}

  <!-- The note's own material, under the note. It is not a rail and it is not
       behind a button: it is what this note links to and quotes, which only
       exists while this note is open. -->
  <Connections
    {note}
    {cited}
    oninsert={writer}
    {onopennote}
    {onshowpassage}
  />
</div>

<style>
  .editor {
    max-width: var(--editor);
  }
  .head {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-bottom: 0.7rem;
  }
  h2 {
    font-size: var(--t-lead);
    min-width: 0;
    overflow-wrap: anywhere;
    /* Space, and deliberately no rule under it: a rule here is what an editable
       field looks like, and this title cannot be edited (see the header — there
       is no rename on the wire). The gap is the seam. */
    padding-bottom: 0.35rem;
  }
  .kind {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    flex: none;
  }
  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    /* See the header: the largest body text in the app, not the smallest. */
    font-size: var(--t-body);
    line-height: 1.75;
    width: 100%;
    min-height: 26rem;
    color: var(--ink);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.9rem 1rem;
    resize: vertical;
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-top: 0.6rem;
  }
  .saved {
    margin: 0 auto 0 0;
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  button {
    font: inherit;
    font-size: var(--t-micro);
    line-height: 1.4;
    color: var(--ink-dim);
    background: var(--bg-raised);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 0.25rem 0.75rem;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    color: var(--ink);
  }
  button:disabled {
    cursor: default;
  }
  /*
   * `Saved` is a status readout wearing a control, and the two want opposite
   * treatments: a disabled button should recede, and the only statement on this
   * page that the note reached the vault must not. Inheriting a 0.55 opacity put
   * it at 2.31:1 — under the 4.5:1 text needs and under even the 3:1 non-text
   * floor — which made the one place this screen tells you what you did the
   * least legible thing on it.
   */
  button.primary:disabled {
    color: var(--ink-dim);
    border-color: var(--line);
  }
  button.primary:not(:disabled) {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
    font-weight: 600;
  }
  .rating {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    flex-wrap: wrap;
    margin-top: 1.4rem;
  }
  .rating-label {
    font-size: var(--t-micro);
    color: var(--ink-dim);
    margin-right: 0.3rem;
  }
  .point {
    min-width: 2.2rem;
    text-align: center;
  }
  /* The selected point is the one that has to read best. It used to be a brass
     fill, on a surface whose *Save* button is also a brass fill — two of them a
     few rems apart, one an action and one a value you already set. */
  /* **Outlined, not filled** — the minimal pass's rule about the accent, which
     `app.css` states in full: a *fill* is the one action a surface is for, and
     everything else that is true right now is ink, a rule or an outline. A
     toggle that fills goes on filling once per row, and a list with six brass
     boxes down it has spent the colour that was supposed to point at one thing.
     The outline is `--accent-text` rather than `--accent`: it carries a word,
     and raw brass measures 2.78:1 on the light theme. */
  .point.on {
    color: var(--accent-text);
    border-color: var(--accent-text);
    font-weight: 600;
  }
  .recorded {
    font-size: var(--t-fine);
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
</style>
