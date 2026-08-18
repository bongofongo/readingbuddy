---
title: The research, applied — readingbuddy's layout re-derived
date: 2026-08-14
status: argument, with positions taken. Nothing settled until you settle it.
source: `docs/gui/design-research.md` for the evidence behind every claim here;
        `docs/gui/layout-redesign.md` for the design being tested;
        `docs/gui/gui-vision.md` for the axiom.
---

> **Settled.** Every position in this document was adopted, including all five
> answers in Part 7. What shipped is `docs/decisions.md` entry 53; the two items
> in Part 6 that are not layout — a written notification-level spec, and undo
> before accelerators — are still open and are engine-shaped rather than
> frontend-shaped.

# The research, applied

## How I'm reasoning

Three tests, applied to every decision below.

1. **What does the evidence actually say, and at what strength?** Most of the
   design canon is craft consensus. Craft consensus is real evidence — but a
   decision resting on it is arguable in a way a decision resting on a measured
   effect is not, and it should be *held* differently.
2. **Who pays?** Every layout principle is a trade with a named victim. A
   position that does not name its victim is not a position.
3. **Does the mechanism survive the translation?** Most findings come from a
   different context — print, aviation displays, e-commerce, a browser. The
   question is whether the *mechanism* transfers, not whether the finding
   sounds relevant.

Where I take a position I say so. Where the evidence does not reach, I say that
instead — there are four places below where the honest answer is "nobody knows,
pick and move on," and pretending otherwise would be the failure mode this whole
exercise is meant to avoid.

**Two things this document does that the research digest deliberately did not:**
it computes your actual palette against both contrast algorithms, and it answers
the five open questions at the end of `layout-redesign.md`.

---

# Part 1 — The axiom, stress-tested

> **The app tells you what you did. It never tells you what you have left.**

## 1.1 The strongest empirical support in your docs is for this line

The axiom is a precise restatement of a distinction that a 128-experiment
meta-analysis arrived at independently.

Deci, Koestner & Ryan (1999) separate the **informational** from the
**controlling** aspect of a reward. Feedback that tells you *that you did well*
supports competence — and verbal rewards and positive feedback measurably
*enhance* intrinsic motivation. A reward *contingent on doing the thing*
announces that the thing is a means to the reward, and engagement-, completion-
and performance-contingent tangible rewards all significantly undermined
free-choice intrinsic motivation (≈ −0.34 overall for tangible rewards).

**"What you did" is informational. "What you have left," measured against a
target the system set, is controlling.** You wrote the rule from taste. The
literature got there from experiments. That convergence is worth more than either
alone, and it is the single best-supported decision in `gui-vision.md`.

Two corollaries you already got right, and now have mechanisms for:

- **Streaks in the past tense are permitted; live counters are not.** The
  research explains exactly why: a streak is a completion-contingent reward *with
  a loss-aversion multiplier* — its force comes from the pain of breaking, not
  the pleasure of continuing, which makes it the most controlling form on the
  list. Strip the contingency and you are left with a fact about March.
- **"You have read 43 books" versus "43 of 50."** Self-determination theory's
  three needs are autonomy, competence, relatedness. The first sentence serves
  competence with no contingency attached. The second attaches one.

## 1.2 But the wording is stronger than the rule you want

Here is the problem. `gui-vision.md` says *"it never tells you what you have
left."* Held literally, that forbids:

- "p. 214 of 502" — which you already permit, and correctly, because it describes
  one book you chose to open.
- "12 pages left in this chapter" — a fact, useful, with no target in it.
- "3 out · 2 in" on a note — which you permit on `/notes`, again correctly.

You have already had to bolt two qualifiers onto the axiom to make it survive
contact with the design: *"a number on the home surface may describe one book you
chose to open"* and *"counts live on a page you deliberately visit."* **Both
qualifiers are patches on a phrasing that names the wrong thing.**

The research names the mechanism cleanly. From §30, on the soft end of deceptive
design:

> The mechanism is not the number; it is that **any number displayed against an
> implied target converts a place into a ledger.**

And on the opposite failure, which is the one your current phrasing risks:

> A strict no-numbers rule can itself become a dark pattern of omission.
> Withholding information the user actively wants, in service of the designer's
> aesthetic of calm, is paternalism — and paternalism is what "autonomy" in
> self-determination theory is against.

**Position: restate the axiom so it names the mechanism rather than the symptom.**

> **The app states what happened. It never sets a target, and it never counts
> against one.**

Test it against everything you want to forbid, and it holds: goals (a target),
live streaks (counting against one), the orphan queue (counting against an
implied target of zero), "3 books unrated" (same), an unread badge (same), a
highlight inbox (same). Test it against everything you want to permit, and it
also holds, *without needing either qualifier*: `p. 214 of 502`, `3 out · 2 in`,
`you read on eleven days in March`, a card, the wall.

The old wording needed the "page you chose to open" carve-out because it
mis-identified *numbers* as the danger. The danger is *targets*. Once you name
that, the carve-out is unnecessary — and you stop having to relitigate every
number that appears.

**What this costs:** it is a weaker-sounding rule, and weaker-sounding rules are
easier to erode. "Never tells you what you have left" is a better slogan. If the
slogan's rhetorical force is doing real work on your own future decisions, keep
it as the gloss and put the sharper version underneath as the operative test.

## 1.3 The word "reward" is working against you

`gui-vision.md` names three **reward registers**: finishing, thinking, returning.

Here is the uncomfortable finding. Sailer & Homner's 2020 meta-analysis of
gamification found positive effects across the board — cognitive g = 0.49,
motivational g = 0.36, behavioural g = 0.25. **But their own high-rigour subgroup
analysis is the result that matters: restricted to methodologically strong
studies, the cognitive effect survived and the motivational and behavioural
effects became non-significant.**

The surviving effect is about *learning*. readingbuddy's registers are not
learning interventions. **So there is no evidence-backed case that the reward
registers produce motivation.** And Deci et al.'s risk profile is the worst
possible one for this app: the undermining effect is largest precisely where
intrinsic motivation already exists, on voluntary self-directed activity with
high pre-existing interest. Which is reading.

**But look at what you actually built.** A card is not earned against a
threshold. Nothing has "a threshold announced in advance." You "find out what you
earned by having earned it." **That is not a reward system. It is a record
system.** The card is an artefact minted by an event, like a receipt or a
photograph — and the motivational literature has nothing bad to say about
artefacts, only about contingencies.

**Position: rename the concept from *reward registers* to *records*.** This is a
change in vocabulary, not in design, and it is worth making for one reason: the
word "reward" invites contingency, and contingency is the thing the evidence
punishes. In eighteen months someone — possibly you — will read "reward register"
and reasonably infer that a register ought to have a threshold, a tier, a
progression. The word is a slow leak toward the exact design the meta-analysis
warns about.

*(This is the argument, incidentally, for why "congratulatory things" in the
original brief survives and "set goals" does not. A congratulation is a statement
about an event. A goal is a contract.)*

## 1.4 Three structural deficits the axiom does not catch

The axiom governs what the app *says*. It does not govern what the app's
*structure implies*, and there are three places where the structure makes a
deficit statement no sentence in the app ever makes.

