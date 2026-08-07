<script lang="ts">
  import '../app.css';

  let { children } = $props();
</script>

<div class="shell">
  <header>
    <a href="/" class="mark">readingbuddy</a>
    <!-- Deliberately no count, no badge, no "unread" total. `docs/decisions.md`
         forbids task-completion framing by name and `gui/CLAUDE.md` sharpens it:
         the app tells you what you did, never what you have left.

         A link is not a number. This is the door to the one page counts live on,
         and it is what makes that page "somewhere you chose to go" rather than
         somewhere the app pushed a figure at you — the whole distinction the
         home-surface rule rests on. It says *reading life*, never *stats*: the
         page is what you did, not a dashboard measuring you.

         Item 47 added the second door on the same terms. It says *cards* — the
         object — and never *finished*, which would make it a tally of a thing
         with an implied other half. -->
    <nav>
      <a href="/cards">Cards</a>
      <a href="/life">Reading life</a>
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
    padding: 0.9rem 1.25rem;
    border-bottom: 1px solid var(--line);
    display: flex;
    align-items: baseline;
    gap: 1rem;
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
    gap: 1rem;
    font-size: 0.85rem;
    color: var(--ink-dim);
  }
  header nav a:hover {
    color: var(--accent-text);
  }
  main {
    flex: 1;
    padding: 1.25rem;
  }
</style>
