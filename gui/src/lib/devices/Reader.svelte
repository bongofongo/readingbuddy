<script lang="ts">
  /**
   * One reader, plugged in or in a bag.
   *
   * ## The two halves, and why one card rather than two components
   *
   * A reader that is here and a reader that is away are the *same object* in
   * two states, and drawing them as two components is how the second becomes a
   * lesser one — a grey row at the bottom of the page that stops looking like a
   * thing you own. It is the same card; what changes is which verbs it has.
   *
   * ## What it never says
   *
   * No badge, no total across readers, and nothing framed as owed. The one
   * number on the card describes **this reader's own contents** — *3 books have
   * something new on them* — which is the same permission `/life`'s figures
   * have: a page you chose to open, in the past tense, about one thing. When
   * that number is zero it renders as nothing at all rather than as a zero.
   *
   * ## Every write is explicit and shows its path first
   *
   * `docs/decisions.md` requires the destination be shown *before* the install,
   * not after — a path shown afterwards is not the same promise. So the install
   * control is two steps: the verb, then the path plus a confirmation. There is
   * no automatic install anywhere in this app and this component is the one
   * most tempted to add one.
   */
  import type { DeviceScanDto } from '$lib/api/bindings';
  import { isHere, readerName, unreadable, waiting, type Reader } from './readers';
  import {
    conditionLabel,
    installIsPrimary,
    installVerb,
    obstruction,
    seenLabel,
    syncedLabel,
    waitingLabel,
  } from './words';

  let {
    reader,
    scan,
    busy = null,
    onInstall,
    onUninstall,
    onSync,
    onStats,
    onRename,
    onForget,
    onPull,
  }: {
    reader: Reader;
    /** What is on the volume. `null` until the scan answers, or if it refused. */
    scan: DeviceScanDto | null;
    /** The verb currently running against this reader, so a control can say so. */
    busy: string | null;
    onInstall: (mount: string) => void;
    onUninstall: (mount: string) => void;
    onSync: (mount: string) => void;
    onStats: (mount: string) => void;
    onRename: (deviceId: string, label: string) => void;
    onForget: (deviceId: string) => void;
    /**
     * Fetch over the LAN from a reader whose own window is open (item 15b).
     *
     * Offered for a reader that is **not** here, and that is the whole point:
     * with the volume in front of you the cable is better in every way, and a
     * wireless button beside a plugged-in reader is a slower path to the same
     * place. This is for the one in the other room.
     */
    onPull: (deviceId: string) => void;
  } = $props();

  const here = $derived(isHere(reader));
  const name = $derived(readerName(reader));
  const status = $derived(reader.mount?.status ?? null);
  const device = $derived(reader.device);

  const newHere = $derived(waitingLabel(waiting(scan)));
  const unread = $derived(unreadable(scan));

  const refusal = $derived(
    status === null
      ? null
      : obstruction(
          status.modified,
          status.unrecognised,
          status.installed_version,
          status.our_version,
        ),
  );
  const verb = $derived(status === null ? null : installVerb(status.condition));
  /**
   * Which control gets the fill. Exactly one, and it is whichever verb this
   * reader actually wants: the plugin when it needs writing, the sync when it
   * does not. Two accented buttons on one card is a card with no primary
   * action.
   */
  const writeFirst = $derived(status !== null && installIsPrimary(status.condition));

  /** Two steps for the install, so the path is on screen before anything writes. */
  let confirming = $state(false);
  /** The rename box, open only when asked for. A name is not a field you fall into. */
  let renaming = $state(false);
  let draft = $state('');

  function openRename() {
    draft = device?.label ?? '';
    renaming = true;
  }

  function commitRename(event: SubmitEvent) {
    event.preventDefault();
    if (device === null) return;
    onRename(device.device_id, draft);
    renaming = false;
  }
</script>

