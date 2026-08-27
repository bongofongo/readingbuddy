<script lang="ts">
  /**
   * The root, and it is deliberately almost nothing: the stylesheet, and
   * whatever the group below asked for.
   *
   * ## Why there is a group at all
   *
   * Everything in this app used to hang off one layout that always drew the
   * header row. Reading mode (item 54) is the first surface that must **not**
   * have it — the whole claim of that route is that at rest you see the book and
   * the four things you can do to it, and a permanent row of places to go
   * elsewhere is the opposite of that claim.
   *
   * The two ways to get there are a conditional in the shell (*if not
   * `/reading`, draw the header*) and a route group. The group is taken because
   * the conditional puts knowledge of reading mode into the chrome that reading
   * mode exists to escape — and because it would have grown a second arm the
   * first time any other surface wanted the window to itself.
   *
   * So `(shell)/` is the app with its header, `reading/` is the app without one,
   * and **the URLs are unchanged**: a group's directory name is not a path
   * segment. `/`, `/book/3`, `/cards`, `/notes` and `/life` are where they were.
   *
   * Nothing else belongs here. A token, a nav item, a `main` element — all of
   * that is the shell's, and a fact that lived here would be a fact reading mode
   * silently inherited.
   */
  import '../app.css';

  let { children } = $props();
</script>

{@render children()}
