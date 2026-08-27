<script lang="ts">
  /**
   * calibre, as a connection.
   *
   * ## Feature-detected, and it never asks anybody to install anything
   *
   * `docs/decisions.md` is explicit: *"Present → the features work; absent →
   * they aren't there. Never ask the user to install or configure it."* So both
   * tools absent is a **perfectly good answer** and this panel says what is
   * available rather than what is missing. There is no download link here and
   * there must not be one.
   *
   * ## Two tools, not one flag
   *
   * `ebook-convert` and `calibredb` are detected separately because a half
   * install degrades to the half that works. Converting and importing a library
   * are different capabilities and a single *calibre: yes/no* would switch off
   * the one you have.
   *
   * ## Dry run first, always
   *
   * The report has the same shape either way, so the panel shows what would
   * happen and then does it — the same promise the plugin install makes about
   * its destination path, applied to a library instead of to a volume.
   */
  import type { CalibreReportDto, CalibreStatusDto } from '$lib/api/bindings';

  let {
    status,
    preview = null,
    done = null,
    busy = false,
    onPreview,
    onImport,
  }: {
    status: CalibreStatusDto;
    /** What a dry run said would happen. `null` until one is asked for. */
    preview: CalibreReportDto | null;
    /** What the real import did. */
    done: CalibreReportDto | null;
    busy: boolean;
    onPreview: () => void;
    onImport: () => void;
  } = $props();

  const canRead = $derived(status.calibredb !== null);
  const canConvert = $derived(status.ebook_convert !== null);
</script>

<article class="calibre">
  <header><h3>calibre</h3></header>

  {#if !canRead && !canConvert}
    <!--
      Not an error, not an apology, and above all not an instruction. This
      machine does not have calibre; that is a fact about the machine, and
      readingbuddy's whole library works without it.
    -->
    <p class="hint">
      calibre is not on this computer, so readingbuddy is not offering to read its library or
      convert a file. Nothing else is affected — your books, your passages and your notes never
      needed it.
    </p>
  {:else}
    <p class="hint">
      readingbuddy found calibre and asks it questions; it never edits calibre's library.
      {#if canRead && canConvert}
        Both the library reader and the converter are here.
      {:else if canRead}
        The library reader is here. The converter, <code>ebook-convert</code>, is not — so
        converting a file is not offered.
      {:else}
        The converter is here. <code>calibredb</code> is not, so reading calibre's library is not offered.
      {/if}
    </p>

    <dl class="facts">
      {#if canRead}
        <div>
          <dt>calibredb</dt>
          <dd><code>{status.calibredb}</code></dd>
        </div>
      {/if}
      {#if canConvert}
        <div>
          <dt>ebook-convert</dt>
          <dd><code>{status.ebook_convert}</code></dd>
        </div>
      {/if}
    </dl>

    {#if canRead}
      {#if done !== null}
        <p class="result">
          Read {done.rows} rows out of calibre's library.
          {#if done.unmatched.length > 0}
            {done.unmatched.length === 1 ? 'One of them' : `${done.unmatched.length} of them`} could not
            be told apart from a book you already have, and was left for you.
          {/if}
        </p>
      {:else if preview !== null}
        <!-- What would happen, before it happens. Same shape of promise the
             plugin install makes about its path. -->
        <p class="result">
          calibre's default library has {preview.rows}
          {preview.rows === 1 ? 'row' : 'rows'} in it.
          {#if preview.unmatched.length > 0}
            {preview.unmatched.length}
            {preview.unmatched.length === 1 ? 'row' : 'rows'} could not be told apart from a book you
            already have; those are left for you either way.
          {/if}
          <!-- **No "yet"** — `axiom.test.ts` bans it, and the plain statement
               is the stronger promise anyway: this is a dry run and it wrote
               nothing, full stop. -->
          This was a look, not an import: readingbuddy has written nothing.
        </p>
      {/if}

      <div class="actions">
        {#if preview === null && done === null}
          <button onclick={onPreview} disabled={busy}>
            {busy ? 'Asking calibre…' : 'See what calibre has'}
          </button>
        {:else if done === null}
          <button class="primary" onclick={onImport} disabled={busy}>
            {busy ? 'Reading it…' : 'Bring it in'}
          </button>
          <button onclick={onPreview} disabled={busy}>Look again</button>
        {:else}
          <button onclick={onPreview} disabled={busy}>Look again</button>
        {/if}
      </div>
      <p class="hint">
        This reads calibre's own default library. readingbuddy keeps its own copy of what it finds —
        calibre still owns the files.
      </p>
    {/if}
  {/if}
</article>

<style>
  .calibre {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg-raised);
    padding: 1rem 1.1rem 0.9rem;
    min-width: 0;
  }
  h3 {
    font-size: 1.05rem;
  }
  header {
    margin-bottom: 0.5rem;
  }
  .facts {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(13rem, 100%), 1fr));
    gap: 0.55rem 1.2rem;
    margin: 0.6rem 0 0.8rem;
  }
  .facts div {
    min-width: 0;
  }
  dt {
    font-size: 0.72rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--ink-dim);
  }
  dd {
    margin: 0.1rem 0 0;
    font-size: 0.88rem;
    overflow-wrap: anywhere;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.82em;
    color: var(--ink-dim);
  }
  .result {
    margin: 0 0 0.7rem;
    font-size: 0.9rem;
    max-width: var(--column);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-bottom: 0.6rem;
  }
  button {
    font: inherit;
    font-size: 0.78rem;
    line-height: 1.4;
    color: var(--ink-dim);
    background: var(--bg);
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
    opacity: 0.6;
  }
  button.primary {
    color: var(--accent-on);
    background: var(--accent);
    border-color: transparent;
    font-weight: 600;
  }
  .hint {
    margin: 0 0 0.4rem;
  }
</style>
