/**
 * A rating scale, as a row of buttons.
 *
 * Both failures this guards are invisible on a screenshot: a `step` of zero
 * hangs the webview, and floating-point accumulation sends `set_rating` a value
 * the scale does not contain. Neither shows up as a wrong pixel.
 */
import { describe, expect, it } from 'vitest';

import type { RatingScaleDto } from '$lib/api/bindings';
import { MAX_POINTS, ratingSteps } from './rating';

function scale(over: Partial<RatingScaleDto>): RatingScaleDto {
  return { id: 1, name: 'stars', min: 1, max: 5, step: 1, ...over };
}

describe('ratingSteps', () => {
  it('walks the whole range, ends included', () => {
    expect(ratingSteps(scale({}))).toEqual([1, 2, 3, 4, 5]);
  });

  it('lands exactly on half steps', () => {
    // `1 + 0.5 + 0.5 + 0.5 + 0.5 + 0.5` is `3.5000000000000004` accumulated.
    // Multiplying from the index and rounding to the hundredth is what puts
    // every point back on a value the scale actually contains.
    const steps = ratingSteps(scale({ step: 0.5 }));
    expect(steps).toEqual([1, 1.5, 2, 2.5, 3, 3.5, 4, 4.5, 5]);
    expect(steps.every((v) => Number.isInteger(v * 2))).toBe(true);
  });

  it('handles a zero floor', () => {
    expect(ratingSteps(scale({ min: 0, max: 3 }))).toEqual([0, 1, 2, 3]);
  });

  it('offers one point when the range is a single value', () => {
    expect(ratingSteps(scale({ min: 4, max: 4 }))).toEqual([4]);
  });

  it('refuses a non-positive step rather than looping for ever', () => {
    // `rating_scales.step` carries no `CHECK`, so a hand-written or imported
    // row can hold this — and a `while (v <= max) v += step` here would hang
    // the whole webview, taking the app with it.
    expect(ratingSteps(scale({ step: 0 }))).toEqual([]);
    expect(ratingSteps(scale({ step: -1 }))).toEqual([]);
  });

  it('refuses a reversed range and a non-finite bound', () => {
    expect(ratingSteps(scale({ min: 5, max: 1 }))).toEqual([]);
    expect(ratingSteps(scale({ max: Number.POSITIVE_INFINITY }))).toEqual([]);
    expect(ratingSteps(scale({ step: Number.NaN }))).toEqual([]);
  });

  it('refuses a scale with more points than a row can be', () => {
    // A hundred-point scale is legal to store and illegible to draw. The screen
    // then reports the recorded value instead, which is a refusal that says
    // what it refused.
    expect(ratingSteps(scale({ min: 0, max: 100 }))).toEqual([]);
    expect(ratingSteps(scale({ min: 1, max: MAX_POINTS }))).toHaveLength(MAX_POINTS);
  });

  it('has nothing to offer for a library with no scale at all', () => {
    expect(ratingSteps(null)).toEqual([]);
  });
});
