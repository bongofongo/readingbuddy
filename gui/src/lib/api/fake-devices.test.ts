/**
 * The fake's readers, and the two libraries layer 2 cannot render.
 *
 * `FakeClient` is constructed by `client()` with no arguments, so Playwright
 * always gets the populated page. The empty states — nothing plugged in, no
 * calibre — are reachable only from here, and they are the two the axiom has
 * the most to say about.
 */
import { describe, expect, it } from 'vitest';

import { FakeClient } from './fake';

const KINDLE = '/run/media/oliver/Kindle';
const POCKETBOOK = '/run/media/oliver/PB632';
const BORROWED = '/media/oliver/Reader';

describe('the readers the fake models', () => {
  it('hands them back newest-seen first, which is the engine’s order', () => {
    // Stated rather than left to array order: a page that re-sorted would look
    // right against a fixture that happened to already be sorted.
    return new FakeClient().pairedDevices().then((ds) => {
      const seen = ds.map((d) => d.last_seen_at ?? d.installed_at);
      expect([...seen].sort((a, b) => b - a)).toEqual(seen);
    });
  });

  it('includes a reader with no name and one whose name is long', async () => {
    const ds = await new FakeClient().pairedDevices();
    expect(ds.some((d) => d.label === null)).toBe(true);
    expect(ds.some((d) => (d.label?.length ?? 0) > 40)).toBe(true);
  });

  it('includes a reader nothing has ever been brought across from', async () => {
    // `last_synced_at: null` is the case the copy is most likely to get wrong.
    const ds = await new FakeClient().pairedDevices();
    expect(ds.some((d) => d.last_synced_at === null)).toBe(true);
    expect(ds.some((d) => d.last_synced_at !== null)).toBe(true);
  });

  it('refuses a plugin status for a path that is not a KOReader install', async () => {
    // The real engine errors rather than answering with an empty status, and a
    // screen that treated the two the same would offer to install onto a stick.
    await expect(new FakeClient().pluginStatus('/media/usb-stick')).rejects.toThrow();
  });
});

describe('nothing plugged in', () => {
  it('is an empty list and not an error', async () => {
    const c = new FakeClient({ plugged: false });
    expect(await c.candidateMounts()).toEqual([]);
    // The readers you own are unaffected: pairing is a relationship, not a cable.
    expect((await c.pairedDevices()).length).toBeGreaterThan(0);
  });
});

describe('installing, and what it does to the page’s two sources', () => {
  it('turns an unpaired volume into a paired reader', async () => {
    const c = new FakeClient();
    const before = await c.pluginStatus(POCKETBOOK);
    expect(before.condition).toBe('absent');
    expect(before.paired).toBe(false);

    const report = await c.installPlugin(POCKETBOOK);
    expect(report.plugin_dir).toBe(before.plugin_dir);
    expect(report.upgraded_from).toBeNull();

    const after = await c.pluginStatus(POCKETBOOK);
    expect(after.condition).toBe('current');
    expect(after.device_id).toBe(report.device_id);
  });

  it('reports an upgrade as an upgrade rather than as a first install', async () => {
    const c = new FakeClient();
    const report = await c.installPlugin('/run/media/oliver/KOBOeReader');
    expect(report.upgraded_from).toBe(1);
    expect(report.version).toBe(2);
  });

  it('refuses a reader whose files we did not put there', async () => {
    await expect(new FakeClient().installPlugin(BORROWED)).rejects.toThrow();
  });

  it('an uninstall drops our row and leaves the volume a KOReader volume', async () => {
    const c = new FakeClient();
    const report = await c.uninstallPlugin(KINDLE);
    expect(report.forgot_device).not.toBeNull();
    expect((await c.pairedDevices()).some((d) => d.device_id === report.forgot_device)).toBe(false);
    // Still a reader, just not one of ours.
    expect((await c.pluginStatus(KINDLE)).condition).toBe('absent');
  });
});

describe('forgetting and renaming', () => {
  it('says whether there was anything to forget', async () => {
    const c = new FakeClient();
    const id = (await c.pairedDevices())[0]!.device_id;
    expect(await c.forgetDevice(id)).toBe(true);
    expect(await c.forgetDevice(id)).toBe(false);
    expect(await c.forgetDevice('nobody')).toBe(false);
  });

  it('clears a blank name rather than storing one', async () => {
    const c = new FakeClient();
    const id = (await c.pairedDevices())[0]!.device_id;
    await c.renameDevice(id, '  the good one  ');
    const named = (await c.pairedDevices()).find((d) => d.device_id === id);
    expect(named?.label).toBe('the good one');

    await c.renameDevice(id, '   ');
    const cleared = (await c.pairedDevices()).find((d) => d.device_id === id);
    expect(cleared?.label).toBeNull();
  });
});

describe('a whole-device sync', () => {
  it('reports found and synced apart', async () => {
    const c = new FakeClient();
    const done = await c.syncMount(KINDLE);
    expect(done.found).toBe(3);
    expect(done.synced).toBe(2);
    expect(done.reports).toHaveLength(2);
    expect(done.device_id).not.toBeNull();

    // And afterwards nothing is new, while the books are still there — the
    // distinction the two numbers exist to keep.
    const again = await c.syncMount(KINDLE);
    expect(again.found).toBe(3);
    expect(again.synced).toBe(0);
  });

  it('stamps nothing for a volume carrying nobody’s pairing', async () => {
    const c = new FakeClient();
    const done = await c.syncMount(POCKETBOOK);
    expect(done.device_id).toBeNull();
    // The books still came across. Importing sidecars has never needed a pairing.
    expect(done.synced).toBe(1);
  });
});

describe('calibre', () => {
  it('reports both tools absent as an answer, not a failure', async () => {
    const status = await new FakeClient({ calibre: false }).calibreStatus();
    expect(status.calibredb).toBeNull();
    expect(status.ebook_convert).toBeNull();
  });

  it('a dry run says it is one', async () => {
    const c = new FakeClient();
    expect((await c.importCalibreLibrary({ dryRun: true })).dry_run).toBe(true);
    expect((await c.importCalibreLibrary({})).dry_run).toBe(false);
  });
});
