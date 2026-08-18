<script lang="ts">
  /**
   * The wall: jackets, face out, cut into the groups the arrangement asked for.
   *
   * ## The three numbers on it, and what each one answers
   *
   * **86px minimum, not 132.** This is the single biggest change on the home
   * surface and it is the direct answer to "the books are too large and take too
   * much of the screen". It is also well above the perceptual floor for what the
   * wall is mostly doing: humans recognise scenes at over 80% from 32×32 images,
   * and re-finding a book you have read is recognition — you already hold a
   * template of the cover, and top-down guidance makes it pop out at thumbnail
   * sizes carrying no legible text at all.
   *
   * **The row gap is 1.85× the column gap**, not 1.36×. Books on a shelf sit
   * close together horizontally and shelves are far apart vertically; equal gaps
   * read as a spreadsheet. But proximity works on *ratios*, and a ratio in the
   * low 1.3s registers as "slightly uneven" rather than as rows — paying
   * vertical space for an effect that does not land.
   *
   * **A fixed 2:3 box, and the jacket letterboxes inside it.** Cropping is
   * disqualified here for a reason that is specific to books: the printed title
   * is the item's only textual identifier at this size, and cropping cuts it off
   * the edges. Letterboxing is slightly ragged and honest about a non-standard
   * asset. The box also has to exist *before* the image does — a cover grid is
   * the worst case for layout shift, and images of undeclared dimension are its
   * first listed cause.
   *
   * ## Where the spine shelf plugs in
   *
   * Item 19 and item 26's WebGL shelf is deferred, not cancelled, and this is
   * the seam it lands in: a group's field is one component's worth of markup
   * below, and the headings, the runs and the empty states are not its problem.
   * That is a narrower contract than the old whole-band layout seam, and it is
   * the one the ray tracer wants — books in, an event out, never a per-frame
   * boundary.
   */
  import BookTile from '$lib/components/BookTile.svelte';

  import type { ShelfGroup } from './arrangements';

  let { groups }: { groups: ShelfGroup[] } = $props();
</script>

{#each groups as group (group.key)}
  <section class="group">
    <!-- The heading and a hairline running to the right margin. The rule is
         decoration and the gap is what actually separates the groups: `--line`
         measures Lc 0.0 on the dark theme and 13.3 on the light, both at or
         below the point of invisibility. It is drawn because a shelf has an
         edge, not because the wall would be ambiguous without it. -->
    <h3 class="band-title">{group.heading}</h3>
    <div class="field" class:captioned={group.captions}>
      {#each group.books as book (book.id)}
        <BookTile {book} caption={group.captions} />
      {/each}
    </div>
  </section>
{/each}

<style>
  .group + .group {
    margin-top: 2.6rem;
  }
  h3 {
    display: flex;
    align-items: center;
    gap: 0.9rem;
    margin-bottom: 1.1rem;
  }
  h3::after {
    content: '';
    flex: 1;
    height: 1px;
    background: var(--line);
  }
  .field {
    display: grid;
    /* `auto-fill`, not `auto-fit`: with `auto-fit` a group of three books
       stretches them to the full width of the window, and a jacket the size of
       a poster is not what a shelf looks like. Empty tracks stay empty. */
    grid-template-columns: repeat(auto-fill, minmax(86px, 1fr));
    gap: 2.4rem 1.3rem;
    align-items: start;
  }
  /* Where the caption is on, the row gap carries a text block as well as the
     gutter between shelves, so it does not need to be as tall to read as one. */
  .field.captioned {
    gap: 1.8rem 1.3rem;
  }

  /* 320px is this app's extreme, and the terminal sibling's
     `every_screen_draws_at_every_size` exists because the layout bug always is
     one. Two tracks at 76px rather than one enormous jacket. */
  @media (max-width: 420px) {
    .field,
    .field.captioned {
      grid-template-columns: repeat(auto-fill, minmax(76px, 1fr));
      gap: 1.6rem 1rem;
    }
  }
</style>
