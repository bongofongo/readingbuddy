<script lang="ts">
  /**
   * A book's front, in whichever of the three states it is actually in.
   *
   * Extracted from `BookTile` by item 27 rather than copied into the book view,
   * and that is the point of the extraction: *"a book with no cover looks like
   * the same book in both frontends"* was a comment, and between two files in
   * this one it was about to become a comment in two places. Now the tile and
   * the detail view cannot drift, because there is one composition.
   *
   * Three states, and the third is not a failure of the second:
   *
   * - **bytes** — the real jacket.
   * - **a measurement, no bytes** — the plate, in *this* book's own colour, as
   *   `procedural_cover` composes it in `render3d/texture.rs`: a base, an inset
   *   panel and two rules.
   * - **neither** — the hatch. A book nobody has measured must not get a grey
   *   plate, or "never measured" and "this jacket is grey" become one picture.
   *
   * It draws no box of its own: the caller owns the aspect, the shadow and the
   * hover, because a tile in a grid and a hero on a page want different ones.
   */
  import type { AccentDto } from '$lib/api/bindings';
  import { plateShades } from '$lib/accent';

  let { src, accent }: { src: string | null; accent: AccentDto | null } = $props();

  // Bounded through `$lib/accent` — never the raw `cover_accent`, which is
  // stored unclamped because a luma band is a renderer's policy about its own
  // lighting rather than a fact about the file.
  const shades = $derived(plateShades(accent));
</script>

{#if src}
  <img src={src} alt="" loading="lazy" />
{:else if shades}
  <span
    class="plate"
    style:--plate={shades.base}
    style:--panel={shades.panel}
    style:--rule={shades.rule}
    aria-hidden="true"
  >
    <span class="panel"><span class="rule"></span><span class="rule short"></span></span>
  </span>
{:else}
  <span class="bare" aria-hidden="true"></span>
{/if}

<style>
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  /* `--plate` is the engine's measured accent, banded by `$lib/accent`. */
  .plate {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    background: linear-gradient(
      160deg,
      color-mix(in srgb, var(--plate) 88%, white 12%),
      var(--plate) 55%,
      color-mix(in srgb, var(--plate) 82%, black 18%)
    );
  }
  /* `--panel` and `--rule` are computed in `$lib/accent`, not mixed here: the
     step has to be away from *this* jacket's lightness, and a stylesheet mixes
     toward a colour named at author time. See `plateShades`. */
  .panel {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    gap: 5px;
    width: 62%;
    height: 58%;
    padding: 0 0 12%;
    background: var(--panel);
  }
  .rule {
    height: 2px;
    width: 74%;
    margin-inline: auto;
    background: var(--rule);
  }
  .rule.short {
    width: 46%;
    margin-inline: auto auto;
  }

  .bare {
    display: block;
    height: 100%;
    background: repeating-linear-gradient(
      -45deg,
      transparent,
      transparent 7px,
      var(--line) 7px,
      var(--line) 8px
    );
  }
</style>
