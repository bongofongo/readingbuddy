<script lang="ts">
  /**
   * Reading now — the entrance, and the calmest surface in the app.
   *
   * ## Why this is its own page, and why it is the one the app opens on
   *
   * The wall of everything used to be here with the open books as a band on top
   * of it. Those are two questions with two different shapes: *what am I
   * reading* has two or three answers and you ask it every day, and *what have I
   * read* has hundreds and you ask it when you are browsing. Answering both on
   * one surface made the first a strip and the second a scroll past a preamble.
   *
   * Split, each gets what it needs. This page is the daily one, so it opens the
   * app; `/library` is the wall, and it is a place you go.
   *
   * The brief's two halves — *"opening the app should be calming, with less
   * happening and more whitespace"* and *"the single book page is where the time
   * goes"* — are better served by the split than by the band: this page is now
   * two or three previews and a lot of nothing, which is as quiet as a surface
   * that says anything can be.
   *
   * ## A click here goes into reading mode, and that is the point of it
   *
   * Every book on this page has an open reading by construction, and the thing
   * you do with a book you are in the middle of is read it. So a preview leads
   * to `/reading?book=…` rather than to the book's page — `$lib/nav.ts` holds
   * that rule, because the wall's tiles follow it too for the books that are
   * still open.
   *
   * The book's own page is one link away on every preview, which is what keeps
   * this from being a fork somebody has to get right.
   *
   * ## Two requests, and the second is an N+1
   *
   * `currentlyReading` for the page, and then the newest mark per open reading —
   * for which there is **no request**, so it is `listHighlights` plus
   * `listNotes` each, fetching every highlight in a book to render one line.
   * It works and it is wrong; `$lib/library/latest.ts` records why it is left
   * visible rather than hidden behind a client-side aggregate.
   */
  import { client, type OpenReading } from '$lib/api/client';
  import { latestMark, ordered, type Preview as PreviewOf } from '$lib/library/latest';
  import Preview from '$lib/library/Preview.svelte';
  import Moment from '$lib/moments/Moment.svelte';

  let previews = $state<PreviewOf<OpenReading>[] | null>(null);
  let failure = $state<string | null>(null);

  const open = $derived(previews === null ? [] : ordered(previews));

  // `$effect` rather than a `+page.ts` load: the data comes from an in-process
  // engine over Tauri's IPC, which does not exist during `vite build`, and a load
  // function is the one place SvelteKit might try to run it there.
  $effect(() => {
    load().catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });

  /**
   * The open readings, each with the newest thing written against it.
   *
   * Which readings these are is the engine's answer (`currently_reading`, a
   * selection predicate — item 17), and the order is `ordered`'s. Nothing here
   * decides either.
   */
  async function load(): Promise<void> {
    const api = client();
    const reading = await api.currentlyReading();
    previews = await Promise.all(
      reading.map(async (r) => {
        const [hs, ns] = await Promise.all([
          api.listHighlights(r.book.id),
          api.listNotes(r.book.id),
        ]);
        return { reading: r, mark: latestMark(hs, ns), touched: r.reading.last_modified };
      }),
    );
  }
</script>

<svelte:head><title>readingbuddy</title></svelte:head>

<!--
  The moment, above the page and above the failure below it.

  It is the app noticing something rather than the shelf reporting something, so
  it is not inside the `{#if}` that decides whether the readings loaded: a moment
  is still true when the database will not open, and the two failures are
  unrelated. It renders nothing when there is nothing, which is most of the time.
-->
<Moment />

<!-- The page's name is in the nav, where the shell says where you are — so this
     heading is for the document outline and for a screen reader, and takes no
     space on a surface whose whole brief is calm. -->
<h1 class="sr-only">Reading now</h1>

{#if failure}
  <!-- A failure redirects. The repo's refusal-with-a-next-move shape: `ko pull`
       names `--new`, `calibre status` reports absence and prescribes nothing. -->
  <p class="note">The library did not open: {failure}</p>
  <p class="hint">
    readingbuddy reads the library in <code>READINGBUDDY_DATA_DIR</code>, or the
    directory it was started in. <code>make dev-db</code> builds one to look at.
  </p>
{:else if previews === null}
  <p class="hint">Opening…</p>
{:else if open.length === 0}
  <!-- Idle is not blank, and an empty state names the moves that fill it. No
       "yet": a library with nothing open is a fact about today, not an omission
       somebody owes. -->
  <p class="note">No book is open.</p>
  <p class="hint">
    <a href="/library">The library</a> is where you start one — a book's page has the
    button. <code>rb ko pull</code> takes what is on a connected reader, and a page
    turn from a device opens a reading too.
  </p>
{:else}
  <section class="open">
    <!-- No count and no heading over the previews. The nav already says where
         you are, and a number here would be the one thing this surface is for
         turned into a tally of what is unfinished. -->
    <div class="previews">
      {#each open as p (p.reading.reading.id)}
        <Preview book={p.reading.book} reading={p.reading.reading} mark={p.mark} />
      {/each}
    </div>
  </section>
{/if}

<style>
  /*
   * A wrapping grid on the page's own ground.
   *
   * `auto-fit`, so two open books use the width they are given rather than
   * leaving a hole where a third is not — the page is however many books you
   * have open, and most of the time that is two.
   */
  .previews {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(310px, 1fr));
    gap: 2rem;
  }
  @media (max-width: 680px) {
    .previews {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .open {
    margin-top: 1rem;
  }
  .note {
    max-width: var(--column);
    margin: 0 0 0.5rem;
  }
  .hint a {
    color: var(--accent-text);
  }
</style>
