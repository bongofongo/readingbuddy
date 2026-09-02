<script lang="ts">
  /**
   * Devices — every reader readingbuddy knows, and every way to connect one.
   *
   * The fifth place, and the first one that is about the app's edges rather
   * than about a book. It answers three questions with no clicking: which
   * readers do I own, what is the state of my reading on this one, and how else
   * could I connect.
   *
   * ## Scanning is automatic; writing never is
   *
   * `docs/decisions.md` splits those deliberately — *mount → import is
   * automatic and read-only, mount → install is explicit and shows the path* —
   * and this page is where the split is visible. Arriving on it lists mounts,
   * reads each plugin's status and scans each volume, all of which are reads.
   * Nothing writes to a reader until somebody presses something, and the
   * install control shows its destination first. **Do not add an auto-install
   * here**; it is the single most tempting change to this file and it would undo
   * the decision the whole plugin design rests on.
   *
   * ## Three requests, and the join is not the mount path
   *
   * `pairedDevices` (readers you own), `candidateMounts` (volumes plugged in)
   * and `pluginStatus` per mount — which is the only thing that reads a
   * `device_id` off a volume, and therefore the only thing that can join the
   * other two. `last_mount_path` looks like it would do the job and must not:
   * migration `0019` says mount points move, so it is a sentence about the past
   * and two Kobos plugged in one after another share it.
   *
   * The join itself lives in `$lib/devices/readers.ts` so its cases — four
   * volumes, a reader in a bag, a volume carrying somebody else's pairing — are
   * reachable from vitest rather than only from a screenshot.
   *
   * ## Counts, and the ones that are still forbidden
   *
   * A number describing *one reader's own contents* is allowed here for
   * `/life`'s reason: a page you chose to open, past tense, about one thing.
   * What is still forbidden and must never appear: a total across readers, a
   * number in the nav, and anything phrased as what you have left.
   */
  import type {
    CalibreReportDto,
    CalibreStatusDto,
    DeviceScanDto,
    PluginStatusDto,
  } from '$lib/api/bindings';
  import { client } from '$lib/api/client';
  import Calibre from '$lib/devices/Calibre.svelte';
  import ReaderCard from '$lib/devices/Reader.svelte';
  import { isHere, readers as joinReaders, type Mount, type Reader } from '$lib/devices/readers';
  import { syncSentence } from '$lib/devices/words';

  /** Three states, not a nullable: not asked, asked, answered (item 27's finding). */
  let loaded = $state(false);
  let failure = $state<string | null>(null);
  let list = $state<Reader[]>([]);
  /** What is on each volume, keyed by mount. Filled after the readers are drawn. */
  let scans = $state<Record<string, DeviceScanDto>>({});
  let calibre = $state<CalibreStatusDto | null>(null);
  let calibrePreview = $state<CalibreReportDto | null>(null);
  let calibreDone = $state<CalibreReportDto | null>(null);
  let calibreBusy = $state(false);
  /** The mount a write is running against, so its own controls can say so. */
  let busy = $state<string | null>(null);
  /** What the last action did, in the past tense. One line, never a log. */
  let said = $state<string | null>(null);

  const here = $derived(list.filter(isHere));
  const away = $derived(list.filter((r) => !isHere(r)));

  $effect(() => {
    void load();
    void loadCalibre();
  });

  async function load(): Promise<void> {
    const api = client();
    try {
      const [paired, mountPaths] = await Promise.all([api.pairedDevices(), api.candidateMounts()]);
      // `pluginStatus` refuses a path that is not a KOReader install. A volume
      // that stopped being one between the listing and the ask is a reader that
      // went away, not an error to put on the screen — so it becomes a mount
      // with no status and still appears.
      const mounts: Mount[] = await Promise.all(
        mountPaths.map(async (path) => ({
          path,
          status: await api.pluginStatus(path).catch((): PluginStatusDto | null => null),
        })),
      );
      list = joinReaders(paired, mounts);
      failure = null;
      // The scans are read-only and are the automatic half. They are deliberately
      // *not* awaited with the above: the cards are worth drawing before a
      // device with four hundred sidecars has finished being walked.
      void Promise.all(mountPaths.map((path) => scan(path)));
    } catch (e) {
      failure = e instanceof Error ? e.message : String(e);
    } finally {
      loaded = true;
    }
  }

  async function scan(mount: string): Promise<void> {
    try {
      const result = await client().scanDevice(mount);
      scans = { ...scans, [mount]: result };
    } catch {
      // A volume that cannot be walked leaves its card without a contents line.
      // The card is still right about everything else it says.
    }
  }

  async function loadCalibre(): Promise<void> {
    // Its failure is **not** this page's: the readers are the page, and calibre
    // is one more way to connect beside them.
    calibre = await client()
      .calibreStatus()
      .catch(() => null);
  }

  /** Run one write against a mount, then re-read everything it could have changed. */
  async function act(mount: string, what: () => Promise<string>): Promise<void> {
    busy = mount;
    said = null;
    try {
      said = await what();
      // Re-read rather than patching state in place: an install mints a device
      // id, an uninstall drops a row, and a sync moves a timestamp. Echoing what
      // was sent is how a screen comes to disagree with the database.
      loaded = false;
      await load();
      await scan(mount);
    } catch (e) {
      said = e instanceof Error ? e.message : String(e);
    } finally {
      busy = null;
    }
  }

  function install(mount: string) {
    void act(mount, async () => {
      const report = await client().installPlugin(mount);
      return report.upgraded_from === null
        ? `Connected. readingbuddy wrote ${report.written.length} files into ${report.plugin_dir}.`
        : `Updated the plugin on this reader, from v${report.upgraded_from} to v${report.version}.`;
    });
  }

  function uninstall(mount: string) {
    void act(mount, async () => {
      const report = await client().uninstallPlugin(mount);
      return `Took ${report.removed.length} files back off this reader. Nothing else on it was touched.`;
    });
  }

  function sync(mount: string) {
    void act(mount, async () => {
      const report = await client().syncMount(mount);
      return syncSentence(report.found, report.synced);
    });
  }

  function stats(mount: string) {
    void act(mount, async () => {
      const report = await client().importDeviceStatistics(mount);
      if (report.schema_version === null) {
        // A reader whose owner never turned the statistics plugin on. Not a
        // failure — there is simply nothing there.
        return 'This reader does not keep reading times, so there were none to bring across.';
      }
      return `Read ${report.days} days of reading time, across ${report.books_matched} of your books.`;
    });
  }

  function rename(deviceId: string, label: string) {
    void act(deviceId, async () => {
      await client().renameDevice(deviceId, label);
      return label.trim() === ''
        ? 'That reader has no name of its own again.'
        : `Called it ${label.trim()}.`;
    });
  }

  function forget(deviceId: string) {
    void act(deviceId, async () => {
      await client().forgetDevice(deviceId);
      return 'Forgotten here. The plugin is still on that reader — readingbuddy cannot reach it from this computer.';
    });
  }

  async function calibreAsk(dryRun: boolean): Promise<void> {
    calibreBusy = true;
    try {
      const report = await client().importCalibreLibrary({ dryRun });
      if (dryRun) {
        calibrePreview = report;
        calibreDone = null;
      } else {
        calibreDone = report;
      }
    } catch (e) {
      said = e instanceof Error ? e.message : String(e);
    } finally {
      calibreBusy = false;
    }
  }
