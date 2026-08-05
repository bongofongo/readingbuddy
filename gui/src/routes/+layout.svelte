<script lang="ts">
  import '../app.css';

  let { children } = $props();
</script>

<div class="shell">
  <header>
    <a href="/" class="mark">readingbuddy</a>
    <!-- Deliberately no count, no badge, no "unread" total. `docs/decisions.md`
         forbids task-completion framing by name and `gui/CLAUDE.md` sharpens it:
         the app tells you what you did, never what you have left. -->
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
    color: var(--accent);
  }
  main {
    flex: 1;
    padding: 1.25rem;
  }
</style>
