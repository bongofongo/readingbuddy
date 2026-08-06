<script lang="ts">
  /**
   * The shelf — the home surface (item 26).
   *
   * Two bands: what you are reading, pulled proud, and everything else in
   * whichever arrangement you left it in. The arrangement is a value from
   * `$lib/shelf/layouts`, not a shape baked into this file — see that module
   * for why, and for where the deferred spine shelf plugs in.
   */
  import BookTile from '$lib/components/BookTile.svelte';
  import { client, type OpenReading, type StoredBook } from '$lib/api/client';
  import Moment from '$lib/moments/Moment.svelte';
  import ShelfSwitch from '$lib/shelf/ShelfSwitch.svelte';
  import { layoutById, recallLayout, rememberLayout, type ShelfLayoutId } from '$lib/shelf/layouts';

  let books = $state<StoredBook[] | null>(null);
  let reading = $state<OpenReading[]>([]);
  let failure = $state<string | null>(null);

  let layoutId = $state<ShelfLayoutId>(recallLayout());
  // Capitalised, because that is what marks it a component rather than an HTML
  // element at the call site — `<layout.component />` would be parsed as a tag.
  const Layout = $derived(layoutById(layoutId).component);

  function pick(id: ShelfLayoutId) {
    layoutId = id;
    rememberLayout(id);
  }

  // `$effect` rather than a `+page.ts` load: the data comes from an in-process
  // engine over Tauri's IPC, which does not exist during `vite build`, and a load
  // function is the one place SvelteKit might try to run it there.
  $effect(() => {
    const c = client();
    c.listBooks()
      .then((b) => (books = b))
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)));
    // Its own call, and its failure is not the shelf's. The strip is an
    // ornament on the library; a library that loaded must not be replaced by an
    // error because the thing above it did not.
    c.currentlyReading()
      .then((r) => (reading = r))
      .catch(() => (reading = []));
  });
</script>

<svelte:head><title>readingbuddy</title></svelte:head>

<!--
  The moment, above the shelf and above the failure below it.

  It is the app noticing something rather than the shelf reporting something, so
  it is not inside the `{#if}` that decides whether a library loaded: a moment is
  still true when the database will not open, and the two failures are unrelated.
  It renders nothing when there is nothing, which is most of the time — item 23's
  `moment_epoch` means a freshly seeded library legitimately has none.

  Mounted here rather than in the layout on purpose. This is where the app greets
  you and where the wall of finished books will eventually be, and it also gives
  "poll again after a write that could mint one" for free: the writes are on the
  book view, and coming back here remounts this and asks again.
-->
<Moment />

<h1>Library</h1>

{#if failure}
  <!-- A failure redirects. The repo's refusal-with-a-next-move shape: `ko pull`
       names `--new`, `calibre status` reports absence and prescribes nothing. -->
  <p class="note">
    The library did not open: {failure}
  </p>
  <p class="hint">
    readingbuddy reads the library in <code>READINGBUDDY_DATA_DIR</code>, or the
    directory it was started in. <code>make dev-db</code> builds one to look at.
  </p>
{:else if books === null}
  <p class="hint">Reading the shelf…</p>
{:else if books.length === 0}
  <!-- Idle is not blank. An empty state names the moves that fill it and never
       apologises. These are the two importers that need no network. -->
  <p class="note">Nothing on the shelf yet.</p>
  <p class="hint">
    <code>rb epub &lt;file&gt;</code> adds a book from a file.
    <code>rb ko pull</code> takes what is on a connected reader.
  </p>
{:else}
  {#if reading.length > 0}
    <!-- Pulled proud. Which books these are is the engine's answer
         (`currently_reading`), not a filter spelled a second time here. -->
    <section class="band reading-band">
      <h2 class="band-title">Reading</h2>
      <div class="proud">
        {#each reading as { book, reading: r } (r.id)}
          <BookTile {book} proud />
        {/each}
      </div>
    </section>
  {/if}

  <section class="band">
    <div class="band-head">
      <h2 class="band-title">On the shelf</h2>
      <ShelfSwitch current={layoutId} onpick={pick} />
    </div>
    <!-- No count. Not here and not in the header: this is the surface you land
         on, and a number on it is the completion framing `docs/decisions.md`
         bans. The switch says which arrangement is on and nothing about size. -->
    {#key layoutId}
      <Layout {books} />
    {/key}
  </section>
{/if}

<style>
  h1 {
    font-size: 1.05rem;
    color: var(--ink-dim);
    font-weight: 500;
    margin-bottom: 1.4rem;
  }
  .band + .band {
    margin-top: 2.2rem;
  }
  .band-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    flex-wrap: wrap;
    margin-bottom: 1rem;
  }
  /* The heading itself is `.band-title` in `app.css` now — item 27 promoted it
     when the book view needed the same voice. Only the spacing is this page's. */
  h2 {
    margin-bottom: 1rem;
  }
  .band-head h2 {
    margin-bottom: 0;
  }
  /*
   * A strip, not a grid — and a different *kind* of object, not a bigger tile.
   *
   * Size alone does not carry hierarchy: at 13% larger it read as a second grid
   * that happened to be above the first, and at 720px the two sizing laws
   * crossed over and the "proud" band rendered *smaller* than the shelf below
   * it. So the strip now sits on its own ground with a shelf rule under it, and
   * its track is a clamp that is wider than the grid's column at every width
   * rather than a `minmax` that competes with it for space.
   */
  .reading-band {
    background: var(--bg-raised);
    border-bottom: 1px solid var(--line);
    /* Bleeds to the window edge, which is what makes it a ground rather than a
       card. `main` has 1.25rem of padding; this cancels and reapplies it. */
    margin-inline: -1.25rem;
    padding: 0 1.25rem 1.1rem;
  }
  .reading-band h2 {
    padding-top: 1.1rem;
  }
  .proud {
    display: grid;
    grid-auto-flow: column;
    /* Never narrower than a shelf column at the same width. The floor is what
       stops the inversion; the ceiling stops one book filling the window. */
    grid-auto-columns: clamp(150px, 42vw, 200px);
    gap: 1.2rem;
    overflow-x: auto;
    padding-bottom: 0.3rem;
    scrollbar-width: thin;
    /* The clip has to look deliberate. Snapping lands a tile at the gutter
       rather than mid-word, the padding keeps that landing off the edge, and
       the mask says "there is more" where a hard cut said "bug". */
    scroll-snap-type: x proximity;
    scroll-padding-inline: 0 1.25rem;
    -webkit-mask-image: linear-gradient(to right, #000 calc(100% - 2rem), transparent);
    mask-image: linear-gradient(to right, #000 calc(100% - 2rem), transparent);
  }
  .proud > :global(a) {
    scroll-snap-align: start;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
</style>