### (a) "Reading now" is a continue shelf, and continue shelves become ledgers

From §23 of the research:

> The shelf is populated by **starting** and drained only by **finishing**. Since
> people start far more than they finish, its steady state is a queue of things
> abandoned.
>
> The observable evidence that this is real is that **every major implementation
> eventually shipped a manual removal affordance** — Netflix, Plex, Spotify and
> Steam all added "remove from this row" *after the fact*, which is a confession
> that the automatic population rule was wrong.

Your band is `repeat(auto-fit, minmax(310px, 1fr))` with no cap. At 1440px that
is four across and then it wraps. **A reader with nine open readings gets a
two-and-a-half-row wall of started-and-not-finished books, at the top of the home
surface, above the shelf.** No number appears. The arithmetic is done by the
user, on the highest-salience region of the page, every time they open the app.

This is the axiom's largest live exposure, and `layout-redesign.md` does not
address it — the section discusses states for zero and for missing data, but not
for *many*.

**Position, three parts:**

1. **Cap the band at one row — four previews maximum.** The research's range for
   a continue row is three to five. Four falls out of your own grid at the target
   width, so the cap and the layout agree.
2. **Order by recency of the latest mark, not by start date.** This is the
   research's specific recommendation and it is the one that does the work: stale
   readings fall off the visible end *without any act of dismissal*, so nothing
   ever requires the user to decide, on the home screen, whether they are going to
   finish something. It also happens to be the ordering that makes the band most
   useful — the book you touched yesterday is the book you are reading.
3. **Cap silently. No "and 3 others."** That would be a count of what is left,
   and it would be the only such count in the app. The overflow books are on the
   shelf immediately below, showing `Reading · 35%` under their tiles. Nothing is
   hidden; it is just not promoted.

**And note the asset you have that Netflix does not.** Those products had to bolt
on "remove from row" because they had no non-judgemental way to drain the queue.
**You have `put down` as a first-class, non-failure state.** The drain already
exists in the data model and is already framed correctly. It does not need to be
on the home surface — recency ordering handles the home surface — but it means
the ledger has an honest exit, which is more than the pattern usually gets.

**What the cap costs:** the fourth-most-recent book you were reading loses its
preview, and the research names this exactly — "a short capped row is calm but
drops the fourth thing you were reading, which is exactly the item you most need
help resuming." I think the trade is right here because the shelf is *directly
below* and carries the state, but it is a real loss and you should know you are
choosing it.

### (b) The arrangement switch is not axiom-neutral

This is the finding I did not expect, and I think it is the most important thing
in this document.

Year grouping does something you have not stated explicitly: **it quarantines
books with no reading into a single group at the bottom of the wall.** Under
`Year`, the "No reading recorded" group is one region, below every year, in the
least-attended part of a long scroll. Books you have read and books you have not
are spatially separated, and the wall reads as an accumulating record with an
appendix.

**Under `Author` or `Title`, they interleave.** A Goodreads `to-read` import sits
between two books you finished, identical in every respect except that it lacks
the thing the others have. The wall stops being a record of reading and becomes a
**mixed field of read and unread** — which is the textbook backlog rendering, and
it arrives without a single number or label changing.

The research makes the mechanism explicit. From §24, on Belk:

> **The set, once perceived as a set, generates its own pressure to complete.** An
> app that displays "unread" as a *state* rather than an *absence* has, without
> saying anything, implied a task.

And:

> The calm reading of an accumulating archive depends on a sort order that is
> itself a value judgement; **the moment the user is allowed to sort by "date
> added," the same grid becomes a to-do list, and the app has no way to stop
> that.**

`layout-redesign.md` defends the "No reading recorded" group carefully — "no
count, no call to action, no styling that marks it as unfinished business." That
defence is about *styling*, and styling is not the mechanism. **Adjacency is the
mechanism.** Under Year you have adjacency on your side for free. Under
Author/Title you have thrown it away and are relying on the styling to hold the
line, which the research says it will not.

**Position: books with no closed reading appear only under `Year`, in the "No
reading recorded" group. Under `Author` and `Title`, the wall shows the reading
life only.** They are not deleted, not hidden behind a toggle, not counted — they
are simply not what those two arrangements are arrangements *of*. `Author`
answers "whose work have I read"; a book you have not read has no answer to
contribute.

*(If that feels like data disappearing: it is the same discipline as
`goodreads.rs` refusing to invent a start date. An arrangement is a question, and
a book with no reading has no answer to this one.)*

**The counter-argument, honestly:** a user who imported 400 Goodreads `to-read`
books and sorts by Author to find one of them now cannot. That is a real
retrieval need and it is not served by the shelf under this proposal. **But it is
better served by `/notes`-style search than by a wall** — you are looking for a
known item by name, which §20 says is a list task, not a grid task. If this
becomes a real complaint, the answer is a search affordance, not restoring the
interleave.

### (c) The moment is your one "demand attention" event, and that should be deliberate

The research offers a scale (§27): **ignore → change blind → make aware →
interrupt → demand attention.** Its discipline is per-element: *for each thing,
decide what happens if the user never notices it.*

Run readingbuddy through it and almost everything sits at the bottom — which is
the design working. A new backlink appears: change blind. A book moves to a
different year group: change blind. The latest-mark line updates: change blind.
Nothing bad happens if any of these go unseen.

**The moment is the exception. It is at "demand attention," and it is the only
thing in the app that is.** By the research's own test: if the user never notices
the moment, they do not write a reflection. Is that bad?

Under the axiom, arguably not — the app does not get to require reflection. But
the moment is *designed* to make reflection happen, which means it is the one
place the app sets an implicit expectation. **Position: that is defensible, and
it should be conscious rather than accidental.** Two guardrails from the
research:

- **It fires once and never again.** You already specify this. Brignull's
  durability test is exactly this: *a request that respects "no" permanently is
  guidance; one that asks again next week is nagging.* Firing once is what keeps
  the moment on the right side of that line.
- **Its payload is an open cursor, not a prompt.** Also already specified.
  "Congratulations, then write final thoughts" is Weiser's recentering — the
  periphery moving to the centre on a genuine change of state. A dismissable
  dialog would be the same notification level with none of the value.

**What I would add:** a written notification-level assignment for every element
that changes. Not because any current element is wrong, but because §27's real
observation is that **most interfaces have no such spec, so every new feature
defaults upward — because the person shipping it wants theirs seen.** You have
one at "demand attention" today. The spec is what stops there being three in a
year.

---

# Part 2 — The library, re-derived

## 2.1 86px tiles: supported, with an asymmetry you are not serving

**The floor is far below 86px.** Torralba, Fergus & Freeman measured humans at
**over 80% scene recognition on 32×32 colour images** — a drop of only ~7
percentage points from full resolution on 1/64th of the pixels. At 86px wide and
~129px tall you are four times that in each dimension.

**But the finding is about recognition, not identification, and that distinction
decides your caption question.** From §21:

> **Re-finding a book you have read is recognition.** You already hold a template
> of that cover — dominant colour, layout gestalt — and Guided Search's history
> and top-down guidance make it pop out at thumbnail sizes carrying no legible
> text at all.
>
> **Identifying a book you have never seen from cover art alone is not
> recognition.** The cover is an arbitrary symbol and the only real identifier is
> the title.
>
> **Tile size encodes an assumption about how well the user knows the
> collection.**