</script>

<h1 class="sr-only">Devices</h1>

{#if said !== null}
  <p class="said" role="status">{said}</p>
{/if}

{#if failure !== null}
  <!-- A failure redirects. It names what is wrong and the thing that would work. -->
  <section>
    <h2 class="band-title">Your readers</h2>
    <p class="hint">
      readingbuddy could not ask about your readers: {failure}
    </p>
    <p class="hint">
      <code>readingbuddy ko plugin status</code> asks the same question from a terminal, and says more
      about why.
    </p>
  </section>
{:else if !loaded && list.length === 0}
  <p class="hint">Looking for readers…</p>
{:else}
  <section>
    <h2 class="band-title">Plugged in now</h2>
    {#if here.length === 0}
      <!-- Idle is not blank. The empty state names the moves that fill it, and
           it does not apologise or count anything. -->
      <p class="hint">
        Nothing is plugged in. Connect a reader running KOReader over USB and it appears here on its
        own — readingbuddy reads what is on it and writes nothing until you ask.
      </p>
    {:else}
      <div class="grid">
        {#each here as reader (reader.key)}
          <ReaderCard
            {reader}
            scan={reader.mount === null ? null : (scans[reader.mount.path] ?? null)}
            {busy}
            onInstall={install}
            onUninstall={uninstall}
            onSync={sync}
            onStats={stats}
            onRename={rename}
            onForget={forget}
          />
        {/each}
      </div>
    {/if}
  </section>

  <section>
    <h2 class="band-title">Your readers</h2>
    {#if list.length === 0}
      <!-- `away.length === 0` is true of two different libraries — one where
           every paired reader happens to be plugged in, and one where nothing
           has ever been paired — and the sentence below was written for the
           first. Read against *Nothing is plugged in.* directly above it, the
           second reads as a contradiction. So the empty case gets its own
           sentence, and keeps the durable half of the other one.

           It names the move rather than the absence — no *yet*, which the axiom
           bans a few lines down and which turns an empty shelf into something
           outstanding. -->
      <p class="hint">
        A reader becomes one of yours when readingbuddy's plugin goes on it. Once it is paired it is
        listed here even when it is in a drawer — pairing is a relationship, not a cable.
      </p>
    {:else if away.length === 0}
      <p class="hint">
        Every reader readingbuddy has been introduced to is plugged in right now. One in a drawer
        would still be listed here — pairing is a relationship, not a cable.
      </p>
    {:else}
      <p class="hint">
        Paired, and not here at the moment. A reader in a bag is still one of yours.
      </p>
      <div class="grid">
        {#each away as reader (reader.key)}
          <ReaderCard
            {reader}
            scan={null}
            {busy}
            onInstall={install}
            onUninstall={uninstall}
            onSync={sync}
            onStats={stats}
            onRename={rename}
            onForget={forget}
          />
        {/each}
      </div>
    {/if}
  </section>
{/if}

<section>
  <h2 class="band-title">Other ways in</h2>
  <div class="grid">
    {#if calibre !== null}
      <Calibre
        status={calibre}
        preview={calibrePreview}
        done={calibreDone}
        busy={calibreBusy}
        onPreview={() => void calibreAsk(true)}
        onImport={() => void calibreAsk(false)}
      />
    {/if}
    <article class="ways">
      <header><h3>Over a cable</h3></header>
      <p class="hint">
        Plug a KOReader reader in and readingbuddy reads its passages, its notes and where you have
        got to. That much needs no plugin and changes nothing on the device.
      </p>
      <p class="hint">
        Putting readingbuddy's plugin on the reader is what pairs the two, so this app knows the
        device between sessions. It writes into one folder of its own and can be taken off exactly.
      </p>
      <!--
        Wireless is item 15b and does not exist. Saying so is better than a
        greyed-out control, which is a dead end with a tooltip — and better than
        silence, which leaves somebody hunting for a setting.

        **No "yet"** — `axiom.test.ts` bans the word across every component, and
        it is right to here too: this is a sentence about what readingbuddy does
        today, not about something it owes you.
      -->
      <p class="hint">
        Over wifi, one day. The pairing readingbuddy does over the cable is what will make that need
        nothing typed when it arrives.
      </p>
      <p class="hint">
        A file at a time works too: <a href="/library">the library</a> takes an EPUB or a PDF straight
        in.
      </p>
    </article>
  </div>
</section>

<style>
  section {
    margin: 0 0 2rem;
  }
  .band-title {
    margin-bottom: 0.6rem;
  }
  .grid {
    display: grid;
    grid-template-columns: 1fr;
    gap: 0.9rem;
    align-items: start;
  }
  /*
   * **Two columns, capped — never `auto-fit`.** The first draft was
   * `auto-fit, minmax(24rem, 1fr)`, which gives three at 1440 and looked
   * broken in the rendered page for two reasons a rule about minimum widths
   * cannot fix. These cards have wildly different heights — a reader with a
   * refusal and an unreadable sidecar is three times the height of one with
   * nothing to say — so three columns leave a hole the size of a card in the
   * second row; and `auto-fit` also *stretches a lone card to the full width*,
   * which spread one reader's four facts across 1300px of nothing.
   *
   * Two is the shape the content has: four readers make a clean 2×2, and one
   * makes a card rather than a banner. The fold is at the width two 24rem
   * columns plus the gap actually fit.
   */
  @media (min-width: 56rem) {
    .grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  .ways {
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 1rem 1.1rem 0.9rem;
    min-width: 0;
  }
  .ways h3 {
    font-size: var(--t-lead);
  }
  .ways header {
    margin-bottom: 0.5rem;
  }
  .ways a {
    color: var(--accent-text);
  }
  /*
   * What the last action did. `role="status"` so it is announced rather than
   * only seen — a write that reports nothing to a screen reader is a write that
   * did not happen as far as that user is concerned.
   */
  .said {
    margin: 0 0 1rem;
    padding: 0.5rem 0.75rem;
    border-left: 2px solid var(--accent);
    font-size: var(--t-fine);
    max-width: var(--column);
  }
</style>
