/**
 * The devices page's words, and the four sentences that would be lies.
 *
 * These are axiom assertions more than string tests. Every one of them is a
 * phrasing somebody would reach for by reflex, that is wrong about a real
 * library, and that no screenshot of a populated fixture would catch.
 */
import { describe, expect, it } from 'vitest';

import {
  conditionLabel,
  installIsPrimary,
  installVerb,
  obstruction,
  syncedLabel,
  syncSentence,
  waitingLabel,
} from './words';

describe('a null sync stamp is not "never"', () => {
  it('says it is about our records rather than about the reader', () => {
    // Migration `0020` arrived with **no back-fill** — nothing recorded which
    // device a past sync read from — so `null` means *not since we started
    // recording*. "Never synced" would accuse a reader synced fifty times.
    const said = syncedLabel(null);
    expect(said).toMatch(/started recording/i);
    expect(said).not.toMatch(/never/i);
  });

  it('prints a real stamp as the day it was', () => {
    expect(syncedLabel(1_756_200_000)).toBe('2025-08-26');
  });
});

describe('nothing on a reader and nothing new on it are different sentences', () => {
  it('says so', () => {
    // The pair that a single "0 books synced" collapses. A brand-new Kobo and a
    // fully-synced Kindle would render identically.
    expect(syncSentence(0, 0)).not.toBe(syncSentence(12, 0));
    expect(syncSentence(0, 0)).toMatch(/nothing on this reader/i);
    expect(syncSentence(12, 0)).toMatch(/already here/i);
  });

  it('speaks a real sync in the past tense', () => {
    expect(syncSentence(12, 1)).toBe('Brought one book across.');
    expect(syncSentence(12, 3)).toBe('Brought 3 books across.');
  });
});

describe('the one figure on a reader card', () => {
  it('renders no zero, because a zero in a figure’s place is a scoreboard', () => {
    expect(waitingLabel(0)).toBeNull();
    expect(waitingLabel(-1)).toBeNull();
  });

  it('describes the reader’s contents and never what you owe', () => {
    const said = waitingLabel(3)!;
    expect(said).toMatch(/3 books/);
    // The vocabulary the axiom bans by name, applied to this surface.
    expect(said).not.toMatch(/pending|waiting for you|to do|remaining|unread|due/i);
  });
});

describe('a reader we will not write to', () => {
  it('offers no control at all rather than a disabled one', () => {
    // A disabled button is a dead end wearing a tooltip. The refusal is the
    // sentence beside it, and the sentence carries the move.
    expect(installVerb('obstructed')).toBeNull();
    expect(installVerb('absent')).not.toBeNull();
    expect(installVerb('upgradable')).not.toBeNull();
  });

  it('names the file and the move, in that order', () => {
    const edited = obstruction(['main.lua'], [], 1, 1)!;
    expect(edited).toContain('main.lua');
    expect(edited).toMatch(/move it aside/i);

    // Somebody else's file in our directory. We leave it exactly where it is.
    const theirs = obstruction([], ['notes.txt'], 1, 1)!;
    expect(theirs).toContain('notes.txt');
    expect(theirs).toMatch(/left exactly where it is/i);

    // A newer plugin than this build carries.
    const newer = obstruction([], [], 3, 2)!;
    expect(newer).toMatch(/v3/);
    expect(newer).toMatch(/v2/);

    expect(obstruction([], [], 1, 2)).toBeNull();
  });

  it('prefers our own edited files, which is the one the reader can undo', () => {
    expect(obstruction(['main.lua'], ['notes.txt'], 9, 1)).toContain('main.lua');
  });
});

describe('the plugin’s state, in a reader’s terms', () => {
  it('never frames a reader as unfinished work', () => {
    for (const c of ['absent', 'current', 'upgradable', 'unversioned', 'obstructed'] as const) {
      expect(conditionLabel(c)).not.toMatch(/error|failed|missing|incomplete|not set up/i);
    }
  });

  it('never says "connected", which the Plugged in chip already says', () => {
    // Found by rendering it: the card carries a *Plugged in* chip about the
    // cable, and *Not connected* under it is a flat contradiction — the thing
    // is in a USB port. Two facts, two vocabularies.
    for (const c of ['absent', 'current', 'upgradable', 'unversioned', 'obstructed'] as const) {
      expect(conditionLabel(c)).not.toMatch(/\bconnected\b/i);
    }
  });

  it('puts the fill on the verb the reader actually wants', () => {
    // The accent is for the action, and on a reader whose plugin is current the
    // action you came for is the sync. The first draft accented *Put the plugin
    // on again* — the least likely thing anybody wants.
    expect(installIsPrimary('absent')).toBe(true);
    expect(installIsPrimary('upgradable')).toBe(true);
    expect(installIsPrimary('current')).toBe(false);
    expect(installIsPrimary('unversioned')).toBe(false);
    expect(installIsPrimary('obstructed')).toBe(false);
  });

  it('degrades on a condition this build does not know', () => {
    // ts-rs drops `#[serde(other)]`, so the union is exhaustive in TypeScript
    // and open on the wire. A newer engine's condition must not crash a page.
    expect(conditionLabel('a-newer-engine-said-this' as never)).toBe(
      'readingbuddy is on this reader',
    );
    expect(installVerb('a-newer-engine-said-this' as never)).toBeNull();
  });
});
