<script lang="ts">
  /**
   * The shell: one row at the top of the window, and nothing else.
   *
   * ## No sidebar, and that is the decision rather than the default
   *
   * A permanent left column is a permanent edge, and the calmest surface is the
   * one with the fewest of them. Four text links do not need a column — and
   * keeping the shell edgeless is what makes the rails that *do* exist (on the
   * book page) unambiguous: a rail there means **you are working**, never
   * **you are navigating**.
   *
   * ## No count, no badge, anywhere in it
   *
   * Unchanged and non-negotiable. `docs/decisions.md` forbids task-completion
   * framing by name and `gui/CLAUDE.md` sharpens it: the app tells you what you
   * did, never what you have left. A link is not a number. *Reading life* is a
   * door to the one page counts live on, which is what makes that page
   * somewhere you chose to go; it says *reading life* and never *stats*, and
   * *cards* — the object — and never *finished*.
   *
   * ## `aria-current`, not a class
   *
   * Which page you are on is a real state and assistive technology should get
   * it. The underline is styled off the attribute for the same reason the
   * links pane carries its direction in text: a state that only exists in CSS
   * is a state no test and no screen reader can see.
   */
  import { page } from '$app/state';

  import '../app.css';

  let { children } = $props();

  /**
   * The four places. Order is deliberate — the shelf, the vault, the wall, the
   * record — and it is the order they were built in as places rather than as
   * screens.
   *
   * **Library and Notes are the two this gained.** The wordmark alone as a home
   * link is discoverable only by guessing, and the vault is being promoted from
   * a band inside one book to a place of its own.
   */
  const NAV = [
    { href: '/', label: 'Library' },
    { href: '/notes', label: 'Notes' },
    { href: '/cards', label: 'Cards' },
    { href: '/life', label: 'Reading life' },
  ];

  /**
   * Whether a nav entry names where you are.
   *
   * `/` matches only itself: every route starts with it, so a prefix test would
   * light *Library* on every page in the app. Everything else matches its own
   * subtree, so `/book/3` is not in the nav at all and lights nothing — which is
   * correct, and is why the wordmark is also a link home.
   */
  function here(href: string): boolean {
    const path = page.url.pathname;
    return href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
  }
</script>

<div class="shell">
  <header>
    <a href="/" class="mark">readingbuddy</a>
    <nav aria-label="Places">
      {#each NAV as n (n.href)}
        <!-- `undefined` rather than `false`: `aria-current="false"` is a valid
             value meaning *not current*, and it would put the attribute on every
             link in the row for a selector to trip over. -->
        <a href={n.href} aria-current={here(n.href) ? 'page' : undefined}>{n.label}</a>
      {/each}
    </nav>
  </header>
  <main>
    {@render children()}
  </main>
</div>

<style>
  .shell {
    min-height: 100%;
    display: flex;
    flex-direction: column;
  }
  header {
    /* The shell's own row is full-bleed and its *contents* are centred, so the
       rule under it runs to the window edge while the wordmark lines up with
       the first tile below it. */
    width: 100%;
    max-width: var(--shell);
    margin-inline: auto;
    padding: 1rem 2rem;
    display: flex;
    align-items: baseline;
    gap: 1.5rem;
  }
  .mark {
    font-weight: 600;
    letter-spacing: 0.01em;
    /* **`--accent-text`, not `--accent`.** `app.css` states this exact pair and
       this exact number as the whole reason the second token exists: `--accent`
       on `--bg` measures 2.78:1, and the wordmark was the one string in the app
       still using it. Found by item 47's screenshot review, in every shot. */
    color: var(--accent-text);
  }
  header nav {
    margin-left: auto;
    display: flex;
    gap: 1.4rem;
    font-size: 0.85rem;
  }
  header nav a {
    color: var(--ink-dim);
    padding-bottom: 0.25rem;
    border-bottom: 1px solid transparent;
  }
  header nav a:hover {
    color: var(--ink);
  }
  /* Where you are, said twice — in ink and with a rule under it — because the
     accent is spent on state you can act on and *this is the page you are on*
     is exactly that. The border is on both states so lighting one does not
     shift the row. */
  header nav a[aria-current='page'] {
    color: var(--ink);
    border-bottom-color: var(--accent);
  }
  main {
    flex: 1;
    width: 100%;
    max-width: var(--shell);
    margin-inline: auto;
    padding: 0.5rem 2rem 3rem;
  }
  @media (max-width: 860px) {
    header {
      padding: 0.9rem 1.25rem;
      gap: 1rem;
      flex-wrap: wrap;
    }
    header nav {
      gap: 1rem;
    }
    main {
      padding: 0.5rem 1.25rem 2rem;
    }
  }
</style>
