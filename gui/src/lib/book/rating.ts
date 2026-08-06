/**
 * The points of a rating scale, as a control can offer them.
 *
 * ## What this is, and what it deliberately is not
 *
 * `RatingScaleDto` is `{ min, max, step }` — a *declared range*, not a list.
 * Turning it into the values a row of buttons offers is the same kind of work
 * as drawing an axis from a domain: it is about the control, not about the
 * rating. The **validation** stays where it is, in `Engine::set_rating`, and
 * nothing here decides what a legal rating is; it decides what to draw.
 *
 * ## Why it is a module with a test rather than four lines in a component
 *
 * Two things go wrong here and both are invisible on a screenshot. A `step` of
 * `0` is an infinite loop — the column carries no `CHECK`, so a scale written
 * by hand or by an importer can hold one, and a webview that hangs takes the
 * whole app with it. And floating-point accumulation makes `1 + 0.5 * 5` come
 * out as `3.5000000000000004`, which reaches `set_rating` as a value the scale
 * does not contain.
 */
import type { RatingScaleDto } from '$lib/api/bindings';

/**
 * How many points a control will offer before it stops being one.
 *
 * A hundred-point scale is a legal thing to store and an illegible thing to
 * draw. Past this the screen shows the recorded value and no control, which is
 * a refusal that says what it refused rather than rendering four hundred boxes.
 */
export const MAX_POINTS = 21;

/**
 * The values on the scale, low to high — or an empty list when the scale cannot
 * be drawn as one.
 *
 * Empty is a real answer and callers must handle it: a non-positive step, a
 * reversed range, a non-finite bound, or more points than [`MAX_POINTS`].
 */
export function ratingSteps(scale: RatingScaleDto | null): number[] {
  if (!scale) return [];
  const { min, max, step } = scale;
  if (!Number.isFinite(min) || !Number.isFinite(max) || !Number.isFinite(step)) return [];
  if (step <= 0 || max < min) return [];

  const count = Math.floor((max - min) / step) + 1;
  if (count > MAX_POINTS) return [];

  const out: number[] = [];
  for (let i = 0; i < count; i += 1) {
    // Multiplied from the index rather than accumulated, and rounded to the
    // hundredth: `min + step * i` still drifts, but it drifts once instead of
    // compounding, and the round lands it back on a value the scale contains.
    out.push(Math.round((min + step * i) * 100) / 100);
  }
  return out;
}
