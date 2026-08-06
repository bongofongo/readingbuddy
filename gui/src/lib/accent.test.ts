/**
 * The plate colour's band.
 *
 * A property, because the rule is general rather than a list of examples — and
 * scoped honestly, which for this function means admitting the one place the
 * floor cannot be reached. Asserting a clean `luma >= MIN` here would be
 * asserting something false and weakening it later, which this repo rates worse
 * than asserting less.
 */
import { describe, expect, it } from 'vitest';

import { plateColor, plateShades } from './accent';

const LUMA_MIN = 0.09;
const LUMA_MAX = 0.58;

/**
 * The band holds to within a byte, and cannot hold to better than one.
 *
 * The output is three integers, so rounding moves the luma by up to half a step
 * per channel. Asserting equality to 1e-6 would be asserting something about
 * arithmetic that never reaches the screen; this is the real resolution.
 */
const BYTE = 1 / 255;

function lumaOf(css: string): number {
  const m = css.match(/rgb\((\d+) (\d+) (\d+)\)/);
  if (!m) throw new Error(`not a colour: ${css}`);
  const [r, g, b] = [Number(m[1]) / 255, Number(m[2]) / 255, Number(m[3]) / 255];
  return r * 0.2126 + g * 0.7152 + b * 0.0722;
}

/** A deterministic sweep of the cube. No `Math.random` — a flaky colour test is worse than none. */
function* cube(step = 51) {
  for (let r = 0; r <= 255; r += step)
    for (let g = 0; g <= 255; g += step)
      for (let b = 0; b <= 255; b += step) yield { r, g, b };
}

describe('plateColor', () => {
  it('is nothing at all when nothing was measured', () => {
    // Not a grey. A book with no measurement gets a different empty state, so
    // that "never measured" and "this jacket is grey" stay distinguishable.
    expect(plateColor(null)).toBeNull();
    expect(plateColor(undefined)).toBeNull();
  });

  it('never exceeds the ceiling, for any colour in the cube', () => {
    // The ceiling is the reachable half: scaling *down* never clips a channel,
    // so this direction is unconditional.
    for (const c of cube()) {
      const out = plateColor(c)!;
      expect(lumaOf(out), `${c.r},${c.g},${c.b} -> ${out}`).toBeLessThanOrEqual(LUMA_MAX + BYTE);
    }
  });

  it('never darkens a colour that was already too dark', () => {
    // The honest floor. A very saturated dark colour cannot always *reach*
    // LUMA_MIN — see the pinned case below — but it must never move away from
    // it, which is the property that actually protects the shelf.
    for (const c of cube()) {
      const before = (c.r * 0.2126 + c.g * 0.7152 + c.b * 0.0722) / 255;
      const after = lumaOf(plateColor(c)!);
      if (before < LUMA_MIN) expect(after).toBeGreaterThanOrEqual(before - BYTE);
    }
  });

  it('cannot lift saturated blue to the floor, and that is the known limit', () => {
    // Pure blue's luma is 0.0722 and its blue channel is already at maximum, so
    // the scale that would reach 0.09 clips and the luma does not move. Pinned
    // rather than papered over: the alternative is desaturating toward white,
    // which throws away the one thing a jacket colour has to keep.
    const out = plateColor({ r: 0, g: 0, b: 255 })!;
    expect(lumaOf(out)).toBeLessThan(LUMA_MIN);
    expect(lumaOf(out)).toBeGreaterThan(0.06);
  });

  it('sends black to the floor rather than dividing by it', () => {
    const out = plateColor({ r: 0, g: 0, b: 0 })!;
    expect(lumaOf(out)).toBeCloseTo(LUMA_MIN, 2);
  });

  it('keeps the hue it was given', () => {
    // A warm jacket stays warm. The whole reason the colour is scaled rather
    // than clipped per channel is that clipping moves hue, and hue is the only
    // thing the measurement is really claiming.
    const warm = plateColor({ r: 200, g: 90, b: 40 })!;
    const m = warm.match(/rgb\((\d+) (\d+) (\d+)\)/)!;
    const [r, g, b] = [Number(m[1]), Number(m[2]), Number(m[3])];
    expect(r).toBeGreaterThan(g);
    expect(g).toBeGreaterThan(b);
  });

  it('always emits bytes a browser will accept', () => {
    for (const c of cube(85)) {
      expect(plateColor(c)).toMatch(/^rgb\((\d{1,3}) (\d{1,3}) (\d{1,3})\)$/);
    }
  });
});

describe('plateShades', () => {
  it('is nothing when there is nothing measured', () => {
    expect(plateShades(null)).toBeNull();
  });

  it('always steps the panel and the rule clear of the plate', () => {
    // The defect this replaced: a mix toward a fixed pole gives dark jackets a
    // crisp composition and pale ones a flat rectangle, or the exact reverse.
    // Every jacket must get a visible step, and the rule a larger one than the
    // panel — that is what makes it read as composition rather than as noise.
    for (const c of cube()) {
      const s = plateShades(c)!;
      const [base, panel, rule] = [lumaOf(s.base), lumaOf(s.panel), lumaOf(s.rule)];
      expect(Math.abs(panel - base), `panel flat on ${c.r},${c.g},${c.b}`).toBeGreaterThan(0.01);
      expect(Math.abs(rule - base)).toBeGreaterThan(Math.abs(panel - base));
    }
  });

  it('steps the same way for the panel and the rule', () => {
    // Both toward the near pole. A panel lighter than its plate with a rule
    // darker than it would read as two unrelated marks.
    for (const c of cube()) {
      const s = plateShades(c)!;
      const [base, panel, rule] = [lumaOf(s.base), lumaOf(s.panel), lumaOf(s.rule)];
      expect(Math.sign(panel - base)).toBe(Math.sign(rule - base));
    }
  });

  it('lightens a dark jacket and darkens a light one', () => {
    const dark = plateShades({ r: 20, g: 20, b: 30 })!;
    expect(lumaOf(dark.panel)).toBeGreaterThan(lumaOf(dark.base));

    const light = plateShades({ r: 235, g: 232, b: 220 })!;
    expect(lumaOf(light.panel)).toBeLessThan(lumaOf(light.base));
  });
});
