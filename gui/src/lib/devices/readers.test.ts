/**
 * The join, and the four things it must get right that no screenshot shows.
 *
 * Every case here is one a fixture with a single plugged-in reader passes
 * without asserting anything: the mount-path join, a volume carrying somebody
 * else's pairing, a reader in a bag, and the key that survives a replug.
 */
import { describe, expect, it } from 'vitest';

import type { DeviceScanDto, PairedDeviceDto, PluginStatusDto } from '$lib/api/bindings';

import { isHere, readerName, readers, unreadable, waiting, type Mount } from './readers';

function device(over: Partial<PairedDeviceDto> = {}): PairedDeviceDto {
  return {
    device_id: 'aaaa1111',
    label: 'Kindle',
    plugin_version: 1,
    installed_at: 1_700_000_000,
    last_mount_path: '/run/media/oliver/Kindle',
    last_seen_at: 1_756_000_000,
    last_synced_at: null,
    ...over,
  };
}

function status(over: Partial<PluginStatusDto> = {}): PluginStatusDto {
  return {
    mount: '/run/media/oliver/Kindle',
    plugin_dir: '/run/media/oliver/Kindle/koreader/plugins/readingbuddy.koplugin',
    installed: true,
    installed_version: 1,
    our_version: 1,
    paired: true,
    device_id: 'aaaa1111',
    modified: [],
    unrecognised: [],
    condition: 'current',
    ...over,
  };
}

function scan(books: DeviceScanDto['books']): DeviceScanDto {
  return { root: '/mnt', books, warnings: [], parsed: books.length, cached: 0 };
}

function book(state: DeviceScanDto['books'][number]['state']): DeviceScanDto['books'][number] {
  return {
    path: '/mnt/x.sdr/metadata.epub.lua',
    title: 'x',
    authors: null,
    partial_md5: null,
    book_id: null,
    matched_by: null,
    state,
    ko_percent: null,
    ko_status: null,
  };
}

describe('joining readers to what is plugged in', () => {
  it('joins on the device id the volume carries, never on the mount path', () => {
    // The same reader, on a different port from the one our row remembers. A
    // join on `last_mount_path` — which is what the column looks like it is for
    // — would show this as two readers.
    const d = device({ last_mount_path: '/run/media/oliver/Kindle' });
    const mount: Mount = {
      path: '/media/oliver/Kindle1',
      status: status({ mount: '/media/oliver/Kindle1' }),
    };
    const [only, ...rest] = readers([d], [mount]);
    expect(rest).toEqual([]);
    expect(only?.device?.device_id).toBe('aaaa1111');
    expect(only?.mount?.path).toBe('/media/oliver/Kindle1');
    expect(isHere(only!)).toBe(true);
  });

  it('shows a volume carrying another readingbuddy’s pairing as an unpaired reader', () => {
    // `pairing.lua` names a device we have no row for. It is still a KOReader
    // volume and still worth showing — with no history, because we have none.
    const mount: Mount = {
      path: '/mnt/theirs',
      status: status({ mount: '/mnt/theirs', device_id: 'somebody-else', paired: false }),
    };
    const [only] = readers([], [mount]);
    expect(only?.device).toBeNull();
    expect(isHere(only!)).toBe(true);
  });

  it('lists a paired reader that is not plugged in, after the ones that are', () => {
    const bag = device({ device_id: 'bbbb2222', label: 'the Kobo' });
    const mount: Mount = { path: '/mnt/k', status: status({ mount: '/mnt/k' }) };
    const list = readers([device(), bag], [mount]);
    expect(list.map((r) => r.key)).toEqual(['aaaa1111', 'bbbb2222']);
    expect(list.map(isHere)).toEqual([true, false]);
  });

  it('keeps a volume whose plugin status refused', () => {
    // A volume that stopped being a KOReader install between the listing and
    // the ask. Dropping it makes a reader vanish from the page whose job is to
    // say which ones are here.
    const [only] = readers([], [{ path: '/mnt/gone', status: null }]);
    expect(only?.key).toBe('/mnt/gone');
    expect(only?.device).toBeNull();
  });

  it('keys a paired reader by its id and never by the port it is in', () => {
    const mount = (path: string): Mount => ({ path, status: status({ mount: path }) });
    const first = readers([device()], [mount('/media/a')]);
    const second = readers([device()], [mount('/media/b')]);
    expect(first[0]?.key).toBe(second[0]?.key);
  });
});

describe('what a reader is called', () => {
  it('prefers the name you gave it', () => {
    const [r] = readers([device({ label: 'the bedside Kobo' })], []);
    expect(readerName(r!)).toBe('the bedside Kobo');
  });

  it('falls back to the volume, then to the head of the id', () => {
    const mount: Mount = { path: '/run/media/oliver/PB632', status: null };
    expect(readerName(readers([], [mount])[0]!)).toBe('PB632');
    // A cleared name — `renameDevice('')` stores NULL rather than a blank, so
    // this is the state the fallback exists for.
    expect(readerName(readers([device({ label: null, last_mount_path: null })], [])[0]!)).toBe(
      'aaaa1111',
    );
  });

  it('does not read a whitespace label as a name', () => {
    // The engine clears a blank rather than storing one, so this should be
    // unreachable — which is exactly why it is asserted rather than assumed.
    const mount: Mount = { path: '/run/media/oliver/Kindle', status: status() };
    const [r] = readers([device({ label: '   ' })], [mount]);
    expect(readerName(r!)).toBe('Kindle');
    // And with nothing else to go on, the id's head rather than the blank.
    expect(readerName(readers([device({ label: '   ' })], [])[0]!)).toBe('aaaa1111');
  });
});

describe('what is on a volume', () => {
  it('counts only the states the engine calls syncable', () => {
    const s = scan([
      book({ state: 'new', candidates: [] }),
      book({ state: 'updated', new_highlights: 2, refreshed: 0 }),
      book({ state: 'unchanged' }),
    ]);
    expect(waiting(s)).toBe(2);
  });

  it('counts an unread volume as nothing waiting rather than as unknown', () => {
    expect(waiting(null)).toBe(0);
    expect(waiting(scan([]))).toBe(0);
  });

  it('counts unreadable sidecars apart, because one must not cost the rest', () => {
    const s = scan([
      book({ state: 'new', candidates: [] }),
      book({
        state: 'unreadable',
        diagnostic: {
          kind: 'sidecar_unparsable',
          path: 'x.sdr/metadata.epub.lua',
          severity: 'warning',
          detail: 'nil',
          display: 'could not parse x',
        },
      }),
    ]);
    expect(waiting(s)).toBe(1);
    expect(unreadable(s)).toBe(1);
  });
});
