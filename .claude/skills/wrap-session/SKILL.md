---
name: wrap-session
description: End-of-session wrap-up for this repo — verify the build, write a session log capturing decisions/discoveries/gotchas from the conversation, and commit everything to main. Use when the user says "wrap up", "commit and write a session log", "/wrap-session", or asks to close out the session.
---

# wrap-session

Close out a working session in three steps: **verify → log → commit**. Run the
steps in order; do not commit if verification fails.

## 1. Verify

Launch the `cargo-tester` agent to run `cargo test --workspace` and
`cargo clippy --workspace --all-targets`.

- Failures or clippy warnings → report them to the user and STOP. Never commit
  a red tree during wrap-up; fixing is a separate decision.

## 2. Session log

Write `sessions/YYYY-MM-DD-<short-topic-slug>.md` (date = today; if a log for
today + same topic already exists, append a `## Addendum` section instead of
creating a second file).

Content — mine the WHOLE conversation, not just the last task. Prioritize
what a future session would otherwise have to rediscover:

- **Decisions locked** — choices the user made and the why (one line each).
- **Bugs found** — especially pre-existing ones and how they were fixed.
- **Technical gotchas** — API quirks, library traps, environment surprises.
  These are the highest-value entries; be specific enough to act on
  ("contentless FTS5 can't DELETE" beats "FTS was tricky").
- **Verification** — what was tested and how (test counts, live smoke results).
- **Deferred** — what was consciously left for later.

Style: terse bullets, no narrative padding. If a gotcha changed a convention,
also check it's reflected in `CLAUDE.md` (update if not).

## 3. Commit to main

- Stage everything relevant: `git add -A` (review `git status` first; leave
  obviously-unrelated stray files unstaged and mention them).
- Split into logical commits when the session contained clearly separable
  chunks (e.g. feature vs docs); otherwise one commit is fine. The session
  log + any skill/doc updates can ride in the last commit.
- Message style: match this repo's history — short, lowercase, plain
  ("rebuilt as workspace: engine lib + thin cli"). Body only when the why
  isn't obvious. Always end with:

  ```
  Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>
  ```

- Commit directly to `main` (this repo's convention — no feature branches
  unless the user asks). Do NOT push unless the user asks.

## 4. Report

Tell the user: test/clippy status, session log path, commit hash(es) +
one-line summaries. Mention anything left unstaged and why.