**Your shelf contains both populations, and they are already spatially
separated.** The year groups are, by construction, books you have read — pure
recognition, and 86px is generous for it. The "No reading recorded" group is, by
construction, books you have *not* read, many of which arrived in a CSV and which
you may never have seen. **That group is a discovery surface wearing a
recognition surface's clothes.**

**Position: 86px stands.** It is well above the perceptual floor for the task the
wall is mostly doing, and it is the direct answer to "too large, too much of the
screen."

**One arithmetic note worth checking against the prototype.** `layout-redesign.md`
says 86px "gives eight or nine books a row instead of five" at 1440. On my
arithmetic with `repeat(auto-fill, minmax(86px, 1fr))`, a 1.4rem column gap, the
1400px max-width and 2rem page padding, it gives **twelve across at ~91px each**;
at 1024 it gives nine. If the prototype shows eight or nine at 1440 then one of
the padding, the gap or the max-width differs from the spec, and the two documents
disagree. Worth reconciling, because twelve-across is a materially denser wall
than the doc is describing — denser than "the density a shelf actually has," and
possibly *past* the point where the jacket reads as an object rather than a
texture.

## 2.2 Captions: the answer is per-band, not a setting

This is open question #2, and I think the research resolves it in a way neither
of your two options anticipated.

**The test, from §22:** a caption earns its space when the image alone cannot
identify the item. **Book covers are an unusual case because the title is printed
on the artwork** — so a cover *at sufficient size* is self-captioning and a
caption below is redundant; a cover *below* that size is not, and the caption is
the only identifier.

At ~91px wide, title type on a typical trade cover renders somewhere around
8–14px, often reversed, often in a display face, often over an image. **That is
below reliable legibility.** So your tiles are *not* self-captioning.

But that only matters for identification, and identification is only the task for
books you do not know. Which are exactly the books in one group.

**Position: captions off, globally, except in "No reading recorded," where they
are on.** Not a setting.

The reasoning:

- The year groups are recognition surfaces. A caption there adds a text row to
  every tile, doubles vertical space, introduces truncation decisions, and — per
  §20 — turns a wall of images into a mixed image-plus-text surface that scans
  worse than either pure form. It buys nothing, because you already know these
  books.
- "No reading recorded" is an identification surface. A caption there is the only
  thing making the group usable, and it is also the group most likely to contain
  missing or wrong cover art, since nothing in your pipeline guarantees a cover
  for an unread Goodreads import.
- **The inconsistency is not arbitrary — it tracks a real difference in the
  task**, which is the strongest kind of inconsistency to have. And it is
  explicable in one sentence to anyone who asks.

**Against making it a setting:** §25's reframe is the one I would hold onto —
*"a density control is not a way to avoid choosing. It is a way to make one
choice survivable."* A captions toggle here would be a way to avoid choosing,
and it would double the layout states to design and test while interacting badly
with tile size (a caption that fits at 91px truncates at 86px, and both truncate
differently from what you mocked). If you later ship a density control with three
designed steps, **captions should be a property of the step, not an independent
axis.**

**What this costs:** two tile components instead of one, and a wall that is not
uniform. If uniformity of the wall is itself the point — and there is a real
aesthetic argument that it is — then the alternative position is captions off
everywhere and "No reading recorded" gets a different treatment entirely, or does
not appear on the wall at all. I would not fight hard for my version over that
one; I would fight hard against *captions on everywhere*, which spends the
scanability of the whole wall to serve one group.

## 2.3 Aspect ratio: letterbox, never crop — and CLS gives you a second reason

`layout-redesign.md` does not state a strategy for covers whose aspect ratio is
not 2:3, and your library will be full of them: trade paperbacks near 1:1.5,
mass-market near 1:1.6, manga and art books well outside both, plus whatever
resolution four different providers happened to supply.

Four strategies exist, and for books specifically one is disqualified:

| strategy | verdict for readingbuddy |
|---|---|
| **Crop to a fixed box** | **No.** Cropping loses title text at the edges. On a surface where the printed title is the item's only textual identifier (§2.2), cropping destroys the identifier. |
| **Letterbox in a fixed box** | **Yes.** Safe, slightly ragged, wastes a little space. The raggedness is the cost of honesty about a non-standard asset. |
| **Justified rows** (vary row height per row) | No — it "destroys the column rhythm that makes a shelf feel like a shelf," which is the one thing this wall is for. |
| **Masonry** | No — abandons rows, and with them horizontal comparison *and predictable keyboard navigation*. See §3.3 on why keyboard structure matters more here than it looks. |

**And there is an independent argument for the fixed box that has nothing to do
with aesthetics.** A cover grid is the worst case for layout stability — many
images of uncertain dimension arriving at uncertain times — and *images without
declared dimensions are the first listed cause of Cumulative Layout Shift*
(threshold: ≤0.1 good, >0.25 poor). **The tile's box must be reserved before the
image exists.** A fixed aspect ratio is what reserves it.

Two decisions arrive together at no extra cost: the letterbox field is a place
for the typographic plate to live when there is no cover at all, and the reserved
box is what makes the plate and the image occupy identical space so nothing moves
when one replaces the other.

## 2.4 Year grouping: strongly supported, and `Recent` should go

**On grouping by the reading's finish year — this is the best-supported layout
decision in the redesign.** Photo apps default to time universally, and not from
laziness. Time requires **zero user maintenance**, can **never be empty**, never
needs a taxonomy decision, **maps onto autobiographical memory** (people remember
*when* far better than which folder), and **degrades gracefully** — a
chronological archive with a million items still has a meaningful top.

Taxonomy has none of those properties. Your instinct to group by nothing, then by
time, is the same conclusion Apple and Google reached from a much larger sample
of users than either of us has.

And the psychological half is precisely your case:

> **Sorted by date finished, descending, with no counts, it reads as an
> accumulating record of things done. Sorted by date added with unread items
> foregrounded, it reads as a backlog.**

You picked the first. That is the "look back and feel accomplished" mechanism,
delivered with no digits, exactly as `layout-redesign.md` claims.

**Position on open question #3: drop `Recent`.**

`Recent` is one of two things and both are bad. If it means recency of *finishing*,
it is redundant — `Year` already puts the most recent finishes at the top, which
your own doc notes. If it means recency of *adding*, it is **the backlog sort by
name**, and it is the single arrangement most likely to convert the wall into a
to-do list.

There is a third possible reading — recency of *interaction* — which would be
genuinely useful. But that is what the "Reading now" band is for, and it would be
a second, competing answer to the same question on the same page.

**What dropping it costs:** a user who just imported forty books has no way to see
what arrived. That is a real want, and I would serve it somewhere that is not the
wall — an import result screen, or `/notes`-style search. Not a permanent
arrangement of the home surface.

## 2.5 The gap asymmetry promises something it does not deliver

`gap: 1.9rem 1.4rem` — row gap larger than column gap, on the reasoning that
"books on a shelf sit close together horizontally and shelves are far apart
vertically. Equal gaps read as a spreadsheet."

