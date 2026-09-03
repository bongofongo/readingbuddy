<script lang="ts">
  /**
   * Cards — the last few reads that ended, and a door to all of them.
   *
   * ## What this page is, after the minimal pass
   *
   * It used to be the wall: every card in the library, behind two rows of pills
   * — *Show* with `All`, one chip per year and `Still reading`, and *Order* with
   * three — over twenty-four bordered boxes, a total and a pager. Roughly
   * fourteen controls before a single card.
   *
   * Each of those was argued for on its own and the page was still wrong,
   * because it was answering two questions at once. *What did I last finish* has
   * six answers, needs no control, and is the one you have when you open the
   * app. *Everything I have ever read, arranged how I like* has hundreds and is
   * the reason the pills exist. A surface that answers two questions is two
   * surfaces — so the second one is `/cards/history`, and it took every control
   * with it.
   *
   * What is left here is the **narrow interface** over a deep thing: one
   * request, no parameters a reader can set, and one link out to the page that
   * owns the parameters. The door is not a compromise, it is the mechanism —
   * arriving at the history page through it is what makes the count there
   * something you asked for.
   *
   * ## One request, and no count
   *
   * `listReadingRows` with a stated limit and no filter. `countReadings` is
   * **not** called: a total on this page would be an aggregate about the library
   * on a surface the reader did not choose to open for one, which is the line
   * `gui/CLAUDE.md` draws. The history page counts, because you went there.
   *
   * Nothing here says how many are behind the door either — *and 92 more* is the
   * same number wearing a preposition.
   *
   * ## Why `finished` and not `last_modified`
   *
   * A card is minted when a read ends, so the newest cards are the reads that
   * ended most recently. `last_modified` would reorder this page every time a
   * note was written against an old book, which makes *what did I last finish*
   * answer something else. Open reads have no `finished_at` and sort last, which
   * is right: they have no card-worthy ending to show.
   */
  import { client, type ReadingRow } from '$lib/api/client';
  import Card from '$lib/card/Card.svelte';

  /**
   * How many cards this page draws.
   *
   * Six, and the number is chosen to be a **glance** rather than a page: two
   * rows at a desktop width, one screen at any width, and nothing below the fold
   * that a reader has to decide whether to scroll for. `limit` is required on
   * the wire and has no serde default, so a client states a real number — see
   * `$lib/cards/wall.ts`, which owns the history page's page size for the same
   * reason and is deliberately not shared with this one. Two surfaces, two
   * decisions.
   */
  const RECENT = 6;

  let rows = $state<ReadingRow[] | null>(null);
  let failure = $state<string | null>(null);

  // `$effect` rather than a `+page.ts` load: the data comes from an in-process
  // engine over Tauri's IPC, which does not exist during `vite build`, and a load
  // function is the one place SvelteKit might try to run it there.
  $effect(() => {
    client()
      .listReadingRows({ limit: RECENT, sort: 'finished', offset: 0, filter: null })
      .then((rs) => (rows = rs))
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });
</script>

<svelte:head><title>Cards — readingbuddy</title></svelte:head>

<!-- The page's name is in the nav, where the shell says where you are — so this
     heading is for the document outline and for a screen reader, and takes no
     space on a surface whose whole brief is calm. `/cards/history` draws its
     heading, because the nav marks *Cards* current on both and the two pages
     would otherwise be indistinguishable by name. -->
<h1 class="sr-only">Cards</h1>

{#if failure}
  <!-- A failure redirects: say what was refused and name the thing that works.
       No CLI command — this screen's audience is a reader with no terminal in
       the window. -->
  <p class="note">These cards did not open: {failure}</p>
  <p class="hint">
    Every card is also on the book that minted it — the <a href="/library">library</a> is the way
    there.
  </p>
{:else if rows === null}
  <p class="hint">Reading the cards…</p>
{:else if rows.length === 0}
  <!-- Idle is not blank. It says what a card is and where one comes from, and it
       names no command. The door is not drawn: a link to every card is a lie
       when there are none, and the move that fills this page is reading. -->
  <p class="note">No cards here.</p>
  <p class="hint">
    A card is one read of one book — its cover, its dates, and a passage you marked. Reading
    something in the <a href="/library">library</a> is what mints one.
  </p>
{:else}
  <div class="recent">
    {#each rows as row (row.reading.id)}
      <Card {row} />
    {/each}
  </div>

  <!-- The whole interface of this page, and it is a door rather than a control:
       it changes nothing here, it goes somewhere that owns something this page
       does not. No figure on it — *and 92 more* is a count of what you have not
       looked at. -->
  <p class="out"><a class="door" href="/cards/history">Every card →</a></p>
{/if}

<style>
  /*
   * Three columns, and it is a fixed count rather than the wall's `auto-fill`.
   *
   * The wall does not know how many cards it has, so it fills whatever tracks
   * fit and lets the last row be short. This page has **exactly six**, and six
   * into four columns is 4 + 2 — a ragged second row on a surface whose whole
   * claim is that it is composed. Six into three is two full rows at a desktop
   * width, three at a tablet and six at a phone, and every one of those is a
   * complete rectangle. A fixed column count is only available *because* the
   * count is fixed, which is the same reason the wall may not have one.
   */
  .recent {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--s-5) var(--s-5);
  }
  @media (max-width: 900px) {
    .recent {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (max-width: 560px) {
    .recent {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .out {
    margin: var(--s-5) 0 0;
  }
  .note {
    max-width: var(--column);
    margin: 0 0 var(--s-2);
  }
  /* The move out of an empty state, in the accent — it is the only link in the
     sentence and it has to look like one. */
  .hint a {
    color: var(--accent-text);
  }
</style>
