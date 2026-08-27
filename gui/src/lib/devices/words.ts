/**
 * Words for the devices page, and the line they stay on.
 *
 * `phrasing.ts`'s rule applies here unchanged: the engine states the value, the
 * frontend words it. `PluginConditionDto` is the engine's verdict about what an
 * install would do — item 55 put it on the wire precisely so this file cannot
 * become a second implementation of `installed_version < our_version` — and
 * everything below chooses English for it.
 *
 * Kept out of `phrasing.ts` because that file is the *library's* vocabulary and
 * this is one screen's. The two would have nothing to say to each other.
 */
import type { PluginConditionDto } from '$lib/api/bindings';
import { dayLabel } from '$lib/phrasing';

/**
 * What the plugin on this reader is, said in the reader's terms rather than in
 * ours.
 *
 * **It never says *connected*, and that is a correction the rendered page
 * forced.** The card already carries a *Plugged in* chip, which is about the
 * cable; a line under it reading *Not connected* is a flat contradiction — the
 * thing is plainly connected, it is sitting in a USB port. These two facts are
 * about different things, so they get different words: the chip is the cable,
 * and this is whether readingbuddy is **on** the reader.
 *
 * `Obstructed` is deliberately not a word here — it is a refusal, and a refusal
 * names what is in the way, which the page does from `modified` /
 * `unrecognised` beside it.
 */
export function conditionLabel(c: PluginConditionDto): string {
  switch (c) {
    case 'absent':
      // **Not "not on it yet"** — `axiom.test.ts` bans the word and the reason
      // applies exactly here: *yet* turns a reader you have never paired into a
      // reader you were supposed to have paired.
      return 'readingbuddy is not on this reader';
    case 'current':
      return 'readingbuddy is on this reader';
    case 'upgradable':
      return 'readingbuddy is on this reader · a newer plugin is ready';
    case 'unversioned':
      return 'readingbuddy is on this reader · it does not say which version';
    case 'obstructed':
      return 'readingbuddy is leaving this reader alone';
    default:
      // Exhaustive in TypeScript, open on the wire — ts-rs drops
      // `#[serde(other)]`, so a newer build's condition degrades here instead
      // of crashing. See `phrasing.ts`' `readingStateLabel`.
      return 'readingbuddy is on this reader';
  }
}

/**
 * Whether writing the plugin is the thing this reader most wants done.
 *
 * **The accent goes on exactly one control and it has to be the useful one.**
 * `gui/CLAUDE.md`: the accent is for state you can act on, and the two jobs that
 * most need to be loud are *this is selected* and *this is the action*. On a
 * reader whose plugin is already current, the action you came for is bringing
 * your reading across — and the first draft of this page put the fill on *Put
 * the plugin on again*, i.e. on the least likely thing anybody wants, found by
 * rendering it and looking.
 */
export function installIsPrimary(c: PluginConditionDto): boolean {
  return c === 'absent' || c === 'upgradable';
}

/**
 * What the button that writes to this reader says, or `null` when there is no
 * such button.
 *
 * `null` for `obstructed` is the whole of the refusal: the engine will not
 * write, so the page shows **no control** rather than one that errors when
 * pressed. A disabled button is a dead end wearing a tooltip.
 */
export function installVerb(c: PluginConditionDto): string | null {
  switch (c) {
    case 'absent':
      return 'Connect this reader';
    case 'current':
      return 'Put the plugin on again';
    case 'upgradable':
      return 'Update the plugin';
    case 'unversioned':
      return 'Put the plugin on again';
    case 'obstructed':
      return null;
    default:
      return null;
  }
}

/**
 * Why we are not writing to this reader, and what would change it.
 *
 * Every arm names the file and the move — the repo's refusal-with-a-next-move
 * shape, and the reason a refusal here is not a dead end. The order matches
 * `PluginStatus::is_obstructed`'s: our own edited files first, because that is
 * the one a person did on purpose and can undo.
 */
export function obstruction(
  modified: string[],
  unrecognised: string[],
  installedVersion: number | null,
  ourVersion: number,
): string | null {
  if (modified.length > 0) {
    return `You have edited ${andList(modified)} on this reader. readingbuddy will not overwrite or remove your version — move it aside and it will connect.`;
  }
  if (unrecognised.length > 0) {
    return `${andList(unrecognised)} is in readingbuddy's folder on this reader and is not ours. It will be left exactly where it is.`;
  }
  if (installedVersion !== null && installedVersion > ourVersion) {
    return `This reader carries plugin v${installedVersion} and this readingbuddy is v${ourVersion}. A newer readingbuddy put it there; this one will not write over it.`;
  }
  return null;
}

/**
 * When this reader was last in your hands.
 *
 * `last_seen_at` means that, and only since item 55 — before it, it moved on
 * install alone, so a reader plugged in nightly reported the day its plugin was
 * put there. `null` reaches this only for a row written by an older build.
 */
export function seenLabel(lastSeenAt: number | null): string {
  const day = dayLabel(lastSeenAt);
  return day === null ? 'Not since readingbuddy started recording' : day;
}

/**
 * When everything this reader had was last brought across.
 *
 * **`null` is not *never*.** Migration `0020` arrived with no back-fill — no
 * row anywhere could attribute a past import to a device — so an absent value
 * means *not since readingbuddy started recording it*, which is a sentence
 * about our records and not about the reader. Wording it *Never synced* would
 * accuse a reader that has been synced fifty times, and it would be the
 * completion framing the axiom forbids into the bargain.
 */
export function syncedLabel(lastSyncedAt: number | null): string {
  const day = dayLabel(lastSyncedAt);
  return day === null ? 'Not since readingbuddy started recording' : day;
}

/**
 * What one whole-device sync did, in the past tense.
 *
 * The two numbers are different facts and this is the function that keeps them
 * so: `found: 0` is *there is nothing on this reader* and `synced: 0` with
 * books found is *you already have all of it*. A single "0 books synced" would
 * render a brand-new Kobo and a fully-synced Kindle identically.
 */
export function syncSentence(found: number, synced: number): string {
  if (found === 0) return 'There is nothing on this reader to read yet.';
  if (synced === 0) return 'Everything on this reader was already here.';
  return synced === 1 ? 'Brought one book across.' : `Brought ${synced} books across.`;
}

/**
 * How much of this reader is not here yet, or `null` when none of it is.
 *
 * `null` rather than *0 waiting*, because a zero rendered in the place a number
 * goes is a scoreboard reading zero — and *nothing waiting* is not a thing you
 * have left to do. The page draws the absence as an absence.
 */
export function waitingLabel(n: number): string | null {
  if (n <= 0) return null;
  return n === 1 ? 'One book has something new on it' : `${n} books have something new on them`;
}

/** *a*, *a and b*, *a, b and c* — `phrasing.ts`' `joinList`, for file names. */
function andList(items: string[]): string {
  if (items.length === 0) return '';
  if (items.length === 1) return items[0]!;
  return `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]!}`;
}
