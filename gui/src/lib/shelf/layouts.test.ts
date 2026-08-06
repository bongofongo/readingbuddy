/**
 * The shelf seam, layer 1.
 *
 * What is worth asserting here is not that a grid renders — that is layer 2 and
 * a screenshot. It is that the *registry* holds: that a layout can be added
 * without editing anything but `LAYOUTS`, and that every way of asking for one
 * that could fail degrades to a shelf rather than to nothing.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  DEFAULT_LAYOUT,
  LAYOUTS,
  layoutById,
  recallLayout,
  rememberLayout,
  type ShelfLayoutId,
} from './layouts';

/** A `localStorage` that is present and works. `vitest` runs in `node`. */
function withStorage(impl: Partial<Storage>) {
  vi.stubGlobal('localStorage', impl as Storage);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('the registry', () => {
  it('offers more than one arrangement', () => {
    // Not padding. A seam with a single implementation is a guess about what
    // varies; this repo's standing complaint about guards that cannot fail is
    // the same defect. If this ever drops to one, the abstraction stopped being
    // load-bearing and should be argued for again rather than kept out of habit.
    expect(LAYOUTS.length).toBeGreaterThan(1);
  });

  it('has no two layouts under one id', () => {
    const ids = LAYOUTS.map((l) => l.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('gives every layout a component and a name', () => {
    for (const l of LAYOUTS) {
      expect(l.component, `${l.id} has no component`).toBeTruthy();
      expect(l.label.trim(), `${l.id} has no label`).not.toBe('');
    }
  });

  it('opens on the first entry', () => {
    expect(DEFAULT_LAYOUT).toBe(LAYOUTS[0].id);
  });
});

describe('layoutById', () => {
  it('finds each layout by its own id', () => {
    for (const l of LAYOUTS) expect(layoutById(l.id).id).toBe(l.id);
  });

  it('falls back rather than throwing on an id that is not one', () => {
    // The stored preference is a string from disk written by an older build.
    // A layout that was removed must not take the shelf down with it.
    expect(layoutById('spines-webgl').id).toBe(DEFAULT_LAYOUT);
    expect(layoutById('').id).toBe(DEFAULT_LAYOUT);
    expect(layoutById(null).id).toBe(DEFAULT_LAYOUT);
  });
});

describe('the remembered preference', () => {
  it('round-trips through a working store', () => {
    const store = new Map<string, string>();
    withStorage({
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => void store.set(k, v),
    });

    const other = LAYOUTS[1]!.id;
    rememberLayout(other);
    expect(recallLayout()).toBe(other);
  });

  it('degrades to the default when there is no store at all', () => {
    // The `node` environment these tests run in has no `localStorage`, so this
    // is the un-stubbed case and it is also the real one: a bare `vite build`
    // and a privacy-locked webview both reach it.
    expect(recallLayout()).toBe(DEFAULT_LAYOUT);
  });

  it('degrades when the store throws rather than returning null', () => {
    // Safari in private mode throws on access. A shelf that failed to render
    // because it could not recall a *preference* would be the tail wagging the
    // dog, so both halves swallow it.
    withStorage({
      getItem: () => {
        throw new DOMException('denied', 'SecurityError');
      },
      setItem: () => {
        throw new DOMException('denied', 'SecurityError');
      },
    });

    expect(recallLayout()).toBe(DEFAULT_LAYOUT);
    expect(() => rememberLayout(LAYOUTS[1]!.id as ShelfLayoutId)).not.toThrow();
  });
});