<article class="reader" class:away={!here}>
  <header>
    <h3>{name}</h3>
    {#if here}
      <!-- The one accent on the card, and it is spent on the state you can act
           on: this reader is in your hands right now. -->
      <span class="present">Plugged in</span>
    {/if}
  </header>

  {#if status !== null}
    <p class="state">{conditionLabel(status.condition)}</p>
  {/if}

  <dl class="facts">
    {#if here && reader.mount !== null}
      <div>
        <dt>Where</dt>
        <dd><code>{reader.mount.path}</code></dd>
      </div>
    {:else if device?.last_mount_path}
      <div>
        <dt>Last plugged in at</dt>
        <dd><code>{device.last_mount_path}</code></dd>
      </div>
    {/if}
    {#if device !== null}
      <div>
        <dt>Last in your hands</dt>
        <dd>{seenLabel(device.last_seen_at)}</dd>
      </div>
      <!-- A different fact from the line above it, and the page is the only
           place that difference is visible: plugging a reader in to charge it
           moves one and not the other. -->
      <div>
        <dt>Everything brought across</dt>
        <dd>{syncedLabel(device.last_synced_at)}</dd>
      </div>
      <div>
        <dt>Paired since</dt>
        <dd>{seenLabel(device.installed_at)}</dd>
      </div>
    {/if}
  </dl>

  {#if newHere !== null}
    <p class="waiting">{newHere}</p>
  {:else if here && scan !== null && scan.books.length > 0}
    <!-- *Nothing new* and *nothing on this reader* are different sentences and
         both are said. A device with no books at all falls through to neither. -->
    <p class="quiet">Everything on this reader is already here.</p>
  {/if}
  {#if unread > 0}
    <p class="quiet">
      {unread === 1 ? 'One book on it' : `${unread} books on it`} could not be read. The rest came across
      anyway.
    </p>
  {/if}

  {#if refusal !== null}
    <p class="refusal">{refusal}</p>
  {/if}

  {#if here && reader.mount !== null}
    {@const mount = reader.mount.path}
    {#if confirming && status !== null && verb !== null}
      <!-- The path, before anything is written. This is `decisions.md`'s
           requirement rendered, and it is why the install is two steps.

           **Outside `.actions`**, which is a flex row: as a sibling of the
           buttons this paragraph became a flex item, and the rendered page put
           a tall *Write it* block floating beside three lines of prose. The
           destination is a statement and the buttons are a row; they are not
           the same kind of thing and do not share a container. -->
      <p class="destination">
        readingbuddy will write into<br /><code>{status.plugin_dir}</code><br />and nothing else on
        this reader.
      </p>
    {/if}
    <div class="actions">
      {#if confirming && status !== null && verb !== null}
        <button class="primary" onclick={() => onInstall(mount)} disabled={busy !== null}>
          {busy === mount ? 'Writing…' : 'Write it'}
        </button>
        <button onclick={() => (confirming = false)}>Not now</button>
      {:else}
        {#if verb !== null}
          <button
            class:primary={writeFirst}
            onclick={() => (confirming = true)}
            disabled={busy !== null}
          >
            {verb}
          </button>
        {/if}
        <button class:primary={!writeFirst} onclick={() => onSync(mount)} disabled={busy !== null}>
          {busy === mount ? 'Reading it…' : 'Bring everything across'}
        </button>
        <button onclick={() => onStats(mount)} disabled={busy !== null}>
          Bring reading time across
        </button>
        {#if status?.installed}
          <button onclick={() => onUninstall(mount)} disabled={busy !== null}>
            Take the plugin off
          </button>
        {/if}
      {/if}
    </div>
  {/if}

  {#if device !== null}
    <div class="actions">
      {#if renaming}
        <form onsubmit={commitRename}>
          <label class="sr-only" for={`name-${device.device_id}`}>What to call this reader</label>
          <input
            id={`name-${device.device_id}`}
            bind:value={draft}
            placeholder={name}
            autocomplete="off"
          />
          <button class="primary" type="submit">Call it that</button>
          <button type="button" onclick={() => (renaming = false)}>Leave it</button>
        </form>
      {:else}
        <button onclick={openRename}>Give it a name</button>
        {#if !here}
          <button onclick={() => onPull(device.device_id)} disabled={busy !== null}>
            Fetch over wifi
          </button>
          <!-- Only offered for a reader that is *not* here. With the volume in
               front of you, taking the plugin off is the exact move and this
               one would leave a token behind on a device you are holding. -->
          <button onclick={() => onForget(device.device_id)}>Forget this reader</button>
        {/if}
      {/if}
    </div>
    {#if !here && !renaming}
      <p class="hint">
        Fetching needs the reader on the same network with its window open —
        <em>Tools → readingbuddy → Open the window</em>. It closes itself.
      </p>
      <p class="hint">
        Forgetting is only on this computer — the plugin stays on the reader, and readingbuddy
        cannot reach it from here. Plug it in and <em>Take the plugin off</em> removes it properly.
      </p>
    {/if}
    <p class="id"><code>{device.device_id}</code></p>
  {/if}
</article>

<style>
  .reader {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    background: var(--bg-raised);
    padding: 1rem 1.1rem 0.9rem;
    /*
     * A region has to read as a region and `--bg-raised` measures Lc 0.0
     * against `--bg` in both themes (`app.css` says so). So the border is doing
     * the work and the fill is nearly free — do not remove the border on the
     * theory that the background separates these cards, because it does not.
     */
    min-width: 0;
  }
  /*
   * A reader in a bag is the same object, quieter — never a lesser one. The
   * whole card is not dimmed; only the frame is, so the name and the facts
   * stay at full contrast and it does not read as disabled.
   */
  .away {
    border-style: dashed;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  h3 {
    font-size: var(--t-lead);
    /* The label is the one field a person controls, so it is the one that can
       be any length at all. It wraps rather than truncating: a name someone
       typed is not ours to cut. */
    overflow-wrap: anywhere;
    min-width: 0;
  }
  /* **Outlined, not filled** — the minimal pass's rule about the accent, which
     `app.css` states in full: a *fill* is the one action a surface is for, and
     everything else that is true right now is ink, a rule or an outline. A
     toggle that fills goes on filling once per row, and a list with six brass
     boxes down it has spent the colour that was supposed to point at one thing.
     The outline is `--accent-text` rather than `--accent`: it carries a word,
     and raw brass measures 2.78:1 on the light theme. */
  .present {
    font-size: var(--t-micro);
    font-weight: 600;
    color: var(--accent-text);
    border: 1px solid var(--accent-text);
    border-radius: var(--radius);
    padding: 0 var(--s-1);
    white-space: nowrap;
  }
  .state {
    margin: 0.35rem 0 0.7rem;
    font-size: var(--t-fine);
    color: var(--ink-dim);
  }
  /*
   * **Two columns, and there are four facts.** `auto-fit` gave three inside a
   * wide card, which put *Paired since* alone on a second row and wrapped
   * *Everything brought across* under a heading that had room — a ragged block
   * of four things arranged as 3 + 1. Two is the shape the content has, and it
   * also leaves each column wide enough that a mount path stops breaking
   * mid-word. One column below the fold, where two would be 8rem each.
   */
  .facts {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.55rem 1.2rem;
    margin: 0 0 0.75rem;
  }
  @media (min-width: 26rem) {
    .facts {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  .facts div {
    min-width: 0;
  }
  dt {
    font-size: var(--t-micro);
    color: var(--ink-dim);
  }
  dd {
    margin: 0.1rem 0 0;
    font-size: var(--t-fine);
    overflow-wrap: anywhere;
  }
  code {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 0.82em;
    color: var(--ink-dim);
  }
  /*
   * The one figure on the card. It is accent *text* rather than an accent fill:
   * a fill says "press me", and this is something true about the reader rather
   * than a control.
   */
  .waiting {
    margin: 0 0 0.6rem;
    font-size: var(--t-fine);
    color: var(--accent-text);
  }
  .quiet {
    margin: 0 0 0.6rem;
    font-size: var(--t-fine);
    color: var(--ink-dim);
  }
  /*
   * A refusal, and deliberately not styled as an error — nothing has gone
   * wrong. Somebody edited a file on their own reader and readingbuddy is
   * respecting that. It reads as a note, with the move in it.
   */
  .refusal {
    margin: 0 0 0.7rem;
    padding: 0.5rem 0.7rem;
    border-left: 2px solid var(--line);
    font-size: var(--t-fine);
    color: var(--ink-dim);
    max-width: var(--column);
  }
  .destination {
    margin: 0 0 0.6rem;
    font-size: var(--t-fine);
    color: var(--ink-dim);
    line-height: 1.5;
    max-width: var(--column);
  }
  .destination code {
    color: var(--ink);
    overflow-wrap: anywhere;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: 0.5rem;
  }
  form {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    width: 100%;
  }
  input {
    font: inherit;
    font-size: var(--t-fine);
    flex: 1 1 12rem;
    min-width: 0;
    padding: 0.25rem 0.5rem;
    color: var(--ink);
    background: var(--bg);
    border: 1px solid var(--line);
    border-radius: var(--radius);
  }
  button {
    font: inherit;
    font-size: var(--t-micro);
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
    margin: 0.55rem 0 0;
    font-size: var(--t-micro);
  }
  /*
   * The full id, small and last. It is not a name — it is what `rb ko plugin
   * forget` takes, and the reason it is on the card at all is that this page
   * and that command have to be able to talk about the same reader.
   */
  .id {
    margin: 0.6rem 0 0;
    font-size: var(--t-micro);
    overflow-wrap: anywhere;
  }
</style>
