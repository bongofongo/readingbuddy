<script lang="ts">
  import BookTile from '$lib/components/BookTile.svelte';
  import { client, type StoredBook } from '$lib/api/client';

  let books = $state<StoredBook[] | null>(null);
  let failure = $state<string | null>(null);

  // `$effect` rather than a `+page.ts` load: the data comes from an in-process
  // engine over Tauri's IPC, which does not exist during `vite build`, and a load
  // function is the one place SvelteKit might try to run it there.
  $effect(() => {
    client()
      .listBooks()
      .then((b) => (books = b))
      .catch((e) => (failure = e instanceof Error ? e.message : String(e)));
  });
</script>

<svelte:head><title>readingbuddy</title></svelte:head>

<h1>Library</h1>

{#if failure}
  <!-- A failure redirects. The repo's refusal-with-a-next-move shape: `ko pull`
       names `--new`, `calibre status` reports absence and prescribes nothing. -->
  <p class="note">
    The library did not open: {failure}
  </p>
  <p class="note dim">
    readingbuddy reads the library in <code>READINGBUDDY_DATA_DIR</code>, or the
    directory it was started in. <code>make dev-db</code> builds one to look at.
  </p>
{:else if books === null}
  <p class="note dim">Reading the shelf…</p>
{:else if books.length === 0}
  <!-- Idle is not blank. An empty state names the moves that fill it and never
       apologises. These are the two importers that need no network. -->
  <p class="note">Nothing on the shelf yet.</p>
  <p class="note dim">
    <code>rb epub &lt;file&gt;</code> adds a book from a file.
    <code>rb ko pull</code> takes what is on a connected reader.
  </p>
{:else}
  <!-- No count. Not here, and not in the header: this is the surface you land on,
       and a number on it is the completion framing `docs/decisions.md` bans. -->
  <div class="grid">
    {#each books as book (book.id)}
      <BookTile {book} />
    {/each}
  </div>
{/if}

<style>
  h1 {
    font-size: 1.05rem;
    color: var(--ink-dim);
    font-weight: 500;
    margin-bottom: 1rem;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(122px, 1fr));
    gap: 1.4rem 1rem;
  }
  .note {
    max-width: var(--measure);
    margin: 0 0 0.5rem;
  }
  .dim {
    color: var(--ink-dim);
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.85em;
    color: var(--accent);
  }
</style>