I like the intent. But the ratio is **1.36×**, and the research on grouping is
specific about thresholds:

> If the gap *inside* a group is not clearly smaller than the gap *between*
> groups, no border, background or colour reliably fixes the reading.

Proximity is the strongest grouping cue you have, and it works on *ratios*, not
on differences. 1.36× is in the range where the eye registers "slightly uneven"
rather than "these are rows." You are paying vertical space for an effect that
probably does not land.

**Position: either commit (≈1.8–2×, e.g. `2.4rem 1.3rem`) or drop the asymmetry
and take the vertical space back.** I lean toward committing, because the shelf
metaphor is doing real work for the "feel accomplished" goal and rows-as-shelves
is how that reads. But this is exactly the kind of claim to settle by
screenshotting three ratios rather than by argument — it is a perceptual question
with a cheap experiment attached, and `screenshot-reviewer` already exists.

*(A caveat on the whole class of decision: the row gap is not carrying semantic
grouping here — the wrapping rows are not meaningful units — so nothing breaks if
it fails. This is an aesthetic claim, not a structural one, and should be argued
at that weight.)*

---

# Part 3 — The book page, re-derived

## 3.1 The three columns are justified by two different arguments, and it matters

`layout-redesign.md` treats the three columns as one move. They are not, and
separating them tells you which one is fragile.

**The right rail is an inspector, and it is the strongest element in the
redesign.** Canvas-plus-inspector inverts master–detail: the *canvas* is the work
and the rails are instruments. The "Link to…" search that inserts `[[Title]]` at
the cursor is not reference material sitting beside the editor — **it is an
instrument acting on the editor**, which is precisely the configuration that
justifies permanent screen area. Split-attention research says integrate exactly
what must be processed together; writing a note and finding the note to link to
*is* one operation. The doc's own framing is right: *"a graph you can see while
writing into it is a different tool from a graph you have to go and look at."*

**The left rail is a mode selector, and it is justified by a different
argument** — the same one that makes VS Code's activity bar work (§50). It
converts an unbounded set of destinations into a fixed column, and **density is
staged**: at rest you see one work surface, never all of them. The centre swaps;
the rail is how you swap it. That is structurally sound.

**Where it gets exposed is the rail-blindness finding** (§17):

> A rail that is always there, always the same, and only sometimes relevant will
> be learned as wallpaper. **The failure mode of a persistent rail is not clutter.
> It is that the one time it matters, it is invisible.**
>
> The escape is not a position on that axis but a change of kind — **make the
> region change when it has something to say, since habituation is to constancy,
> not to presence.**

Your left rail passes this, but only because of one section. `Write` and
`The book` are constant — they will habituate into wallpaper, and that is fine,
because habituation to a mode selector is exactly what you want (it is Raskin's
argument: habituation *is* expert speed). **`What you wrote` is the section that
changes**, and it is what keeps the rail alive as a region rather than a fixture.

**Position: keep all three columns. But the left rail's note list must be the
part that changes visibly when a note is added** — it is doing double duty as the
rail's anti-habituation mechanism, and that is a load-bearing job nobody assigned
it.

**The count against you.** Header + left + centre + right is four regions;
convergent evidence puts attended regions at about three plus periphery, and
Hutchings's logs found a median of three visible windows on a single monitor.
**You are at the ceiling.** The mitigation is real — the right rail's contents are
conditional on the centre state, so at any moment you are attending centre plus
*one* rail — but that means the other rail is the peripheral one and the design
should say which. Today both are `position: sticky` and both are always
populated, which declares neither.

I do not have a clean answer here, and I would rather say so than invent one. The
obvious move — demote the left rail while a note is open — requires a visual
change you have banned this phase. **Note it as a known risk and look at it in the
phase that allows motion.**

## 3.2 The 52px hero: unqualified support, and the argument is older than you think

This is epicenter design, and 37signals stated the reasoning in two pages in 2006:

> If you're designing a page that displays a blog post, the blog post itself is
> the epicenter. **Not the categories in the sidebar, not the header at the top,
> not the comment form at the bottom.**

Their mechanism is the one that matters for you: **chrome is cheap to add and
expensive to remove, because once a region exists it acquires occupants. Every
feature with no natural home gets filed there, and the region silently becomes the
app's junk drawer.** A 150px hero is a region with nothing to do; a 52px jacket
plus one metadata line is a label.

Your own justification — *"a book page is not a product page; you already know
which book you opened"* — is the same argument. Recovering ~130px of vertical at
the top of the surface where the time goes is the highest-leverage single change
in the redesign. **No caveat.**

## 3.3 The hover-revealed passage actions are fine; the tab stops are not

**On the hover reveal itself: supported.** This is progressive disclosure, and its
measured price — 39% slower, >20% discoverability loss — is a price paid for
*navigation*, not for per-item actions repeated forty times. The research's
failure mode is "the wrong things hidden," and `Annotate` / `Capture a word` /
`Cite` on every passage is the textbook case *for* hiding: three buttons × forty
passages is exactly the "too much happening" the brief objects to, and the
passage is the content while the controls are not.

**On the implementation: there is a defect, and it is the same defect twice.**

`opacity: 0` does not remove an element from the tab order or from the
accessibility tree. So a keyboard user tabbing down the passage list gets **three
tab stops per passage, on invisible buttons.** Forty passages is 120 stops, most
of them landing on something with no visible focus indicator because the button
itself is transparent. `:focus-within` will reveal the group once focus lands
inside — but the first Tab into an invisible control is a focus event the user
cannot see, and that is a 2.4.7 failure in substance if not in letter.

**The fix is not a fix to the reveal. It is a fix to the list.** From §42:

> Tab and Shift+Tab move **between** components; arrow keys move **within** them.
> A composite widget contributes exactly **one** stop to the document tab
> sequence, no matter how many items it contains.
>
> **Tab-stop count is the real ergonomic metric of a keyboard interface, and
> almost nobody measures it.**

**Position: the passage list should be a composite widget — one tab stop, arrow
keys between passages, and the actions reachable by key once a passage is
active.** That takes the page from ~120 stops to ~1 for that region, and it makes
the hover reveal correct for free, because the actions become properties of the
active passage rather than independent controls.

**Three things this buys at once, which is why I would prioritise it:**

1. It is the accessibility floor. Composite-widget keyboard interface is what the
   APG requires the moment you have a list of interactive items.
2. **It is the power-user path.** §48's convergence is mechanical, not
   metaphorical: "I never touch the mouse" and "arrow keys within regions" are the
   same implementation. `j`/`k` through your passages *is* roving tabindex.
3. It is what makes the shelf work too — the same argument applies to a grid of
   200 covers, which is otherwise 200 tab stops.

**Two constraints to respect while doing it.** First, **a role is a promise**:
`role="listbox"` tells assistive tech that arrow keys, Home/End and type-ahead
all work, and shipping the role without the full contract makes things *worse*
than a plain list, because you have removed the user's fallback expectations.
Second, if you add single-key shortcuts (`j`/`k`, `/`), **WCAG 2.1.4 requires one
of: an off switch, remapping, or focus scoping.** Focus scoping is the elegant
answer and it is free here — the keys are only live when the list has focus,
which also makes them discoverable in context. *(The reason is concrete:
speech-input users dictate text and the software emits letters. The W3C's own
example is a colleague saying "Hey Kim" near a live mic firing three commands.)*

