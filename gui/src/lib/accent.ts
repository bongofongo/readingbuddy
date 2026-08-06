/**
 * The jacket colour, pulled into a band this renderer can light.
 *
 * ## Why the GUI owes this at all
 *
 * `cover_accent` is stored **unclamped** — the median of a 2px border around
 * the real file and nothing else. That is deliberate, and `crates/engine/src/
 * images.rs` states the rule: *the luma clamp stays in the renderer, because it
 * is a renderer's policy about its own lighting rather than a fact about the
 * file.* The terminal sibling has such a policy (`render3d/texture.rs`'s
 * `clamp_luma`, band 0.14–0.62, chosen so a spine reads against the page edges
 * either side of it). This frontend had none, so it was painting the raw
 * measurement — and a raw measurement spans the whole gamut, including the
 * near-black and the near-white a plate cannot carry an inset panel against.
 *
 * The two bands differ on purpose and must not be unified. A terminal's
 * background is unknown, so the TUI's band is defensive at both ends; a webview
 * knows its own `--bg` and can let a jacket go darker than a spine ever could.
 * Copying the constants across would be the duplication item 39 exists to
 * remove, wearing the opposite costume.
 *
 * ## The second operation, and why it is not tuning
 *
 * A plate is a **stand-in for a jacket nobody has seen**. Asserting a fully
 * saturated colour claims more about the book than the border of a file the
 * engine happened to measure can support, and twenty of them side by side read
 * as swatches rather than as a shelf. So the colour is also pulled a little
 * toward its own grey: hue kept, confidence reduced. A real cover, once it
 * arrives, replaces the plate entirely and none of this applies to it.
 */
import type { AccentDto } from './api/bindings';

/**
 * The band. Wider at the dark end than the terminal's, for the reason above.
 * Both ends are exclusive of the extremes rather than generous: a plate at 0.02
 * is a hole in the shelf and one at 0.9 is a light source.
 */
const LUMA_MIN = 0.09;
const LUMA_MAX = 0.58;

/** How far toward its own grey a plate is pulled. 0 keeps the measurement. */
const SOFTEN = 0.22;

/** Rec. 709, the same coefficients the terminal renderer weights with. */
function luma(r: number, g: number, b: number): number {
  return r * 0.2126 + g * 0.7152 + b * 0.0722;
}

/**
 * A CSS colour for `accent`, or `null` when there is nothing measured.
 *
 * `null` is not a colour and must not become one here — a book with no
 * measurement gets a *different* empty state (see `BookTile`), because a
 * fallback colour would make "no cover was ever measured" and "this jacket is
 * grey" the same picture.
 */
export function plateColor(accent: AccentDto | null | undefined): string | null {
  if (!accent) return null;

  let r = accent.r / 255;
  let g = accent.g / 255;
  let b = accent.b / 255;

  // Scale the whole colour toward the band rather than clipping each channel:
  // clipping moves the hue, which is the one thing a jacket colour has to keep.
  const l = luma(r, g, b);
  if (l < 1e-4) {
    // Black has no hue to preserve, so there is nothing to scale — go straight
    // to the floor. Scaling would divide by ~zero and blow the channels out.
    r = g = b = LUMA_MIN;
  } else {
    const scale = Math.min(Math.max(l, LUMA_MIN), LUMA_MAX) / l;
    r = Math.min(r * scale, 1);
    g = Math.min(g * scale, 1);
    b = Math.min(b * scale, 1);
  }

  // Toward its own grey, after the clamp — doing it before would let the
  // softening move the luma back out of the band it was just pulled into.
  const grey = luma(r, g, b);
  r += (grey - r) * SOFTEN;
  g += (grey - g) * SOFTEN;
  b += (grey - b) * SOFTEN;

  const byte = (v: number) => Math.round(Math.min(Math.max(v, 0), 1) * 255);
  return `rgb(${byte(r)} ${byte(g)} ${byte(b)})`;
}

/** How far the inset panel and its two rules step away from the plate. */
const PANEL_STEP = 0.16;
const RULE_STEP = 0.32;

/**
 * The plate, plus the two shades its composition is drawn in.
 *
 * The step has to be **away from the plate's own lightness**, and that is why
 * it cannot be a `color-mix` in the stylesheet: CSS mixes toward a colour named
 * at author time, so mixing toward white made the panel crisp on dark jackets
 * and invisible on pale ones, and mixing toward the ink did exactly the same
 * thing mirrored. Neither is wrong about a particular book; both are wrong
 * about the shelf, which has to give every jacket the same amount of design.
 *
 * A lerp toward the near pole rather than a luma rescale, because the target
 * here is a *visible step* rather than a band — and unlike `plateColor`'s
 * scaling, a lerp toward a pole cannot clip and so always moves.
 */
export function plateShades(
  accent: AccentDto | null | undefined,
): { base: string; panel: string; rule: string } | null {
  const base = plateColor(accent);
  if (!base) return null;

  const m = base.match(/rgb\((\d+) (\d+) (\d+)\)/);
  if (!m) return null;
  const rgb = [Number(m[1]) / 255, Number(m[2]) / 255, Number(m[3]) / 255] as const;

  // Below the midpoint a jacket has room above it and nowhere useful below.
  const up = luma(rgb[0], rgb[1], rgb[2]) < 0.5;
  const step = (t: number) => {
    const c = rgb.map((v) => (up ? v + (1 - v) * t : v * (1 - t)));
    return `rgb(${c.map((v) => Math.round(v * 255)).join(' ')})`;
  };

  return { base, panel: step(PANEL_STEP), rule: step(RULE_STEP) };
}
