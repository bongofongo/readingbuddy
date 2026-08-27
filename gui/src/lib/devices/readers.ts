/**
 * The join this page is made of: our rows, the volumes plugged in, and what
 * each volume's plugin says.
 *
 * Three sources answer three different questions and none of them answers the
 * page's:
 *
 * - `pairedDevices()` is **every reader you own**, in a bag or not. It is the
 *   only source that can speak about a reader that is not here.
 * - `candidateMounts()` is **every KOReader volume plugged in**, paired or not.
 *   It is the only source that can speak about a reader we have never met.
 * - `pluginStatus(mount)` is what joins them, because it is the only thing that
 *   reads the `device_id` off the volume.
 *
 * `PairedDeviceDto.last_mount_path` looks like it would do the join and **must
 * not**. Migration `0019` says so in as many words: mount points move between
 * sessions, between ports and between machines, so identity is `device_id` and
 * that column is a sentence about the past. Two Kobos plugged in one after the
 * other share a mount path; a join on it shows you the wrong reader's data.
 *
 * Everything here is pure, which is the point of the file — the page does the
 * fetching and this does the arithmetic, so the cases that only occur with four
 * volumes and a device in a bag are reachable from vitest.
 */
import type { DeviceScanDto, PairedDeviceDto, PluginStatusDto } from '$lib/api/bindings';

/**
 * One volume that is plugged in, with whatever we know about it.
 *
 * `status` is nullable because `pluginStatus` can refuse: a volume that stopped
 * being a KOReader install between the listing and the ask is not an error to
 * show, it is a reader that went away.
 */
export type Mount = {
  path: string;
  status: PluginStatusDto | null;
};

/**
 * A reader, as this page thinks of one.
 *
 * **Not a `PairedDeviceDto` and not a mount** — it is either, or both. A reader
 * we have paired with and is in a drawer has no mount; a KOReader volume with
 * nothing of ours on it has no row; a reader plugged in has both, and the
 * commonest case is the one a type built from either source alone gets wrong.
 */
export type Reader = {
  /**
   * Stable across a refresh, and it is the `device_id` whenever there is one.
   *
   * A mount path is the fallback and only for a volume we have never installed
   * onto — which is exactly the reader that has no id yet. Keying every reader
   * by path would make unplugging and replugging into a different port look
   * like a different device.
   */
  key: string;
  /** Our row, when this is a reader we have paired with. */
  device: PairedDeviceDto | null;
  /** The volume, when it is plugged in right now. */
  mount: Mount | null;
};

/**
 * Every reader worth drawing, plugged-in ones first.
 *
 * The order is *presence*, then the engine's own order within each group
 * (`pairedDevices` is newest-seen first and nothing here re-sorts it). Presence
 * first because it is the only thing on this page you can act on: an install,
 * an upgrade and a sync all need the volume, and a reader in a bag has exactly
 * one verb — forget it.
 *
 * A mount whose `pluginStatus` refused still appears. It is a KOReader volume,
 * the engine said so, and dropping it would make a reader vanish from a page
 * whose whole job is to say which ones are here.
 */
export function readers(paired: PairedDeviceDto[], mounts: Mount[]): Reader[] {
  const claimed = new Set<string>();
  const here: Reader[] = mounts.map((mount) => {
    const id = mount.status?.device_id ?? null;
    // `paired` on the status is the engine's answer to *is this ours*, and it
    // is the one to trust: a volume can carry a `pairing.lua` minted by another
    // copy of readingbuddy, and that reader is not in our list.
    const device = id === null ? null : (paired.find((d) => d.device_id === id) ?? null);
    if (device !== null) claimed.add(device.device_id);
    return { key: device?.device_id ?? id ?? mount.path, device, mount };
  });
  const away: Reader[] = paired
    .filter((d) => !claimed.has(d.device_id))
    .map((device) => ({ key: device.device_id, device, mount: null }));
  return [...here, ...away];
}

/** Is this reader plugged in right now? */
export function isHere(r: Reader): boolean {
  return r.mount !== null;
}

/**
 * What to call a reader.
 *
 * The label is the only field on this page a person controls and it is
 * frequently absent — `label` defaults to the mount's *directory name* at
 * install (`Kindle`, `KOBOeReader`), and a rename can clear it back to nothing.
 *
 * The fallbacks descend through what is actually identifying: the name you gave
 * it, the volume it is on right now, then the head of its id. A full uuid is
 * never a name — it is what `forget` takes, and the page prints it as such
 * elsewhere.
 *
 * *A reader* is the last resort and it is deliberately not *Unknown device*: a
 * volume plugged into this machine is not unknown, it is unnamed.
 */
export function readerName(r: Reader): string {
  const label = r.device?.label;
  if (label !== null && label !== undefined && label.trim() !== '') return label;
  const path = r.mount?.path;
  if (path !== undefined) {
    const leaf = path.split('/').filter(Boolean).pop();
    if (leaf !== undefined && leaf !== '') return leaf;
  }
  const id = r.device?.device_id;
  if (id !== undefined && id !== '') return id.slice(0, 8);
  return 'A reader';
}

/**
 * How many books on this volume have something to bring across.
 *
 * **The predicate is the engine's**, not a string test written here:
 * `DeviceState::is_syncable` is `New | Updated`, and `DeviceStateDto` is the
 * same union tagged. This counts a typed thing rather than deciding what
 * syncable means — the counting is the frontend's half, the membership is not.
 *
 * A count is allowed on this page for `/life`'s reason: it is a page you chose
 * to open, and this describes what a reader is holding rather than what you
 * have left to do. It is never rendered anywhere you did not ask to be.
 */
export function waiting(scan: DeviceScanDto | null): number {
  if (scan === null) return 0;
  return scan.books.filter((b) => b.state.state === 'new' || b.state.state === 'updated').length;
}

/** Books on the volume whose sidecar could not be read. Never fatal, always said. */
export function unreadable(scan: DeviceScanDto | null): number {
  if (scan === null) return 0;
  return scan.books.filter((b) => b.state.state === 'unreadable').length;
}