## 3.4 Your most important surface has your smallest text

The note editor: monospace, `0.9rem/1.75`, `min-height: 26rem`, `--bg-raised`.

`0.9rem` is ~14.4px. **This is the smallest body text in the application**, and it
is on the surface `layout-redesign.md` calls "the surface this whole redesign is
for."

The relevant measurement: Rello, Pielot & Marcos (n=104, eye-tracked) found
fixation duration falling continuously up to **22pt**, comprehension significantly
worse at 10–12pt than at 18pt, and recommended **at least 18pt** for body text.
That is a study of *reading web text*, not of writing, so it does not transfer
cleanly — but the direction is not ambiguous, and your notes get read far more
often than they get written (that is the entire premise of `/notes` and the links
rail).

**Position: the editor should be the largest body text in the app, not the
smallest.** I would not go to 18pt in a monospace editor — that is enormous — but
`0.9rem` is a *dense-UI* size applied to a *long-form composition* surface, and
the two have opposite requirements. Somewhere around `1rem`–`1.05rem` with the
leading kept at 1.75 costs you almost nothing (the column is capped anyway) and
puts the writing surface above the chrome instead of below it.

**On monospace itself — the redesign closes this too fast, and the honest answer
is that it is a live disagreement.** iA's case is that a writing font should
*not* optimise for speed: monospace looks provisional, which changes your
willingness to cut; the even rhythm makes typos and doubled words visibly wrong.
The counter-case is that monospace costs 20–30% horizontal space for the same
character count and is measurably slower to read — and **notes get read more often
than written.** Neither side has an experiment.

**But you have a decisive local factor the general argument lacks: the vault is
plain markdown and Obsidian is the other thing editing it.** A monospace surface
makes the markup honest, and "markdown as markdown, never a rich-text editor" is
already settled for good reasons. **Position: monospace stays, on the file-format
argument rather than the aesthetic one.**

**What I would take from the research instead is the colour discipline.** Do not
syntax-highlight the markup — **de-emphasise it.** Dim the `#`, `**`, `[[` to a
low-contrast grey, or hang them outside the measure so the prose keeps an unbroken
left edge. And note the one uncomfortable finding: Sarkar's study of syntax
colouring found the benefit **correlated negatively with experience** — colour
helps novices most. **A near-monochrome editor optimises the expert's aesthetics
at the newcomer's expense**, which is worth knowing even if you accept the trade.
The rule that survives: **colour earns its place where it prevents a specific
misreading** — an unterminated emphasis, a broken `[[wikilink]]` — not where it
decorates categories the reader can already parse.

## 3.5 The accent is doing seven jobs

Count them, from `app.css` and `layout-redesign.md`: the wordmark, the current-nav
underline, the selected-row inset, the progress-rail fill, the primary button
fill, inline `code` literals, the `REFLECTION` kind chip, and the state/progress
fragment in the book header.

From §12, on the hierarchy levers:

> **Hue is loud but semantically overloaded.** A single accent colour spent on
> hierarchy cannot also mean "interactive."
>
> Every lever you spend is one you can no longer spend elsewhere. **A limited
> palette is not asceticism — it is the only way to keep any lever legible as a
> signal.**

Your accent currently means, simultaneously: *this is the app*, *you are here*,
*this is selected*, *this is progress*, *this is the primary action*, *this is a
literal you could type*, *this note is the reflection*, and *this is the reading
state*. Those are not one signal. **When a colour means eight things it means
none**, and the specific cost is that the two that most want to be loud — *this is
selected* and *this is the primary action* — are competing with six neighbours.

**Position: the weakest claim is the state/progress fragment in the book header**,
and I would take the accent off it first. It is metadata on a page where you
already know the book; it does not need the loudest lever in the system, and
`--ink-dim` plus position carries it. That is one job removed for free.

The second-weakest is `code`. But that one has a real justification — it marks
*a thing you could actually type*, which is a genuine semantic — and it appears in
prose where nothing competes. I would leave it.

**The general discipline I would adopt, rather than a list of exceptions:** the
accent is reserved for **state that is true right now and that you can act on** —
selection, focus, current page, progress, primary action. Everything descriptive
uses ink, dim, position and weight. That rule would remove the state fragment and
the reflection chip, keep the rest, and — usefully — it is a rule a test could
almost check.

---

# Part 4 — `/notes`

## 4.1 The split is the wrong way round

`grid-template-columns: minmax(0, 68ch) 22rem` — results on the left at up to
~68ch, focused note on the right at 352px.

**352px minus padding is roughly a 38–42 character measure.** Bringhurst's floor
for multi-column work is 40–50; you are at or below it. The research's own
verdict on the analogous case is Harris's Outlook lesson in reverse: a detail
pane either produces an unreadable measure or wastes the width — and yours
produces the unreadable one.

**Meanwhile the results list is given the prose measure.** But a result row is
title + kind chip + book name pushed right + a two-line snippet. **That is not
prose. It is structured metadata, and §20 says structured, homogeneous, scanned
content wants a list with predictable element positions — not a 68ch prose
column.**

**Position: swap the emphasis.**

```
grid-template-columns: 26rem minmax(0, var(--measure));
```

Results get a fixed, generous-but-bounded 26rem — enough for title, chip, book
and a two-line snippet with a stable left edge to run the eye down. The focused
note gets the prose measure it is actually prose in. And the whole grid caps at
`26rem + 68ch`, which preserves the thing your `max-width` was there to fix (the
two halves ending up at opposite ends of a 1440px window with dead air between).

This also makes the mask honest. Fading the body at 15rem is a scent signal —
"there is more here, `Open` to get it" — and it reads as a deliberate preview
rather than as a truncation artefact once the column is a real reading column.

**What it costs:** the results column stops being flexible, so at 1024px the two
columns are tighter than the current arrangement. That is the right place to
spend the constraint — the preview is the thing you are reading.

## 4.2 What you left out is right, and for a better reason than you gave

**No dangling-links index.** `layout-redesign.md` justifies this as "an orphan
queue with better manners," which is correct and sufficient. The research adds a
second, independent reason: **unlinked mentions and dangling indexes are where
link inflation starts** — "more edges, less signal, and a graph that gets *less*
useful the more diligently you use the feature." Two arguments from different
directions is a stronger position than one.

**No graph view.** Your stated reason is "out of scope in `decisions.md` and it
stays out." **The research gives you a much better one**, and I think it is worth
writing down because it converts a deferral into a decision:

> A force-directed layout of a personal vault is a hairball whose node positions
> are **artifacts of a physics simulation, not of semantics** — so the picture is
> not readable, not stable between sessions, and cannot be returned to. **Its
> actual function is legibility of effort: it makes accumulated work visible,
> which is motivational, not navigational.**

**readingbuddy already does legibility of effort, three times, better.** The
shelf, the card wall, and `/life` all make accumulated work visible — with
stable positions, semantic grouping, and something to actually read. **The graph
view's real job is already done. What remains is its advertised job, which it
does badly.** That is a reason to exclude it permanently rather than to defer it,
and it survives the next person who asks for one.

**A caution worth carrying, from Shipper's account of Roam:** bidirectional
linking sells relief from the anxiety of *"where am I going to put this?"*, and
**that relief holds only while you believe you will retrieve the notes later.**
He found "the need to take notes far outstripped the need to review them." The
implication for you is not to build less linking — it is that **`/notes` is the
page that has to make retrieval real**, because a vault whose retrieval never
happens becomes, in his phrase, "a garbage dump full of crufty links." `/notes`
is not a nice-to-have surface. **It is the thing that keeps the rest of the vault
from being a lie.**

---

# Part 5 — Tokens and contrast, measured

## 5.1 `--measure: 68ch` is not 68 characters — and it means two different things

**`ch` is the advance width of the zero glyph**, which Eric Meyer measured as
typically 20–30% wider than the average character in a proportional face.

So in proportional text, **`--measure: 68ch` renders as roughly 85–90 real
characters** — outside Bringhurst's 45–75 band, and outside WCAG 2.0/2.2 SC
1.4.8's 80-character AAA cap. And `--measure-wide: 82ch` renders as roughly
**100–107 characters.**

**But in the monospace editor, `ch` is exact** — every glyph has the same advance —
so `--measure-wide` there is precisely 82 characters. Marginally over the 80 cap,
and a completely different quantity from what the same token produces in the
passage list two states away.

**One token, two meanings, because one surface is monospace and one is not.** That
is the kind of thing that is invisible until someone measures it.

**Position, in order of what I would actually do:**

1. **Decide what the token is claiming.** If `68ch` was chosen by eye and looks
   right, keep the value and **rename it** so it stops asserting a character
   count — `--measure` implies a typographic measure, and it is not one.
   *(Choosing by eye is legitimate here: §11 concludes measure is "a comfort
   variable with a weak performance signature," and the speed evidence actually
   favours longer lines. You are not violating a finding; you are violating a
   label.)*
2. **If it is claiming ~66 characters, it should be about `52ch`.**
3. **Split the wide measure by surface.** `--measure-editor` in `ch` (where `ch`
   is honest) and `--measure-passages` in `rem` or `ch`-adjusted, because the two
   are not the same quantity and pretending they are will keep producing
   surprises.

**A useful bonus check:** on my arithmetic, the book page's centre column at a
1440px window is ~709px, which is almost exactly `82ch` of your 0.9rem monospace
(~708px). **The cap binds only at ≥1440px and the column is narrower everywhere
below**, dropping to ~59 monospace characters at 1200px. So `--measure-wide` is
doing nothing on most windows — worth knowing before you tune it.

## 5.2 The contrast table, computed

Both algorithms, against your actual tokens. WCAG 2 is the normative standard.
APCA is not — it was pulled from WCAG 3 in 2023 and WCAG 3's contrast algorithm
is formally undetermined — **but its diagnosis is broadly conceded, including by
its critics: the WCAG 2 formula is symmetric, so it cannot distinguish light-on-
dark from dark-on-light, and it systematically overstates contrast at the dark
end.**

APCA's own thresholds, for reading the right column: **Lc 90** preferred body,
**Lc 75** minimum body, **Lc 60** minimum non-body content, **Lc 45** minimum
headline, **Lc 30** absolute floor, **Lc 15** point of invisibility.

| theme | what | pair | WCAG 2 | APCA Lc |
|---|---|---|---:|---:|
| dark | `--ink` on `--bg` | `#e8e4dc` / `#14131a` | 14.56 | **−89.9** |
| dark | `--ink-dim` on `--bg` (`.hint`, author, meta) | `#9b96a5` / `#14131a` | 6.42 | **−46.1** |
| dark | `.band-title` (dim, 0.78rem) | `#9b96a5` / `#14131a` | 6.42 | **−46.1** |
| dark | `--accent-text` on `--bg` | `#c48b3f` / `#14131a` | 6.25 | **−45.2** |
| dark | `--line` hairlines | `#2e2c38` / `#14131a` | 1.35 | **0.0** |
| dark | `--bg-raised` vs `--bg` | `#1d1c25` / `#14131a` | 1.09 | **0.0** |
| light | `--ink` on `--bg` | `#21202a` / `#faf8f4` | 15.17 | **99.0** |
| light | `--ink-dim` on `--bg` | `#6a6577` / `#faf8f4` | 5.29 | **74.0** |
| light | `--accent-text` on `--bg` | `#8f6114` / `#faf8f4` | 5.09 | **72.6** |
| light | `--accent` as focus ring on `--bg` | `#c48b3f` / `#faf8f4` | **2.78** | 51.8 |
| light | `--accent` as focus ring on `--bg-raised` | `#c48b3f` / `#ffffff` | **2.95** | 54.7 |
| light | `--line` hairlines | `#e2ddd2` / `#faf8f4` | 1.28 | **13.3** |
| light | `--bg-raised` vs `--bg` | `#ffffff` / `#faf8f4` | 1.06 | **0.0** |

Four findings, in order of how much they matter.

### (a) The focus ring fails WCAG AA on the light theme

`:focus-visible { outline: 2px solid var(--accent); }` uses raw `--accent`, which
is `#c48b3f` in **both** themes — only `--accent-text` is overridden in light.
Against the light background that is **2.78:1**, and against `--bg-raised` it is
**2.95:1**. **SC 1.4.11 Non-text Contrast requires 3:1.**

This is a straight conformance defect, and it is the exact case §41 warns about:
*the focus indicator is to keyboard users what the mouse cursor is to mouse
users.* It is also ironic in a specific way — `app.css` contains a careful comment
about brass measuring 2.78:1 on light and repairs it for *text*, then leaves the
same value on the ring.

**Position: fix it now, and fix it with `--accent-text`, which is already the
value you computed for exactly this background.** `#8f6114` on light bg is 5.09:1
— comfortably over. Better still, adopt Soueidan's two-tone pattern
(`outline` plus a contrasting `box-shadow` ring), because **`outline` survives
Windows High Contrast Mode and `box-shadow` is forced to `none`** — so the pair
degrades correctly rather than vanishing for the users most likely to need it.

### (b) You repaired the wrong theme

This is the counterintuitive one. `app.css` says: *"On the dark theme brass
already clears it and the two are the same value."* Under WCAG that is true —
6.25:1 comfortably clears 4.5:1.

**Under APCA it is backwards.** Dark `--accent-text` is **Lc 45.2**; light
`--accent-text` is **Lc 72.6**. The theme you repaired ended up with roughly
**60% more perceptual contrast** than the theme you declared fine.

Lc 45 is APCA's *minimum for headline-sized text* (36px/400 or 24px/700). It is
two full tiers below the Lc 75 body floor. And your accent text is not
headline-sized — it is `code` at `0.85em`, the state fragment, the wordmark.
**Small text is where APCA demands more, not less.**

The same applies to the entire dim tier: `--ink-dim` on dark is **Lc 46.1**, and
that token carries `.hint` (0.88rem), `.band-title` (0.78rem), author lines,
progress details, and the "no note" marker on dangling links. **On the light
theme the same token is Lc 74.0.**

**Position: the dark theme needs a brighter dim tier and a brighter accent-text,
and this is the single highest-value legibility change available.** Concretely:
introduce a dark-theme `--accent-text` that is *lighter* than `#c48b3f` rather
than the same value, and lift `--ink-dim` on dark until it clears Lc 60 for the
sizes it actually carries.

**The counter-argument, which is real:** you are building an evening reading tool,
and Lc 75 on `#14131a` means text bright enough that some people will find it
harsh at night. The research says exactly this — *"designing a dark theme to
APCA's Lc 75 body floor means text so bright it may exceed what many designers
consider comfortable, and it eliminates most of the muted-grey secondary-text
palette that dark UIs depend on for hierarchy."* **That tension is genuine and
unresolved in the field.** My position is Lc 60 for the dim tier rather than Lc
75 — non-body content is what the tier actually carries — and to hold the line
that the two themes should not be *this* asymmetric, whatever absolute target you
pick.

**And the practical rule, from Roselli:** use APCA to *rank and choose*, then
verify the result also clears WCAG 2, and document any deliberate deviation.
Automated scanners and any future conformance claim only know WCAG 2.

### (c) `--bg-raised` is perceptually invisible against `--bg`

**Lc 0.0 in both themes** (WCAG 1.09 dark, 1.06 light). That is not a bug — a
raised surface is *meant* to be subtle — but it has a consequence the redesign
does not account for.

`layout-redesign.md` uses `--bg-raised` as a **selection** indicator: *"the
selected row gets `--bg-raised` and a 2px accent inset on its left edge."*
Similarly the passage list's annotation block sits on `--bg-raised` to read as a
distinct region.

**You believe you have two selection cues. You have one.** The 2px accent inset is
doing all the work; the fill contributes essentially nothing perceptually. Same
for the annotation block — it is separated by *position*, not by *region*, because
common region requires the region to be visible.

**Position: this is fine, but know it.** The inset is a good cue (Lc 45 as a
non-text mark is legitimate), and adding surface contrast would cost calm. But
two things follow: **do not add a third state that relies on `--bg-raised` alone
to be distinguishable**, and if the annotation block genuinely needs to read as a
separate region, it needs a border or an indent rather than a fill.

*(This also bears on `--bg-sunk` — see the open questions.)*

### (d) Hairlines are decoration, not structure

`--line` measures **Lc 0.0 on dark** and **Lc 13.3 on light** — both at or below
APCA's Lc 15 point of invisibility.

The redesign leans on hairlines structurally: a rule running to the right margin
under every band heading, a rule under the book header, a hairline separating the
folded right rail at ≤1180px, a rule between `/life` months.

**They are not doing the separating. The gaps are.** Which is consistent with the
research — proximity is the strongest cue available and beats decoration — so
nothing is broken. But **the ≤1180px case is worth checking specifically**: when
the right rail folds under the centre "above a hairline," that hairline is the
only thing announcing a change of region at exactly the moment the layout has
stopped announcing it spatially. If the gap there is not doing the work on its
own, the hairline will not save it.

## 5.3 Dark-mode compensation you do not have

Two cheap, nearly invisible additions, both from §13.

**Halation.** On a dark ground, light strokes bloom — scattered light in the
ocular media plus the visual system's edge response makes light-on-dark
letterforms look **heavier and slightly blurred at the same nominal weight.**
Identical type looks bolder and tighter in dark mode; counters close up.

Your dark theme uses the same weights and the same tracking as light. The two
compensations:

- **Reduce apparent weight via the variable-font `GRAD` axis**, which changes
  stroke weight *without changing advance widths* — so nothing reflows:
  `@media (prefers-color-scheme: dark) { font-variation-settings: "GRAD" -25 }`.
  Your `font-synthesis: none` and system stack complicate this (system fonts vary
  in axis support), so treat it as opportunistic.
- **Add a little tracking** — roughly `+0.5px` for light-on-dark is the craft
  value. This one works everywhere and costs a single declaration.

**A note on your font stack.** `ui-sans-serif, system-ui, -apple-system, 'Segoe
UI', sans-serif` gives you zero network cost and native rendering, at the price of
**three different x-heights and three different metrics across platforms — and
therefore three different real character counts inside a fixed `ch` measure.**
Given §5.1's finding that your measure tokens already do not mean what they say,
this compounds. Not a reason to ship a webfont; a reason to stop treating the
measure as precise.

---

# Part 6 — What the research says you are missing entirely

Six things with no counterpart in the current design. Ordered by what I would do
first.

**1. A tab-stop budget.** Covered in §3.3. This is the highest-leverage item on
the list because it is simultaneously the accessibility floor, the power-user
path, and a fix to a defect you already have. **Count the tab stops on the book
page and on the shelf. If either is in the hundreds, that is the number to fix.**

**2. Reversibility before accelerators.** *Speed comes from habituated,
unconsidered action; unconsidered action produces mistakes; therefore an interface
that wants expert speed must make mistakes cheap — otherwise users rationally slow
down and you have built accelerators nobody dares use.* The redesign has a
`Delete` button in the note editor's save bar. **Position: build undo before you
build any keyboard shortcut that can destroy something.** And *"never use a
warning when you mean undo"* — a confirmation dialog is a mode imposed on a
habituated gesture, which is the maximally error-producing configuration; people
click through them precisely *because* they have done it before. This is
architectural, not cosmetic — it constrains the data layer — which is why it
belongs in the engine conversation rather than the frontend one.

**3. Explicit collapse semantics.** Your breakpoints specify *what* folds. They do
not specify *what kind of thing it becomes*, and that is the actual decision:

| | push (becomes a page you navigate to and back from) | overlay (floats above, dismisses, content keeps its place) |
|---|---|---|
| suits | navigation | utilities and inspectors |

At ≤1180 the right rail "folds under the centre," and at ≤860 the left rail
"stacks above the centre" — so both are currently *neither*; they become
stacked page sections. That is a third option and it is defensible for a document
page. **But the left rail is a mode selector**, and a mode selector that has
scrolled off the top of the page is a navigation surface you reach by scrolling
away from your work. **Position: at ≤860 the left rail wants overlay semantics
(the content never loses its place), not stacking.** Also worth stating the
structural rule you are already following, so it survives: **outer before inner,
context before content — the content pane is never the thing that disappears.**

**4. A notification-level spec.** Covered in §1.4(c). One line per element that
changes: *ignore / change blind / make aware / interrupt / demand attention.* The
point is not that anything is currently wrong. The point is that **without a spec,
every new feature defaults upward.**

**5. Shortcut teaching by rehearsal.** When you get to keyboard work, the evidence
is unusually strong and unusually specific. **ExposeHK: overlaying bindings on
controls the instant a modifier is held produced 94% toolbar and 99% menu hotkey
adoption, against 50% for audio feedback and 35% for tooltips** — and 81% of
*first-block* selections already used hotkeys. The principle: **the novice path
should be the expert path performed slowly with a visible prompt**, not a
different path. And the non-nagging grammar has a forty-year-old reference
implementation in Emacs's `suggest-key-bindings`: **teach after success, briefly
(2s), in the periphery, and let it be switched off.**

*(One caveat that bites you specifically: ExposeHK works by overlaying bindings on
**visible controls**. A chrome-light interface has nowhere to hang the overlay.
Your left rail is the natural surface — it is the one persistent region with
named destinations.)*

**6. The text-spacing stress test.** WCAG 1.4.12 lets users override to line-height
1.5×, paragraph spacing 2×, letter-spacing 0.12em, word-spacing 0.16em, **and
nothing may be lost.** Any fixed-height row, chip or button that exactly fits its
label clips. Your kind chips (`REFLECTION`, `REVIEW`), the `.band-title` rows and
the one-line metadata header are the candidates. **It is a bookmarklet and thirty
seconds**, and it is the cheapest real accessibility check available.

*(Related and worth a single test: at 400% zoom the viewport is 320 CSS px, and
**three panes cannot coexist there.** Your ≤860 breakpoint already collapses to
one column, so you are probably fine — but SC 1.4.10 is about the collapsed state
being genuinely usable, not about the breakpoint existing.)*

---

# Part 7 — Your five open questions, answered

**1. Do put-down readings belong on the wall?**

**Yes, in the year they were put down, styled identically — as the prototype has
it.** The reasoning is not the one in the redesign doc, though.

Hiding them would make abandonment a failure state, which the axiom rejects. But
the stronger argument is structural: **a put-down reading is a reading. It has a
finish date, a duration, passages, possibly a review.** It has everything a
completed reading has except a completion, and the wall is a record of reading,
not a record of completing. Grouping it by the year it ended is a true statement.

**The "arguably a lie" objection is answered by the card, not by the wall.** The
card for that reading says `put down 3 May 2026 · p. 201` plainly. The wall is a
low-resolution surface — 91px, no caption — and asking it to carry a distinction
it has no room to express is asking the wrong surface. **The truth lives one click
away, which is where truth that needs words belongs.**

*(Note this is consistent with §1.4(a): put-down is your non-judgemental drain,
and a drain that leads somewhere shameful is not a drain.)*

**2. Captions on or off by default, and is it a setting?**

**Off, except in "No reading recorded," where they are on. Not a setting.** Full
argument in §2.2. If you reject the per-band split, my second choice is off
everywhere — **not** on everywhere, and **not** a toggle.

**3. Is `Recent` still needed?**

**No. Drop it.** Full argument in §2.4 — it is either redundant with `Year` or it
is the backlog sort. And per §1.4(b), I would go further: `Author` and `Title`
should exclude books with no reading, which is a change the redesign has not
considered and which I think matters more than the `Recent` question does.

**4. Does the centre column need `About` / `Reads` at all?**

**Yes, keep them — but this is the weakest of my five positions.**

The case for dropping: it removes a state from the work surface, and the header's
metadata line plus the right rail's `Reads` readout covers most of it.

The case for keeping, which I find slightly stronger: **§28's Norman argument.**
A tool simpler than its task exports the difference to the user. `About & sources`
is where provenance lives — which source claimed what, which cover came from
where — and provenance is load-bearing for an app whose central claim is *"every
source is a tenancy; this is the freehold."* **Deleting the surface where the
freehold is inspectable would be deleting the evidence for your own positioning.**

But hold this one loosely. The honest version is: **it costs a rail entry and a
centre state, both cheap; it is used rarely; and rarely-used-but-important is
exactly what a mode selector is for.** If it turns out nobody opens it, that is
information, and the rail makes it cheap to remove later.

**5. Does `--bg-sunk` earn its place?**

**No — and §5.2(c) gives you a sharper reason than "nothing ships uses it."**

`--bg-raised` already measures **Lc 0.0** against `--bg` in both themes. A *third*
surface tier one step *below* `--bg` would be at least as invisible. **You would
be adding a token for a distinction the eye cannot make.** Surfaces are how you
spend contrast budget, and on a dark theme that budget is competing directly with
text legibility — which §5.2(b) says you are already short on.

**Position: drop it. If a settings surface later needs to read as sunk, it needs a
border or an inset shadow, not a fill** — because at these luminance distances,
fills are not doing region work in your palette. That is a more useful conclusion
than "wait and see," because it tells you what to reach for instead.

---

# Part 8 — What I would hold loosely

Four places where I have taken a position and the evidence does not really reach.
Flagged so they do not harden by accident.

**The row/column gap ratio (§2.5).** A perceptual question with a cheap
experiment attached. Screenshot three ratios rather than argue.

**Monospace in the editor (§3.4).** A genuine live disagreement with no
experiment on either side. I came down on your side for a local reason (the vault
is markdown, Obsidian is the other editor), not because the general argument
favours it. If the file-format reason ever stops holding, the position should be
revisited rather than inherited.

**The fourth attended region (§3.1).** "About three plus periphery" is
triangulated from Cowan's chunks, Hutchings's window logs and platform
conventions — none of which measured panes. You are at the ceiling on a rule of
thumb, not over a limit.

**Whether the captions split is worth the inconsistency (§2.2).** I think the task
difference justifies it. Someone who values the uniformity of the wall more than I
do would reasonably choose captions-off-everywhere, and I would not be able to
produce evidence against them.

---

# Summary of positions

| # | position | strength |
|---|---|---|
| 1 | Restate the axiom as **"states what happened; never sets a target, never counts against one."** | strong — it removes two patches |
| 2 | Rename *reward registers* → **records**. Vocabulary only. | strong — cheap insurance |
| 3 | **Cap "Reading now" at four**, order by latest mark, cap silently | strong — this is the axiom's live exposure |
| 4 | **`Author`/`Title` exclude books with no reading** | strong — the arrangement switch is not axiom-neutral |
| 5 | 86px stands; **reconcile the 8-vs-12 tiles-across discrepancy** | strong |
| 6 | **Captions off, on in "No reading recorded."** Not a setting | moderate |
| 7 | **Letterbox, never crop.** Fixed box also fixes CLS | strong |
| 8 | **Drop `Recent`** | strong |
| 9 | Commit the gap ratio to ~1.8–2× or drop it | weak — test it |
| 10 | Three columns stay; the note list is the left rail's anti-habituation mechanism | moderate |
| 11 | 52px hero — **no caveat** | strong |
| 12 | **Passage list becomes a composite widget** (~120 tab stops → 1) | strong — the highest-leverage single change |
| 13 | Editor text up to ~1rem; monospace stays on the file-format argument | moderate |
| 14 | **Take the accent off the header state fragment**; adopt an accent rule | moderate |
| 15 | **Swap `/notes`' column emphasis** — `26rem` + `--measure` | strong |
| 16 | Graph view excluded **permanently**, because its real job is already done | strong |
| 17 | **`--measure` does not mean 68 characters** — rename or re-derive | strong — it is measurable |
| 18 | **Focus ring fails AA on light.** Use `--accent-text`, two-tone it | strong — conformance defect |
| 19 | **You repaired the wrong theme.** Dark dim/accent tiers need lifting | strong — and counterintuitive |
| 20 | Drop `--bg-sunk`; fills do no region work at your luminance distances | strong |
| 21 | Put-down readings stay on the wall; the card carries the distinction | strong |
| 22 | Keep `About`/`Reads` | weak — hold loosely |
