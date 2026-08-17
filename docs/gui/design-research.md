---
title: How interfaces get laid out, and why — a research digest
date: 2026-08-14
status: research findings, not doctrine. Nothing here is a decision.
scope: weighted toward layout and spatial hierarchy, with typography, multi-pane
       patterns, collection views, calm design, power-user paths and the
       accessibility floor treated at less depth.
---

# How interfaces get laid out, and why

## What this is

A digest of what the design literature actually establishes about layout, and
how much of it is establishment rather than evidence. It is written to be read
rather than consulted: each idea gets its mechanism, its provenance, an honest
grade on how much is known, and — the part usually missing — what it costs.

Nothing here is a recommendation. Several sections end in a genuine, live
disagreement between credible people, and where that is the case both sides are
stated fairly and neither is resolved.

### The grading used throughout

Every numeric claim in this field arrives with an implied confidence that is
usually higher than its provenance supports. Four labels are used:

| label | means |
|---|---|
| **measured** | a controlled experiment, with a sample size you can check |
| **standardised** | a published spec or normative standard; true by fiat, not by finding |
| **conventional** | long practitioner consensus, no experiment, often good judgement |
| **folklore** | repeated until it sounded like research; provenance is an anecdote or nothing |

The single most useful habit this digest can leave behind is asking which of
those four a number is, before using it.

### If you read only three things

- **[Cockburn, Karlson & Bederson, "A Review of Overview+Detail, Zooming, and Focus+Context Interfaces"](https://faculty.cc.gatech.edu/~stasko/7450/Papers/cockburn-surveys08.pdf)** (ACM Computing Surveys, 2008) — the best survey in the field, and the one that will most change how you think about panes.
- **[Woods, "Visual Momentum"](https://www.sciencedirect.com/science/article/abs/pii/S0020737384800437)** (1984) — the theory of what a *transition between views* costs. Under-cited by an order of magnitude. [Short summary here](https://ferd.ca/notes/paper-visual-momentum.html) if you want the argument before the paper.
- **[Dyson, "Line length revisited: following the research"](https://designregression.com/article/line-length-revisited-following-the-research)** — a researcher reviewing the eye-tracking work that undercuts her own field's consensus, and still declining to abandon it. A model for holding evidence and taste at once.

---

# Part I — The grid tradition, and what survives of it

## 1. The grid is a programme, not a layout

**Karl Gerstner, *Designing Programmes* (1964).** Subtitle: *instead of solutions
for problems, programmes for solutions.*

Gerstner's thesis is one sentence, and it is the intellectual root of every
design system built since. No absolute solution exists, because the possibilities
are unlimited; there are always many solutions, one of which happens to be
optimal under one particular set of conditions. So the designer's deliverable is
not the artefact. It is the rule-set that generates a family of artefacts.

His method borrows Fritz Zwicky's *morphological box*: parameters down one axis,
possible components across the other, design as systematic combination. "The
creative process is to be reduced to an act of selection. Designing means: to
pick out determining elements and combine them." His own mobile grid for
*Capital* magazine used 58 units of width, divisible into 2, 3, 4, 5 or 6
columns with consistent gutters, so one substrate served every page. The stated
goal: **"maximum conformity to a rule with the maximum of freedom."**

Two corollaries practitioners usually miss:

- **Any screen you can point at is one sample from the generator.** Reviewing
  screens instead of reviewing the generator reviews the wrong thing.
- **A programme you cannot enumerate is not a programme.** Gerstner's boxes were
  literally tabulated. A "system" that exists only as a feeling in the designer's
  hands is a style.

**Evidence:** primary text plus sixty years of consensus. No empirical content at
all — this is a philosophy of method, warranted by the fact that systems built
this way survived contact with teams and time. Much "Gerstner said" in
circulation is downstream paraphrase; the book was out of print and expensive for
decades.

**What it costs:** the morphological box is combinatorially honest and
aesthetically blind. It enumerates the option space and never tells you which
cell is good; Gerstner assumed a trained designer doing the selecting. Handed to
a team without that judgement, a programme produces *consistent mediocrity* — and
its consistency makes the mediocrity harder to argue against. There is also a
real expressiveness cost: the programme cannot produce what it was not
parameterised for, and the standing temptation is to widen parameters until the
programme constrains nothing.

## 2. The Swiss modular grid — the construction, and the claim it cannot support

**Josef Müller-Brockmann, *Grid Systems in Graphic Design* (1981; approach first
published 1961).** Taxonomy formalised by **Timothy Samara, *Making and Breaking
the Grid* (2002)**.

The construction detail that gets dropped in every summary: **in a modular grid
the module height is derived from the type.** A module is a whole number of text
lines; the gutter between modules is one line of leading. So text set in any
field lands on the same baselines as text in any other. The grid is *grown from
the typography*, not imposed on it. Müller-Brockmann documents systems from 8 to
32 fields and is explicit that the field count is the real decision — more fields
means more freedom and less order.

Four grid types do four different jobs, and conflating them is the commonest
error in the field:

| type | what it constrains | when it pays |
|---|---|---|
| **Manuscript / block** | one text area; all design is in the margins | continuous prose |
| **Column** | vertical divisions only; nothing constrains vertical position | most editorial and most screens |
| **Modular** | columns *crossed by flowlines* — both axes | genuinely tabular or catalogue content |
| **Hierarchical** | irregular divisions fitted to one specific content | a one-off page |
| **Baseline** | orthogonal to all of the above — a fine horizontal rhythm underneath any of them | print; see §12 for why not screens |

The famous passage is not about layout at all. "The use of the grid as an
ordering system is the expression of a certain mental attitude" — then the wills:
to systematize, to clarify, to penetrate to the essentials, "to cultivate
objectivity instead of subjectivity," to achieve "architectural dominion over
surface and space." Working with the grid is "submitting to laws of universal
validity," framed as democratic service to the reader rather than
self-expression. He also undercuts his own dogma, in the same book: "The grid
system is an aid, not a guarantee… it is an art that requires practice."

**Evidence:** primary text; long consensus. Worth being blunt about the gap:
**there is no experiment showing grid-aligned layouts are read faster,
comprehended better, or preferred.** The claim that grids produce clarity is a
claim about *production discipline and consistency across many artefacts*, not a
perceptual finding. Hold it at exactly that strength and it is still a strong
claim.

**What it costs:** the objectivity is partly a pose — Swiss neutrality is a style
with a strong period signature, and Wolfgang Weingart, trained inside it at Basel,
spent the 1970s demonstrating the discipline had become reflex. Structurally: a
grid dense enough to be flexible (32 fields) constrains nothing, while one coarse
enough to constrain (8 fields) forces content to fit. And the baseline grid — the
mechanism that made modular grids cohere in print — is the part that translates
worst to screens.

## 3. Proportional systems, and the golden-ratio problem

**Van de Graaf's canon (1946); Rosarivo (1947); Tschichold, *The Form of the
Book*; Bringhurst (1992); Tim Brown, "More Meaningful Typography" (2011);
Utopia (2021).**

**Van de Graaf's canon** is worth knowing because it is genuinely elegant: for a
page of *any* ratio, divide into ninths — inner margin 1/9, top 1/9, outer 2/9,
bottom 2/9. The type area then shares the page's proportion, and its height
equals the page's width. On a 2:3 page the margins fall out as 2:3:4:6
(inner:top:outer:bottom). Rosarivo, measuring Renaissance books with compass and
rule, argued the "secret number" was 2:3, not φ.

Tschichold classified as "clear, intentional and definite" — irrational: 1:1.618
(golden section), 1:√2, 1:√3, 1:√5, 1:1.538; rational: 1:2, 2:3, 5:8, 5:9 — and
dismissed everything else as "unclear and accidental." (The 1:1.538 is the item
that gives the game away: it makes the list read as a taste rather than a family
of canonical geometric ratios.)

**The modular scale** applies the same instinct to size. Bringhurst: "a modular
scale, like a musical scale, is a prearranged set of harmonious proportions." Tim
Brown made it a web technique — generate sizes by repeated multiplication (10 →
16.18 → 26.18 at φ). Utopia fluidised it: set a ratio and body size at a minimum
and maximum viewport, let `clamp()` interpolate, derive the space scale from the
type scale.

**What the scales genuinely buy is real but much smaller than advertised:** a
small closed set of values, so decisions are cheap and repeatable and no two gaps
differ by 1px for no reason. Utopia's own authors are honest about it — "this
approach shortcuts certain design decisions… it's not a substitute for good
design." *Any* ratio delivers that benefit. The specific ratio is a taste decision
wearing a mathematical costume.

**Evidence on the aesthetic claim: it does not survive scrutiny.** Markowsky's
"Misconceptions about the Golden Ratio" (1992) shows the Parthenon evidence is
measurement-shopping — the building is not rectangular, so "the dimensions can
effectively be selected to suit the whim of the measurer." Fechner's 1876
rectangle-preference result (~76% of votes to the three rectangles of medium
proportion — already not a clean φ preference) has a badly mixed replication
record; the canonical review is **Christopher Green, "All that glitters: A review
of psychological research on the aesthetics of the golden section" (*Perception*,
1995)**, which concludes the effect is largely methodological.

And the classic typographic scale everyone cites as the ancestor of modular
scales — 6, 7, 8, 9, 10, 11, 12, 14, 16, 18, 21, 24, 30, 36, 48, 60, 72 —
**is not a geometric progression.** [Spencer Mortensen's
analysis](https://spencermortensen.com/articles/typographic-scale/) finds 42pt
missing entirely; six notes in the first interval instead of five (the surplus is
11pt); 30pt a semitone error that propagates to 60; and 72pt rounded down by 1pt.
It was guessed, historically, by people fitting foundry sizes.

**What it costs:** geometric scales grow too fast at the top and too slowly at
the bottom for interface work. At φ, steps above body text jump 16 → 26 → 42 —
right for editorial, useless for a dense panel that needs 14, 15, 16. Most
working UI scales are hand-tuned hybrids with a ratio retro-fitted in the
documentation. There is also a hard conflict with the 8-point grid: **a geometric
type scale and a linear spacing grid cannot both be satisfied** without rounding
one of them.

## 4. The 8-point grid: one real technical fact, and a lot of hygiene

**Material Design (2014) specified an 8dp baseline grid.** [Bryn Jackson's
"8-Point Grid"](https://spec.fm/specifics/8-pt-grid) (2015) and [Elliot Dahl's
intro](https://medium.com/built-to-adapt/intro-to-the-8-point-grid-system-d2573cde8632)
(2016) popularised it; [Nathan Curtis, "Space in Design
Systems"](https://medium.com/eightshapes-llc/space-in-design-systems-188bcbae0d62)
(2016) supplied the vocabulary.

Jackson's stated benefits are consistency, speed — "by removing 7 of every 8
spacing options, you reduce the amount of fiddling available to you" — and
cross-platform fit. He also draws a distinction worth keeping: a **hard grid**
(elements snapped into visible grid cells) versus a **soft grid** (8pt increments
measured *between* elements, no grid drawn), and argues the soft grid is what
survives implementation "because programming languages don't use that kind of
grid structure."

**The one genuinely technical argument** is Dahl's: at 1.5×, 2× and 3× rendering,
an odd base produces fractional device pixels — 5px × 1.5 = 7.5px, a half-pixel
offset — whereas even multiples land on whole pixels at every common density.
That is a verifiable rendering fact and the only part of the case that is not
consensus.

**Curtis's contribution is more useful than the number.** He makes the scale
geometric (2, 4, 8, 16, 32, 64, base 16), names it in t-shirt sizes, and
separates spatial *roles*:

- **inset** — equal padding on all sides
- **squish inset** — vertical halved (buttons, table cells)
- **stretch inset** — vertical increased (text inputs)
- **stack** — vertical space between blocks
- **inline** — horizontal space between wrapping items

That taxonomy explains why one token cannot serve every gap, which the raw
"multiples of 8" rule never does.

**Evidence:** the pixel-density argument is fact. Everything else is the
ergonomics of teamwork — Dahl's own framing is about designer/developer handoff
and cross-team inconsistency, not perception. **There is no study showing
8px-quantised interfaces are better looking or easier to use.** This is
engineering hygiene that acquired the vocabulary of a design principle. Dahl is
admirably honest: "the 'system' is only good if it is easy to follow and repeat."

**What it costs:** quantised space fights optical correctness (§7). Type has no
integer optical bounds, icons carry internal safe area, and a nested radius or
icon-plus-label gap that is optically right is frequently off-grid. It also
fights geometric type scales. And 8 is too coarse for dense text-heavy UI — the
widely-read 4pt counter-argument reports 16 too much and 24 too little for common
gaps, with practitioners reaching for 12 and 20.

> **Live disagreement — 8pt or 4pt?**
> *8pt:* fewer options, faster decisions, fewer arguments; even multiples avoid
> half-pixel offsets; Material standardised it so the ecosystem speaks it.
> *4pt:* 8 is too coarse for dense control-heavy interfaces; the gaps people
> actually need (12, 20) fall between 8pt steps; 4pt still avoids fractional
> pixels at 2× and 3×.
> *The pragmatic majority position now:* 4pt for spacing **inside** components,
> 8pt for **layout** — which is what Material actually does with its 4dp
> type/icon keylines.

## 5. Intrinsic layout: the page has no edges

**Jen Simmons coined "intrinsic web design" (2018).** See also [Rachel Andrew,
"You Do Not Need a CSS Grid Based Grid
System"](https://rachelandrew.co.uk/archives/2017/07/01/you-do-not-need-a-css-grid-based-grid-system/)
(2017) and [*Every Layout*](https://every-layout.dev/) (Heydon Pickering & Andy
Bell, 2019).

The print grid assumes a page of known size. Screens have none, and the
12-column framework era (960.gs, Bootstrap) was an attempt to fake one. Andrew's
diagnosis: those frameworks were "faking a grid by assigning widths to items,"
which forced row wrappers into markup to clear floats. The wrapper was a
workaround, not a feature — and CSS Grid, which defines rows and columns *in CSS
rather than in markup*, removes the reason the abstraction existed. "Grid is a
grid system."

Simmons's "intrinsic" means sizing driven by content rather than fixed
proportion: `min-content`, `max-content`, `minmax()`, `auto`, `fr`, combined so a
layout has "four stages of things squishing or growing at separate moments" with
no media query at all. *Every Layout* pushes this to twelve primitives — Stack,
Box, Center, Cluster, Sidebar, Switcher, Cover, Grid, Frame, Reel, Imposter, Icon
— "context-independent layout components" that harness the browser's own
algorithms. Bell's slogan: **"be the browser's mentor, not its micromanager."**

Container queries complete the shift: a component queries the space *it* has
rather than the viewport, which is what you need when the same card appears in a
sidebar and in a three-up. (Chrome/Edge 105 Aug 2022, Safari 16 Sept 2022,
Firefox 110 Feb 2023.)

The philosophical payload, worth carrying past any specific CSS: **you specify
how things behave relative to each other and to available space, and the
arrangement is an output, not a drawing.**

**What it costs:** fully intrinsic layouts are hard to specify, hard to review
and hard to hand off. Nobody can *draw* `minmax(20ch, 1fr)`; screenshots at three
viewports do not prove correctness, and failure modes appear at sizes nobody
mocked up. Container queries carry a structural constraint — set
`container-type: inline-size` and you can query width but not height, because
querying a dimension you also affect is an infinite loop. Media queries stay
correct for genuinely global facts: viewport, print, `prefers-reduced-motion`,
`prefers-color-scheme`.

Worth noting that Mark Boulton reached the same content-out conclusion years
earlier by hand — *"we should design content out, not canvas in"* — and taught it
as compound and ratio-based grids before CSS could express it.

> **Live disagreement — rigid grid or intrinsic layout?**
> *Grid:* it is what makes many screens by many people look like one product;
> auditable, teachable, reviewable. Müller-Brockmann's argument was never
> aesthetic — it was production discipline across a body of work, which is
> exactly a design system's problem.
> *Intrinsic:* the grid presumes a page, and there isn't one. Imposing a column
> count imposes a constraint the medium does not have.
> Less binary than it sounds: Gerstner's own answer — "maximum conformity to a
> rule with the maximum of freedom" — is closer to the intrinsic position than to
> the framework position.

---

# Part II — Spatial hierarchy: how the eye actually reads a screen

## 6. Grouping, and what wins when cues conflict

This is the operationally useful part of Gestalt and the part textbooks skip.

The classical principles — proximity, similarity, common fate, good continuation,
closure, symmetry — come from **Wertheimer (1923)**, systematised by **Koffka
(1935)**. The mechanism is real and pre-attentive: the visual system must
partition a retinal image into candidate objects before it can recognise
anything, and it does so using regularities that in the natural world correlate
with objecthood. That is why the principles transfer to screens — a UI is an
artificial scene, and the same partitioning machinery runs on it whether or not
you designed for it.

Two consequences designers underrate:

1. **Grouping is not advisory.** Leave equal gaps between eight fields and users
   *will* see one group of eight. No amount of labelling undoes it.
2. **The classical statements are qualitative.** Wertheimer gave no units, no
   thresholds, no functions. Applying the principles to hierarchically parse
   arbitrary images "is yet to be accomplished." The quantitative work came
   seventy years later and covers a narrow slice — Kubovy & van den Berg (2008)
   show that *in regular dot lattices* proximity and similarity combine additively
   in log-odds and proximity falls off as a pure exponential of relative distance.
   Everything outside dot lattices remains demonstration.

**The ordering when cues fight.** Palmer's 1992 demonstration is the origin: draw
six dots in three proximity-pairs, then draw enclosing outlines that cut across
those pairs — the enclosures win, and the perceived grouping flips. **Common
region dominates proximity and similarity.**

The strongest modern confirmation is Montoro, Villalba-García, Luna & Hinojosa
(2017), which removes the obvious objection. Using a repetition-discrimination
task — an indirect measure where subjects never attend to grouping — the authors
first ran a scaling task, adjusting connector thickness until common region and
connectedness felt *subjectively equally strong*, and only then pitted them.
Common region still dominated: it imposed a **540 ms interference cost** on
connectedness-grouped targets (1283 ms vs 743 ms), against a 37 ms baseline
difference between the two single-cue conditions.

Quinlan & Wilton (1998) fill in the lower tiers: colour similarity is a
persuasive grouping cue, shape similarity a weak one, and both can override
proximity under some conditions — so this is a **gradient of relative strengths,
not a strict lattice.** Brooks's handbook chapter states the correct general
rule: "the resulting organization depends on the relative strengths of the two
grouping factors," and adds that speed matters independently of strength, since
"some grouping principles may operate faster than others."

The working heuristic that falls out — **a panel border overrides a gap; a gap
overrides a colour; a colour overrides a shape** — is an extrapolation from
laboratory arrays of dots to interface elements. It is not itself a finding.

**Proximity is the strongest cue you get for free.** NN/g's summary of the
literature is that proximity "can overpower competing visual cues such as
similarity of color or shape." That makes **the spacing scale the primary carrier
of structure, not decoration.** If the gap *inside* a group is not clearly
smaller than the gap *between* groups, no border, background or colour reliably
fixes the reading.

**What common region costs:** its strength is exactly why it is overused. A
border is a cheap, decisive grouping cue, so designers reach for it — and every
border is visual noise, a contribution to clutter, and a commitment to a
rectangle. Whitespace groups just as reliably at lower ink cost, but only if you
have enough of it. The other cost: **because enclosure wins, a card layout makes
cross-card relationships nearly invisible** — fine for a shelf of independent
items, bad for a set of linked notes.

**A contested claim worth knowing about.** Palmer & Rock (1994) argued that
*uniform connectedness* — closed regions of homogeneous properties — is
logically *prior* to grouping, creating the entry-level units grouping then
operates on. This is the theoretical licence for the surface: a filled panel or a
tinted background is not one cue among many but the thing that defines what the
atoms of the screen are. **It has not survived intact.** Han, Humphreys & Chen
found connectedness sped up similarity-grouped stimuli but gave *no* benefit to
proximity-grouped ones, which a strictly prior stage cannot explain; Peterson
(1994) argued the serial ordering is inconsistent with the evidence generally.
Design writing routinely cites Palmer & Rock as settled fact. It is not, and it
has been contested for thirty years.

## 7. Whitespace as material — and the citation that got mangled

**Rubin's vase (1915)** established that figure and ground are a *single*
perceptual decision. You cannot alter one without altering the other. That is the
whole argument for treating space as a material: the shape of the gap is as
designed as the shape of the mark, whether or not you designed it.

**[Mark Boulton, "Whitespace" (A List Apart,
2007)](https://alistapart.com/article/whitespace/)** supplies the operational
split, and it is the vocabulary the whole field now uses:

- **Macro whitespace** — between major elements: sections, columns, page margin.
  Does grouping and hierarchy. Too little produces ambiguity about what belongs to
  what; too much produces content dispersion and scroll cost.
- **Micro whitespace** — between small ones: line spacing, letter and word
  spacing, caption-to-image, list items. Does legibility and texture. Nearly
  invisible individually, and **it is where most of the perceived quality
  difference between competent and excellent typography lives.**
- **Passive whitespace** — what falls out of your margins and leading without
  deliberation.
- **Active whitespace** — added specifically to structure or emphasise.

His worked example is Spiekermann's *Economist* redesign, where the content
volume was identical and only micro whitespace changed. The practical upshot:
**most "add more whitespace" advice is macro advice, most of the quality is
micro, and the two budgets are independent.**

### The "whitespace improves comprehension by 20%" claim

This is worth dwelling on because it is the clearest available case study in how
design folklore forms.

The circulating claim is attributed to "Lin (2004)." That citation resolves to a
study of presentation media and text topology in older adults, which **does not
manipulate margins or whitespace at all.** The number almost certainly comes from
**Chaparro, Baker, Shaikh, Hull & Brady, "Reading online text: a comparison of
four white space layouts" (Usability News, 2004)** — a different paper, by
different authors, that nobody quoting the claim appears to have read.

What that study actually found (2×2: margins present/absent × optimal/sub-optimal
leading; N=19 analysed; comprehension scored out of 8):

| condition | comprehension /8 | reading speed |
|---|---|---|
| margins + optimal leading | 5.17 | 176.73 wpm |
| margins + sub-optimal leading | 5.06 | 182.34 wpm |
| no margins + optimal leading | 4.28 | 185.42 wpm |
| no margins + sub-optimal leading | 4.58 | 200.94 wpm |

The **margins main effect** is ≈5.12 vs ≈4.43 comprehension — **+15.5%**,
F(1,17)=8.34, p=.01 — and ≈179.5 vs ≈193.2 wpm, F(1,17)=3.61, p=.07.

So three things get stripped in transmission. **First, reading was slower with
margins** — it is a speed–accuracy tradeoff, not a free lunch. **Second, leading
had no performance effect at all**; only margins did, which contradicts most of
the advice the claim is used to support. **Third, N=19, one text, 2004 screens,
and an 8-point comprehension instrument where the whole effect is 0.7 points.**

The better-powered relative is **Rello, Pielot & Marcos, "Make It Big!" (CHI
2016, n=104, eye-tracked)** — note the author list; this paper is very frequently
miscited as "Rello & Baeza-Yates," who co-authored *other* Rello readability
papers. It found line spacing of 0.8 hurt comprehension relative to 1.0/1.4/1.8,
but that spacing had **no effect on mean fixation duration** (F(3,89)=0.064,
p=.978), and that 1.0 was rated *higher* than 1.8 for subjective comprehension.

**The defensible claim is narrow: cramped text hurts, and there is a floor below
which you should not go. There is no evidence that generosity beyond adequacy
keeps paying.** The literature is much better on floors than on optima.

**What whitespace costs:** it is the most expensive material on a screen because
it is zero-sum with content, and its costs are paid by people who are not in the
room when you design — users on small viewports, users at 200% zoom, users with
a lot of data. The prestige association cuts against you too: sparse layouts read
as premium, so the aesthetic reward for adding space is immediate and the
usability cost is deferred and invisible.

## 8. Visual weight, the squint test, and how many levels a person can see

Bottom-up salience is genuinely modelled. **Itti, Koch & Niebur (1998)** extract
intensity, colour and orientation at **nine spatial scales** and **four
orientations** (0°, 45°, 90°, 135°), compute centre-surround differences into
**42 feature maps** (six intensity, twelve colour, twenty-four orientation),
collapse them into **three conspicuity maps** and then one saliency map, and
select attention targets by winner-take-all with inhibition of return.

**The squint test — blur the screen until detail is gone and see what still
separates — is a manual read of exactly that map.** That is why a technique with
no citable origin works: it strips high-frequency detail and leaves the
low-frequency contrast structure that drives first fixations.

Itti is explicit about the ceiling: bottom-up salience describes "the first few
hundreds of milliseconds," and a complete account "must include top-down,
volitional biasing influences." **So the squint test tells you what a person with
no goal sees first. It tells you nothing about someone hunting a specific title.**

**Deciding the levels.** The practitioner rule with the best rationale is Steven
Bradley's three tiers — dominant, sub-dominant, subordinate — justified on
capacity grounds: "people can perceive three levels of dominance. They notice
what's most dominant, what's least dominant and then everything else." Bradley's
warning is the operative half: *many focal points means none.*

**The levers trade against each other and do not add usefully.** Size, contrast,
position and isolation are substitutable; push all four on one element and the
rest flatten into undifferentiated background. Wathan & Schoger's inverse in
*Refactoring UI* is the practical version: get hierarchy from two or three text
colours and two weights rather than many sizes, and **de-emphasise rather than
amplify** — on a coloured background, move the secondary text *toward* the
background colour instead of greying it.

**Evidence:** the saliency model is peer-reviewed, heavily cited and
computationally reproducible — the strongest empirical footing anywhere in this
area. But it predicts *free-viewing* fixations, and its predictive power falls
sharply once a task is imposed. The three-levels rule and the squint test are
heuristics with a plausible mechanism and no direct measurement.

**The tension worth naming:** *salience-driven design and calm design pull hard
against each other.* A high-salience focal point is by construction the thing
that grabs attention involuntarily, which is precisely what a restrained,
ambient interface is trying not to do. A screen designed to be scanned in 200ms
and a screen designed to be sat with are different objects, and the squint test
optimises for the former. **A calm surface will fail a squint test that a
dashboard passes, and that failure is sometimes the design working.**

## 9. Scanning patterns: what holds up, and what is folklore

**What holds up: scanning is task-driven first and layout-driven second.**
Yarbus (1967) showed the same painting produces different scanpaths under
different instructions; NN/g reproduces it in interfaces. On one jetBlue page,
"find destinations" produced 38 fixations concentrated on location names; "find
prices" produced 28 on prices; "study for a quiz" produced 228 across nearly
everything. *"The same page will be processed differently by the same user when
her goal changes."*

**The F-pattern is a symptom, not a target.** NN/g's own 2017 correction states
the preconditions exactly: text with little or no formatting for the web, a user
trying to be efficient, and a user not committed enough to read every word. It is
what happens when a page gives the eye nothing to land on, and NN/g calls it
*harmful* — "users may skip important content simply because it appears on the
right side of the page." **Designing for it is a category error.**

The pattern to engineer is **layer-cake** — fixations concentrated on headings
and subheadings, which NN/g calls "by far the most effective way to scan pages"
— produced by short, front-loaded, visually distinct headings. They also document
*spotted* (hunting a specific word), *marking*, *bypassing*, and *commitment*
(near-full reading, under high motivation).

**What does not hold up as stated:**

- **The Gutenberg diagram** — primary optical area top-left, terminal area
  bottom-right, "reading gravity" along the diagonal — has no published
  eyetracking basis, and its own scope condition disqualifies it from interface
  work: it applies to *evenly distributed, homogeneous* information. Any page with
  hierarchy violates the precondition.
- **The Z-pattern** is a design template circulating as a finding. It describes
  what a designer can *induce* with three or four strong elements, not what the
  eye does unprompted.

**Banner blindness is the strongest counter-evidence to any position-based
rule.** Benway & Lane (1998) found visually-separated, salient, **non-advertising**
shortcuts — the correct answers to the task, deliberately made prominent — were
found 58% of the time versus 94% for equivalent items inside the normal menu.
*(Important calibration: that headline figure comes from the six-person pilot,
not the 72-participant main experiment. The main experiment produced the recall
figures: 17 of 71 = 23.9% reported seeing the non-ad banners.)* Users "almost
never look at anything that looks like an advertisement, whether or not it's
actually an ad" — **learned appearance beats position outright.**

**The tension that cuts both ways:** if scanning is mostly goal-driven, then
layout rules matter least for motivated users and most for ambivalent ones. That
means position guidance is weakest exactly where designers most want it, and
strongest for content nobody came for.

## 10. The density argument

Two research traditions reach the same conclusion independently, and together
they are far more useful than the folk version ("dense for power users").

**The proximity compatibility principle** (Wickens & Carswell, *Human Factors*,
1995): display proximity should match *task* proximity. Information that must be
mentally integrated to complete a task should be rendered close together;
information serving separate mental operations should be separated, because
proximity that does not serve integration produces *interference*. **This makes
density a function of the task graph, not of user sophistication** — the right
density for comparing two documents differs from the right density for reading
one, in the same app, for the same person.

**The expertise reversal effect** (Kalyuga, Ayres, Chandler & Sweller, 2003)
supplies the second axis: instructional support that reliably helps novices
reliably *harms* experts, because a learner with an existing schema must reconcile
redundant external guidance against internal knowledge, costing working memory.
Labels, explanatory text, generous separation and step-by-step scaffolding are all
instances. **So "spacious for novices, dense for experts" is not merely a
preference — the same element is a benefit and a cost depending on who is
looking.**

**Tufte's data-ink ratio** is the most influential and most equivocal piece of
this. *The Visual Display of Quantitative Information* (1983) defines data-ink as
"the non-erasable core of a graphic," and the argument is an economy of attention:
every mark costs a decoding operation, so a mark that carries no information is a
tax. Small multiples — "inevitably comparative, deftly multivariate, shrunken,
high-density graphics" — have aged extremely well, because holding scale and axes
constant lets the eye do comparison that would otherwise cost working memory.

The data-ink ratio itself has aged less well, and **the empirical record is an
inverted U, not a monotone:**

- Gillan & Richman (1994): low-data-ink charts produced significantly *worse*
  interpretation accuracy than medium and high — supporting Tufte at the bottom
  end.
- Inbar, Tractinsky & Meyer (2007, N=87): standard bar charts beat Tufte-style
  minimalist versions on all six rated dimensions at p<.001, with preference
  counts of 24-vs-3 and 29-vs-2, and the most extreme minimalist variant chosen
  by literally nobody.
- Bateman et al. (2010, N=20): embellished charts showed no accuracy penalty and
  significantly *better* recall at 2–3 weeks.

Tufte's own best line acknowledges this and is rarely quoted: **"For non-data-ink,
less is more. For data-ink, less is a bore."**

**Evidence, honestly:** the influence is enormous and the evidence is thin in
both directions. Tufte offers no experiments — the book argues from examples,
taste and rhetoric, and the data-ink ratio is not operationalisable (what counts
as non-erasable depends on both the problem and the readership). The tests that
exist are small student samples with narrow stimuli. Bateman's own authors write
that they "are cautious about proposing specific design recommendations." Few's
rejoinder is that memorability is not comprehension and Bateman's plain charts
were badly designed strawmen. Both are right about the other.

**Matt Ström-Awn's reframe is the most useful thing in this area:** density is
*"the value a user gets from the interface divided by the time and space the
interface occupies."* So **whitespace that adds no value is a density loss, not a
gain** — which dissolves most of the argument, because it makes "is this too
dense?" into five separable questions rather than one aesthetic one.

> **Live disagreement — is modern software too sparse?**
> *Too sparse:* mobile-first patterns migrated to desktop produce content
> dispersion — more scrolling, higher interaction cost, users forced to hold
> information across viewports, weaker mental models (NN/g, 2023).
> *Density nostalgia:* that NN/g study is 8 final participants, qualitative, with
> no effect sizes or significance tests. And the Bloomberg Terminal, cited
> constantly as proof density serves experts, is analysed by its best-known
> critic as persisting because of brand identity, sunk training cost and status
> signalling — *"the more painful the UI is, the more satisfied these users
> are."* That is not a design argument.
> **The two sides are mostly arguing about different users.**

> **Live disagreement — can density be a setting?**
> *Yes:* AWS Cloudscape ships comfortable/compact on 4px increments; Material
> defines a density scale; the expertise reversal effect gives it real
> theoretical backing, since one setting cannot be right for both populations.
> *No:* Jared Spool's widely-quoted finding that under 5% of users change any
> setting — with programmers and designers the outliers at 40–80% — means the
> people who build density toggles are precisely the people unlike the users who
> receive them. **(Flag: that figure is an unpublished blog account of an internal
> study, with no methodology or sample frame ever released. The direction is
> almost certainly right; the number should not be quoted as science.)**
> *Nobody has published adoption data for a shipped density toggle*, which is the
> number that would settle it.

---

# Part III — The measure: typography as a layout problem

## 11. Line length, and the speed/preference split

**The rule.** Bringhurst, §2.1.2: "Anything from 45 to 75 characters is widely
regarded as a satisfactory length of line for a single-column page set in a
serifed text face in a text size. The 66-character line (counting both letters
and spaces) is widely regarded as ideal. For multiple column work, a better
average is 40 to 50 characters."

Note the wording. **Bringhurst is reporting a consensus, not a result**, and he
cites no study. The older lineage is the "alphabet-and-a-half" rule (≈39–45 cpl)
and Tinker & Paterson (1929) on print.

**The mechanism usually given** is eye-movement economy. At the end of a line the
eye makes a *return sweep* — a long right-to-left saccade. Return sweeps are
ballistic and systematically undershoot: the longer the line, the larger the
absolute targeting error, so the more often a corrective saccade is needed and
the higher the risk of landing on the wrong line. Short lines cost the opposite
way: you pay the return-sweep tax more often per unit of text, and a very narrow
column fragments phrases across breaks.

**The empirical picture does not support 66 as a speed optimum on screen:**

| study | conditions | finding |
|---|---|---|
| Dyson & Kipping (1998) | 25 / 100 cpl | 100 cpl read **faster**; comprehension unchanged |
| Dyson & Haselgrove (2001) | 25 / 55 / 100 cpl | 55 cpl gave **best comprehension** and beat short lines on speed |
| Bernard, Fernandez & Hull (2002) | 45 / 76 / 132 cpl | no reliable speed differences; 76 cpl rated most desirable |
| Shaikh (2005), online news | 35 / 55 / 75 / 95 cpl | **95 cpl read significantly fastest** (178.82 wpm vs ~167–169); **no effect on comprehension**; 30% named 95 cpl most-preferred and 30% named 35 cpl — and 100% named an extreme as *least* preferred |

So: **speed effects are small, inconsistent in direction, and where they exist
they usually favour longer lines. Preference reliably favours ~55–76.
Comprehension is essentially flat across the whole range.**

The honest summary is that **measure is a comfort variable with a weak
performance signature** — a different and better claim than "66 is optimal,"
because comfort is what sustains a long reading session.

**The mechanism is also under attack.** Slattery & Parker (2019) showed
undersweep fixations last ~130 ms versus ~250 ms for ordinary fixations; Parker &
Slattery (2019) showed readers get parafoveal preview benefit *during* the
undersweep fixation. **The undershoot is not simply wasted time**, which
undercuts the mechanism the 66-character rule was resting on. Parker et al.
(2020) found six-year-olds already use the same strategy.

**Three things that are unambiguous:**

1. **WCAG 2.0/2.2 SC 1.4.8 (AAA) caps width at 80 characters** (40 for CJK), and
   its stated justification is disability-specific — "people with some reading or
   vision disabilities… have trouble keeping their place" — not a general
   reading-speed claim. *(Frequently miscited as a 2.1 criterion. It is 2.0, and
   it has carried through unchanged.)*
2. **Struggling readers are the one clear case.** Schneps et al. (PLOS ONE, 2013)
   tested 27 dyslexic high-school students at ~12.7 cpl versus ~67.2 cpl and found
   **27% faster reading, 11% fewer fixations and >50% fewer regressive saccades**
   on the short measure, with no comprehension loss.
3. **The CSS `ch` unit is not a character.** It is the advance width of the *zero*
   glyph, which Eric Meyer measured as typically 20–30% wider than average
   character width in proportional faces. **A `68ch` token is closer to 85–90 real
   characters** — outside Bringhurst's range and outside WCAG's 80-character cap.
   For an 80-character column, roughly `60ch`.

**And a warning about preference generally.** Wallace et al. (*ACM TOCHI*, 2022,
n=352, 16 fonts) found readers were fastest in their *preferred* font only 20% of
the time — exactly chance — while 73% believed preference would help. The gap
between each reader's fastest and slowest font averaged **35%** (314 WPM vs 232
WPM), with a large effect (d>0.8) for 76% of participants, and **no font was best
for everyone.** So "users prefer shorter lines" is real, but it should not be
laundered into "shorter lines are better for them" — and the paper is the
strongest available argument that a *preference* beats a perfect default.

## 12. Leading, scale, and hierarchy from a limited palette

**Leading is a function of measure, x-height and colour — not a constant.**

Two mechanisms pull the same way. *Return-sweep targeting:* the eye aims for the
next line using vertical displacement as its cue, so as horizontal travel grows
the vertical gap must grow to keep the landing zone unambiguous. This is why a
90-character measure at 1.3 leading feels like it is trying to make you skip a
line, and the same leading is fine at 40 characters. *Typographic colour:*
leading controls the ratio of ink to paper. A face with a large x-height fills
more of its em, so at identical point size and line-height it produces a darker,
tighter block and needs more leading.

Working numbers practitioners converge on — all **folklore**, offered as ranges:

| context | leading |
|---|---|
| desktop long-form, 60–80 char measure | 1.5–1.6 |
| narrow column, 20–25 char | 1.3–1.45 |
| headings | ~1.1–1.2 |

**Headings need *less* leading**, which is the part most often got wrong by
applying a single global `line-height`.

The evidence is weaker than the confidence: **Rello, Pielot & Marcos (CHI 2016)
found line spacing had no significant effect on fixation duration (p=.978) while
font size mattered a great deal** — fixation duration fell continuously up to
22pt, comprehension was significantly lower at 10 and 12pt than at 18pt, and the
authors recommend at least 18pt for body text. The measure×leading interaction
specifically has, as far as can be found, **never been isolated
experimentally** — it is inference from return-sweep mechanics plus consensus.

Note the third variable nobody tracks: **leading trades against the number of
lines visible at once.** In a text-heavy tool, generous leading is bought with
reduced peripheral context.

### Which hierarchy lever is loudest

Six levers build hierarchy: size, weight, colour/value, case, spacing, position.
**They are not equal, and vision science tells you why.** Preattentive features
are detected in parallel across the whole field in under ~200–250 ms; they
include luminance/intensity, hue, size and orientation. **Case is not
preattentive** — reading "NOTES" as more important than "Notes" requires
foveating it. Healey documents an interference asymmetry: **luminance dominates
hue, and hue dominates shape and texture.**

That gives a practical ranking:

1. **Value/luminance contrast** — loudest and cheapest. No layout, no font file,
   no reflow, and seen before the eye moves.
2. **Position and surrounding whitespace** — next, and nearly free.
3. **Weight** — strong and cheap *if the family has the weights*.
4. **Size** — strong but expensive: changes layout, and past two or three steps
   wrecks vertical density.
5. **Hue** — loud but semantically overloaded. A single accent spent on hierarchy
   cannot also mean "interactive".
6. **Case and tracking** — the quietest, and best used for the *lowest* tier. A
   letterspaced uppercase label reads as metadata *precisely because* it is
   subdued and slow.

Craft values for the last one (all folklore, offered as starting points): 0.2–0.25em
tracking for uppercase headings, 0.05–0.1em for acronyms set inline, and
letterspacing should *decrease* as size and weight increase.

**The budget is the point.** Every lever you spend is one you cannot spend
elsewhere. Colour spent on hierarchy conflicts with colour spent on interactivity
or state; size spent on hierarchy conflicts with density; weight spent on
hierarchy conflicts with dark-mode weight compensation (§13). A limited palette
is not asceticism — **it is the only way to keep any lever legible as a signal.**
The specific trap for a calm interface: value contrast is so cheap and loud that
it invites a five-value greyscale, at which point nothing is emphasised because
everything is a slightly different grey, and the low-contrast tiers fail WCAG
anyway.

### Vertical rhythm survived; the baseline grid did not

Rutter's 2006 technique was concrete: pick a basic leading (12px text at 1.5 →
18px unit), make every vertical measurement a multiple of it, recompute
line-height proportionally per size. Done fully, every line across every column
sits on a shared invisible grid, as in print.

**It never worked on the web, for a specific and — until recently — unfixable
reason:** CSS `line-height` distributes leading as *half-leading* above and below
the text box, so glyphs float within the line box rather than sitting on its
baseline, and the offset depends on the font's own metrics. Every font needs
different manual padding; images have arbitrary heights; responsive reflow breaks
alignment.

Zell Liew's negative existence proof is the decisive argument: he examined Medium,
Awwwards and Dribbble — three unambiguously well-designed sites — and none
followed a true baseline grid, yet none looked wrong. His diagnosis is that
**what readers perceive is repetition of spatial relationships, not shared
baselines.**

What the industry does instead: a small closed set of spacing tokens on a common
base. IBM Carbon uses a 2px base with 13 non-linear tokens (2px–160px); Atlassian
uses an 8px base spanning 0–80px; **neither documentation mentions baseline grids
or vertical rhythm at all.** *Every Layout* supplies the mechanism — the
"lobotomised owl", `.stack > * + * { margin-block-start: … }` — on the argument
that "margin is really a property of the relationship between two proximate
elements," so spacing belongs to the parent context, not the child.

**A wrinkle worth knowing:** CSS `text-box-trim`/`text-box-edge` removes
half-leading, which makes true baseline alignment newly feasible — Chrome/Edge
133, Safari 18.2, Firefox 154. *Firefox support landed only within the last few
weeks as of August 2026, so this is newly-baseline rather than settled
infrastructure; anything relying on it needs a fallback.* **The strongest
technical objection to baseline grids expired, and nobody has revisited the
question.**

## 13. Dark mode: polarity, halation, and the contrast maths problem

Two distinct things get conflated here, and separating them is the whole point.

### (1) Polarity

**Dark-on-light ("positive polarity") measurably outperforms light-on-dark for
reading**, and the best-supported mechanism is optical rather than psychological:
a bright display constricts the pupil; a smaller pupil reduces spherical and
chromatic aberration and increases depth of field; retinal image quality
improves.

**Piepenbrock, Mayr, Mund & Buchner (*Ergonomics*, 2013)** tested 84 younger
adults (18–33) and 85 older (60–85) on Landolt-C acuity and a proofreading task.
Positive polarity won for both groups — acuity **d=2.17** in the young, **d=0.58**
in the old; proofreading main effect F(1,165)=9.92, η²=.06, with no polarity×age
interaction. Critically, **there were no differences in eyestrain, headache or
mood** — the fatigue argument for dark mode did not appear in the data. Their
2014 follow-up showed the advantage grows **linearly as character size shrinks.**

This is the most-replicated finding in this entire digest. Google's CHI 2023 study
(n=459) independently found light mode read reliably faster — and, in the now
familiar pattern, participants *did not prefer* the mode they read fastest in.

**The caveats are real and specific.** Legge et al. (1985) found all seven
participants with cloudy ocular media (cataract) read *faster* in dark mode —
light scatter through an opaque lens is worse with a bright field. **This, not
astigmatism, is the evidential base of the widely-repeated "dark mode is better
for astigmatism" claim**, and it is an argument about scatter, based on n=7, in a
cataract population. Dobres et al. (2017) found no polarity effect in simulated
daytime but a light-mode advantage at night for glance reading.

### (2) Irradiation / halation

On a dark ground, light strokes bloom outward — scattered light in the ocular
media and glass, plus the visual system's own edge response, makes light-on-dark
letterforms look **heavier and slightly blurred at the same nominal weight.** Same
reason a crescent moon looks larger than the dark disc it belongs to.

Consequences: identical type looks bolder, tighter and denser in dark mode;
counters close up; hairlines gain apparent mass; and pure white on pure black
maximises the effect.

### So the compensations are

- **Never pure white on pure black.** Material 2 specifies a `#121212` surface
  with 87% / 60% / 38% white for high / medium / disabled emphasis. *(For
  calibration: pure `#FFFFFF` on `#121212` is 18.73:1; white at 87% composites to
  ≈`#E0E0E0`, which is 14.19:1.)*
- **Drop apparent weight**, ideally via the variable-font `GRAD` axis, which
  changes stroke weight *without changing advance widths*, so nothing reflows:
  `@media (prefers-color-scheme: dark) { font-variation-settings: "GRAD" -25 }`.
  Without a GRAD axis, a whole weight step down is the crude version — and it
  costs you the weight lever.
- **Add a little tracking** — roughly +0.5px for light-on-dark is the craft value.
- **Desaturate accents** — Material recommends the 200 tone rather than 500 for
  dark surfaces, because saturated hues on dark grounds chromatically aberrate
  and vibrate.
- **Distrust WCAG 2 contrast ratios on dark backgrounds specifically.** See below.

### The contrast maths problem, quantified

WCAG 2's ratio, `(L1+0.05)/(L2+0.05)`, is **symmetric**: swap foreground and
background and the number is identical. Human contrast perception is not
symmetric. APCA models this with signed Lc values and different exponents per
polarity.

Computing both algorithms against greyscale text:

| | on `#ffffff` | on `#121212` |
|---|---|---|
| WCAG 4.5:1 boundary lands at | `#767676` → **APCA Lc 71.6** | `#7d7d7d` → **APCA Lc −32.8** |
| WCAG ratio needed to reach APCA's Lc 75 body floor | **5.10:1** | **11.67:1** |
| WCAG 1.4.11's 3:1 non-text boundary, in APCA terms | Lc 57.1 | **Lc 20.4** |

**Same nominal ratio, less than half the perceptual contrast.** And a focus ring
that exactly satisfies the 3:1 non-text requirement on a dark background lands
near APCA's Lc 15 "point of invisibility."

**Where this stands as of August 2026, honestly:** the *diagnosis* is broadly
conceded, even by APCA's critics. The *prescription* is contested and
non-normative — **APCA is out of WCAG 3.** It was flagged for removal in early
2023 after failing to gain working-group support, pulled from the July 2023 draft,
and WCAG 3's contrast algorithm is now formally undetermined; Adrian Roselli's
April 2026 estimate puts WCAG 3 at 2030 at the earliest. APCA's Lc thresholds
(Lc 90 preferred body, 75 minimum body, 60 general content, 45 headline, 30
absolute floor, 15 invisibility) are one researcher's judgement calls, not
measured thresholds.

*A caution on the shorthand equivalences* (Lc 75 ≈ 7:1, Lc 60 ≈ 4.5:1, Lc 45 ≈
3:1): Myndex publishes each of those against a **specific mid-light grey
background** — `#ddd`, `#d0d0d0`, `#ccc` respectively. They do not hold for light
backgrounds generally, and specifically not for white, where Lc 75 is 5.10:1.
Applied to white, the shorthand is off by roughly 1.4×.

**The pragmatic resolution most practitioners land on:** use APCA to *rank* and
*choose*, then verify the result also clears WCAG 2, and document any deliberate
deviation — because automated scanners and legal challenges only know WCAG 2.

> **Live disagreement — should a long-form reading tool default to dark?**
> *No:* the polarity evidence is one of the most replicated results in display
> ergonomics, the advantage grows as type shrinks, and the eyestrain benefit
> people cite did not appear in the data.
> *Yes:* those studies measure short, bright-room proofreading, not two-hour
> evening sessions in a dim room; Dobres found the light-mode advantage was a
> *nighttime* effect for glance reading; cataract/scatter patients read faster in
> dark; and a well-compensated dark theme closes most of the measured gap.
> **Where everyone lands — "ship both and honour the OS setting" — is correct and
> under-specifies the real work:** a dark theme done to light-theme contrast
> ratios is unreadable.

---

# Part IV — Multi-pane layout: the archetypes and their prices

## 14. Master–detail, three-pane, and Miller columns

**Master–detail** (Tidwell names it the *Two-Panel Selector*) runs Smalltalk-80's
System Browser (~1980) → NeXTSTEP/NeXTMail (1986–88) → Claris Emailer, Outlook,
Mac Mail. Wikipedia attributes the split to 1980s 80-column terminals: ~20 fields
per record, only 3–5 columns per row, so the list showed recognisable fields and
the detail showed all of them. *(Plausible, and essentially uncited.)*

**The mechanism is selection persistence without a mode change.** A list encodes
*identity* — enough to recognise an item — and the detail pane encodes *content*.
Because selecting never replaces the list, **the user's place in the collection
survives every act of inspection.** There is no back-navigation, no dead end, and
no re-orientation cost on return. This is why master–detail feels frictionless in
ways a drill-down-and-back flow does not: the expensive operation — rebuilding
your mental position in a list of 400 items — simply never happens.

**The three-pane elaboration** adds a *scope* pane (folders, collections, tags) on
the left. Scope → list → detail is a fixed left-to-right gradient from coarse to
fine, which is why it survives across mail, IDEs, DAWs and file managers: **the
reading order matches the refinement order.**

**The axis choice is where it gets contentious**, and there is one unusually good
primary account. Jensen Harris moved Outlook's preview pane from bottom to right
in Outlook 2003 because the vertical layout "showed twice as much of the message
you were reading on the screen plus a few additional emails in the message list."
But he then had to **constrain the reading pane's text to roughly 65 characters**,
because a right-hand detail pane on a wide window produces line lengths no one
can read. Thunderbird ships Classic / Wide / Vertical as a user preference, which
is itself an admission that the answer is content-dependent.

**Miller columns** (Mark S. Miller, 1980, Yale; shipped in the NeXTSTEP File
Viewer, inherited by Finder's Column view) do something unusual: they **convert
hierarchical navigation from a sequence of states into a single visible state.**
The entire path from root to selection stays on screen, and every sibling set
along that path stays clickable.

- They spend *horizontal* space on depth and *vertical* space on breadth — exactly
  right when the tree has high fanout.
- Backtracking is free: click one column left, no stack to unwind.
- **Keyboard traversal is unusually clean** because the four arrow keys map onto
  the four possible moves (sibling up, sibling down, descend, ascend) with no
  modifiers and no ambiguity. This is why terminal file managers adopted it.
- An item's location becomes a *coordinate* (which column, how far down) rather
  than a name to recall — and the coordinate is stable across visits.

Three real costs: **depth eats width** (deep navigation makes each column too
narrow to read); **sort options and metadata display are limited** (a column shows
names, so size, date and tags have nowhere to live — which is why most Finder
users switch to List view the moment they need to sort); and **on shallow
hierarchies the pattern wastes most of the window.** Miller columns are a *deep,
broad, name-addressed tree* pattern. Applying them to anything else is a mismatch.

Remarkably, after 45 years there is **essentially no controlled comparative
research** on columns versus trees. Apple has shipped both in Finder since 2001
and refuses to pick.

## 15. Overview+detail, zooming, focus+context

The HCI trichotomy, from **Cockburn, Karlson & Bederson (2008)** — the best survey
in this whole digest:

- **Overview+detail** separates them *spatially* — two views, two places, both on
  screen.
- **Zooming** separates them *temporally* — one view, you move between scales.
- **Focus+context** integrates them into one continuous display where the focal
  region is embedded in surrounding, degraded context.

**Furnas (1986) made "degraded" precise:** `DOI(x | y) = API(x) − D(x, y)` —
degree of interest equals an item's global *a priori* importance minus its
distance from the current focus. Interest rises with importance and falls with
distance. Everything a fisheye does — collapsed outlines, code folding, a calendar
showing today by hour and this month by date — is that formula with a threshold.

**Shneiderman's mantra** ("overview first, zoom and filter, then details-on-demand")
sits on top with seven tasks: overview, zoom, filter, details-on-demand, relate,
history, extract. *(People consistently forget the last three.)* The mantra is
enormously useful and enormously over-applied: **it describes exploratory visual
analysis of an unfamiliar dataset, not a universal navigation law.** For a
returning user of a familiar personal collection, the overview is a tax, not an
orientation.

**The empirical picture splits by task type, and that split is the finding:**

| study | result |
|---|---|
| Furnas (1986) | navigation accuracy 52% (two flat views) → 64% (one fisheye + one flat) → 75% (two fisheye) on an unfamiliar taxonomy |
| Hornbæk & Frøkjær (2003) | overview+detail gave **highest comprehension**; fisheye gave **faster reading with worse comprehension** |
| Baudisch et al. (2002) | focus+context up to **56% faster** than overview+detail in a *dynamic tracking* task — attributed to divided-attention cost |
| Hornbæk, Bederson & Plaisant (2002) | overviews **increased** task time when semantic zooming was available — **and users still preferred having them** |
| Hornbæk & Hertzum (2007) | fisheye menus worse than plain hierarchical menus |

**Preference and performance dissociate, repeatedly and in both directions.** Users
ask for overviews that measurably slow them down; users perform better with
fisheyes they dislike. **Any decision here validated by asking people what they
want will be wrong roughly half the time.**

## 16. Visual momentum — the actual price of every additional pane

**David Woods, "Visual Momentum" (1984).** From process-control and aviation
displays, not GUI design — and the single most under-cited idea in layout.

Woods defines visual momentum as **"a measure of the user's ability to extract and
integrate information across displays"** — how much of your understanding survives
moving your eyes from one region to another. His framing problem is the
**keyhole**: any display is a small aperture onto a large system, and **the cost
of a system is not the cost of any one view but the cost of the transitions
between views.** Low visual momentum means each transition incurs "mental reset
time," and the user is "assembling a puzzle when there is no picture of the final
product as a reference."

He ranks techniques from low to high momentum:

1. **Total / fixed-format replacement** — worst. Flashing in a new page. *(This is
   what a tab does.)*
2. **Long shot** — a summary view that contextualises the detail.
3. **Perceptual landmarks** — stable cues that persist across views.
4. **Display overlap** — adjacent views sharing content.
5. **Spatial representation** — layout that encodes location and relation.
6. **Spatial cognition** — route-like structure the user can reason about.

**Chandler & Sweller's split-attention effect** supplies the complementary cost
from the other direction: when two information sources must be mentally integrated
and are spatially separated, integration itself consumes working memory that would
otherwise go to the task. Integrated presentations produced higher test scores
*and* less processing time. *(Chandler & Sweller 1991 is in Cognition and
Instruction 8(4); the 1992 paper is in the British Journal of Educational
Psychology — the two are routinely cited to one journal.)*

**Put together: a second pane is not free information.** It is a transition you
have chosen to make cheap by paying for it in permanent screen area, and **it only
pays off if the two panes are actually integrated in the user's head. Two panes
the user never relates to each other are two apps in one window.**

The mirror-image finding keeps this honest: the **redundancy effect** shows that
integrating information which did not need integrating *also* hurts. There is no
rule that says "integrate more." The rule is **integrate exactly what must be
processed together, and separate the rest.**

## 17. How many panes, and why persistent rails stop being seen

**There is no study that says "N panes is the limit."** What exists is convergent
circumstantial evidence, and it converges on about **three attended regions plus
peripheral awareness.**

- **From cognition:** Cowan (2001) puts working-memory capacity at roughly **four**
  chunks, not seven. *(The leap from "4 chunks" to "4 panes" is an analogy, not a
  result — panes are not chunks, and much pane content is recognised rather than
  recalled. Note also that Miller's 1956 "seven" was a deliberately arch survey of
  several unrelated limits, not a capacity constant.)*
- **From behaviour:** Hutchings et al. (AVI 2004) logged real users: **mean 3.5
  visible windows on a single monitor** (median 3), rising only to **6.8** (median
  6) on large multi-monitor setups — a doubling of display area bought under twice
  the visible windows. Meanwhile **78.1% of the time people had eight or more
  windows open**, so the gap between *open* and *visible* is where the interesting
  behaviour lives. Window activations (n=360,084) had a mean duration of 20.9s but
  a **median of 3.77s** — most visits to a region are glances, not dwells.
- **From platforms:** Apple's HIG says split views "typically display two panes
  (primary and secondary), with an optional tertiary pane." Microsoft's WinUI caps
  top navigation at 5 items and left navigation at 5–10. Bloomberg — the densest
  professional tool in wide use — historically enforced a hard **maximum of four
  panels**, only recently relaxed.

### Tiled, tabbed, stacked — three answers to the same shortage

- **Tiled** spends space to eliminate switching cost. Best when panes must be
  compared.
- **Tabbed** spends switching cost to eliminate space cost, and **destroys visual
  momentum** — it is Woods's worst case, total replacement. Widely disliked by
  layout theorists and universally adopted anyway, because its one virtue —
  arbitrary count at constant cost — is the virtue users actually shop for.
- **Stacked** (Andy Matuschak's notes site; Obsidian's Sliding Panes) is a hybrid:
  fixed-width panes accumulate horizontally and are scrolled through, so **the
  history of your path is the layout**, and pane width — hence measure — never
  degrades as you go deeper. Miller columns for non-hierarchical, link-followed
  content. Its cost: an unbounded stack has no natural closing gesture, so it
  accretes.

### Rails: the cost is attentional, not spatial

This is the under-appreciated finding. **Benway & Lane (1998)** deliberately made
*non-advertising* shortcuts salient by separating them — and salience-by-separation
*caused* invisibility (§9). NN/g sharpened it with location-based avoidance that
persists across pages and even across sites: **users learn that a region is not
worth looking at and stop looking, permanently.**

*(One frequently-cited number needs deflating: the "right rail took 0.8% of
attention while occupying 25% of the content area" figure is a single illustrative
gaze plot — one user, one page, **1 of 132 fixations.** The direction is supported
by NN/g's broader banner-blindness work; the specific magnitude is anecdote.)*

**So: a rail that is always there, always the same, and only sometimes relevant
will be learned as wallpaper. The failure mode of a persistent rail is not
clutter. It is that the one time it matters, it is invisible.**

**And the obvious remedy is worse in a different way.** NN/g's hidden-navigation
study (179 participants, 6 sites) found desktop users **at least 39% slower**,
discoverability falling from 48–50% to **27%**, and perceived difficulty rising
**21%** — with the penalty *larger on desktop than mobile*.

**Peripheral information is caught between two failure modes: persistently visible
and habituated into invisibility, or hidden and never found.** The escape is not a
position on that axis but a change of kind — **make the region change when it has
something to say, since habituation is to constancy, not to presence.**

## 18. Progressive collapse: what a desktop app does when the window shrinks

Desktop responsiveness is not web responsiveness. The window is user-controlled
and often *deliberately* small (a side-by-side pane, a tiled quarter-screen); the
pointer stays precise; there is no single canonical width. **So the useful
artefact is not a set of breakpoints but a collapse order: which pane dies first,
and what it turns into.**

GNOME's is the most explicit published doctrine: start from the most constrained
environment and work up, with **1024×600 as the smallest supported desktop size**
and 360×294 as the phone floor. Libadwaita's illustrative thresholds: ~400sp for
navigation sidebars and utility panes, 500–550sp for view switchers and tabs,
**860sp for the first collapse of a triple-pane layout.** WinUI's defaults are
cruder: ≤640px → LeftMinimal, 641–1007px → LeftCompact, ≥1008px → expanded.

Two structural rules fall out, and they are much better supported than the
specific pixel values — they follow from visual momentum, and every major toolkit
independently arrived at them:

**First: outer before inner, context before content.** A triple-pane layout
collapses the outermost split first, leaving list+detail; only at the narrower
threshold does that collapse too. **The content pane is never the thing that
disappears.**

**Second — and this is the real decision — the collapse has two distinct
semantics:**

| semantics | what happens | suits |
|---|---|---|
| **Push** (`AdwNavigationSplitView`) | the sidebar becomes a page you navigate to and back from; the app becomes a drill-down app and gains a back button it did not have | navigation |
| **Overlay** (`AdwOverlaySplitView`) | the sidebar floats above the content and dismisses; **the content never loses its place** | utilities, inspectors |

Getting these backwards produces an app where opening the file list loses your
document, or where a primary navigation surface can only be reached by dismissing
something.

**What collapse costs:** exactly the hidden-navigation numbers above — 39% slower
on desktop, >20% discoverability loss — and they are *worse* on desktop, the
platform where collapse is least necessary. There is also a defaults problem:
**designing collapse on the theory that "the power user will turn it back on" is
designing for a very small minority.**

## 19. Navigation: orienteering, and what "place" actually demands

**Teevan, Alvarado, Ackerman & Karger (CHI 2004)** is the load-bearing empirical
result here. Studying people searching their own email, files and the web, they
found "most of the search behavior we observed did not involve keyword search."
Instead people **orienteered**: they "navigated to their target with small, local
steps using their contextual knowledge as a guide, ***even when they knew exactly
what they were looking for in advance***."

They could have teleported. They chose not to. Two reasons: **stepping lets you
specify less of the need up front**, and **each step supplies context that makes
the result interpretable when you arrive.**

Malone (1983) adds the other half: files serve a **reminding** function that
depends on location. A pure search interface destroys it — nothing is ever
rediscovered by accident.

**This is the empirical foundation under "app as a place." But the phrase needs
discipline, and Harrison & Dourish (CSCW 1996) supply it:**

> **"Space is the opportunity; place is the (understood) reality."**

Space is geometry — panes, coordinates, layout. Place is the accumulated practical
meaning of that geometry. **You cannot draw a place. You can only build a space
stable enough that meaning accretes on it.** Dourish's 2006 retrospective
explicitly attacks the "layer-cake model" that treats space as given and place as
decoration on top.

**The design consequence is unglamorous: stability over time is the whole
mechanism.** A layout that rearranges itself, a list whose order changes, a panel
that appears conditionally — each destroys the substrate on which place is built.
And this conflicts with essentially every other pressure on a product.

Three mechanics worth naming for a local app:

- **Breadcrumbs** show *hierarchical position, not session history*, must not
  replace primary navigation, and are pointless on flat structures.
- **Back-stacks** are session history. They and breadcrumbs answer different
  questions and neither substitutes for the other.
- **Deep links** — custom URL schemes (`obsidian://`, `vscode://`, `things:///`) —
  turn every in-app location into a durable, external, shareable address. This is
  what lets a local app participate in a user's wider system, and it is **the
  strongest possible statement that a location in your app is a real place rather
  than a transient view state.**

**A caution on elaborate spatial metaphors:** Cockburn & McKenzie (CHI 2002, n=69,
collections of 33/66/99 items) found spatial retrieval got *worse* as dimensional
freedom increased from 2D through 2.5D to 3D, in both physical and virtual
conditions. **Spatial memory is real but it is cheap and flat, not rich and deep.**

> **Live disagreement — place-based or search-first?**
> *Place-based:* Teevan's finding that people orienteer even when they could jump;
> Malone's reminding function; place accretes only on a stable space.
> *Search-first:* the command-palette lineage (Emacs `M-x` → Quicksilver → Sublime
> Text 2's Ctrl-Shift-P in 2011 → everything since) is faster, scales to any
> collection size, and costs zero screen area.
> **The resolution most well-regarded apps reach:** these serve different
> populations of the *same* user — orienteering for the familiar and recent,
> search for the remembered-by-name and the old — **and the failure is building
> only one.**

---

# Part V — Collections: the cover grid as a specific problem

## 20. Grid versus list is a question about the task

The theoretical spine is **Jeremy Wolfe's Guided Search 6.0 (2021).** Attention is
deployed roughly **twenty times per second** to whatever is most active on a
"priority map" built from five weighted sources: top-down feature guidance from
your target template, bottom-up salience, scene structure, learned value, and
recent history.

**The practical consequence: search cost depends almost entirely on how well the
target can be described in guidable features.**

| search type | slope |
|---|---|
| pop-out (unique guidable feature — "the orange one") | ≈ 0 ms/item — set size barely matters |
| guided conjunction | ≈ 90 ms/item on target-absent trials |
| purely serial, requiring foveation of each item | **250–350 ms/item** |
| item recognition alone | > 150 ms/item |

So: **a grid is right when the target is guidable by colour, shape or remembered
gestalt, and when the user wants parallel comparison of images. A list is right
when the target is text, when items must be ordered by an attribute, or when the
set is large enough that per-item foveation is ruinous.** Thirty rows at 250
ms/item is 7.5 seconds of worst-case scan; a grid showing twelve covers forces
scrolling, and **every scroll resets the scene guidance the user had built.**

**The card, specifically.** NN/g's finding is that cards fail exactly where lists
succeed — when the user is looking for a *specific known item* — by three
mechanisms: cards **deemphasise ranking** (a grid of equals hides that item 1 was
ranked above item 30); cards are **less scannable** because element position is
unpredictable across cards, so the eye has no fixed column to run down; and cards
**consume more space per item.** *(NN/g asserts the eye-tracking finding without
publishing numbers or a study design — treat it as strong professional judgement
rather than a measured result. The underlying mechanism is well supported by
general visual-search literature.)*

The design rule: **cards are for heterogeneous content being browsed; a plain
vertical list is for homogeneous content being searched.** Using a card because it
looks more designed is the failure mode the pattern is most prone to. Note also
that on a cover grid the cover already has a hard edge, so **the card frame around
it is often redundant decoration that eats the gutter.**

**Every real library contains both tasks.** The same user browses on Sunday and
hunts one specific title on Tuesday. The escape most media apps take is
asymmetric: **a grid as the browsing home, plus a search affordance that returns a
text list.** Search results are not the same object as the shelf, and do not have
to look like it.

## 21. How small a cover can be — and the asymmetry that decides it

**Torralba, Fergus & Freeman, "80 Million Tiny Images" (IEEE TPAMI, 2008)** ran the
load-bearing human-subject experiment. For scene classification, **humans scored
over 80% correct on 32×32 colour images** — a drop of only about 7 percentage
points relative to full resolution, on 1/64th of the pixels. Below 32×32,
performance falls off fast. **Greyscale needs roughly 64×64 to match colour's
32×32**, which tells you colour is doing a large share of the recognition work at
small sizes.

**The crucial move is to notice this is recognition of a category, not
identification of a specific item.** Two different tasks live in a cover grid:

- **Re-finding a book you have read is recognition.** You already hold a template
  of that cover — dominant colour, layout gestalt — and Guided Search's history
  and top-down guidance make it pop out at thumbnail sizes carrying no legible
  text at all.
- **Identifying a book you have never seen from cover art alone is not
  recognition.** The cover is an arbitrary symbol and the only real identifier is
  the title.

**So the tile-size threshold in a personal library is far lower than intuition
suggests — while the same tile in a discovery or import context is nearly useless
without a caption.**

*(The 32×32 figure is measured. The transfer to cover grids is inference — no
study has measured recognition thresholds for book covers. And crowding and
peripheral acuity in a dense grid make the effective threshold larger than the
isolated-image threshold. Treat it as a floor on what is physically possible, not
a recommended size.)*

**The honest statement of the trade:** small tiles maximise items on screen and
therefore parallel comparison, but they push the collection toward pure
recognition and abandon anyone who does not already know their own library —
which includes every new user and every recently imported book. Large tiles serve
strangers and make the shelf feel considered, but reduce the set size where a grid
beats a list. **There is no size that serves both; tile size encodes an assumption
about how well the user knows the collection.**

## 22. Captions and aspect ratio

**The caption test.** NN/g's test for whether a *thumbnail* earns its space is
whether text alone would make the choice hard — their counterexample is a tea
retailer whose thumbnails were photographs of loose leaves, indistinguishable at
thumbnail size. **The symmetric test applies to captions: a caption earns its
space when the image alone cannot identify the item.**

Book covers are an unusual case because **the title is printed on the artwork.**
So a cover at sufficient size is *self-captioning* and a caption below is
redundant; a cover below that size is not, and the caption is the only identifier.
The threshold, not the principle, is where apps differ.

**Aspect ratio is where cover grids actually break.** Movie posters and Steam
capsules are 2:3 by mandate (Steam specifies 600×900, with the title baked into
the artwork and "no other words"); album art is 1:1 by mandate; **book covers are
not standardised at all** — trade paperbacks near 1:1.5, mass-market near 1:1.6,
art books and manga wandering much further, and provider-supplied covers arriving
at whatever someone uploaded.

Four strategies, each with a structural cost:

| strategy | cost |
|---|---|
| **Crop to a fixed box** | loses title text at the edges — bad for books specifically |
| **Letterbox inside a fixed box** | safe, visually ragged, wastes space |
| **Justified rows** (Google Photos) | beautiful for photos; **destroys the column rhythm that makes a shelf feel like a shelf** |
| **Masonry** (Pinterest) | abandons rows entirely, and with them horizontal comparison and predictable keyboard navigation |

Observed practice: Letterboxd runs an uncaptioned 2:3 poster grid and reveals
titles on hover. Apple Books captions every cover. Goodreads' default is a
sortable *table* with a small cover at the left — not a grid at all. calibre also
defaults to a table with the cover grid as an opt-in.

**The apps that escape cleanest are the ones that control their asset pipeline.**
A local-first app importing covers from four providers does not have that option.

*(A relevant unresolved CSS question: whether masonry belongs inside CSS Grid —
WebKit/Apple's position — or as a separate `display: masonry` mode, Chrome's
position. Genuinely unsettled, and it matters precisely for a collection of items
with varying aspect ratios.)*

## 23. The "continue" shelf: resumption aid or abandonment ledger

Netflix's "Continue Watching," Plex's "On Deck," Spotify's "Jump back in,"
Steam's "Recent," Apple Books' "Reading Now." **There is no design-literature
origin — it is a pattern that propagated by imitation.**

**Why it works:** resuming is the highest-probability action, and the pattern
removes navigation from it. The item is already chosen, so the row is pure
resumption.

**Why it fails:** the same mechanism runs in reverse. **The shelf is populated by
*starting* and drained only by *finishing*.** Since people start far more than
they finish, **its steady state is a queue of things abandoned** — permanently
visible on the home screen.

**The observable evidence that this is real is that every major implementation
eventually shipped a manual removal affordance.** Netflix, Plex, Spotify and Steam
all added "remove from this row" *after the fact*, which is a confession that the
automatic population rule was wrong.

Design responses that work in practice: cap the row hard (three to five items, not
scrollable to twenty); order by recency of *interaction* so stale items fall off
the visible end without ceremony; allow one-gesture dismissal that is not framed
as abandoning the item; **never label the row with a count.**

**Evidence: weak as research, strong as convergent practice.** There is no
published study of continue-row abandonment. The Zeigarnik effect, usually cited
as the mechanism, is worth flagging: the 1927 original has a shaky replication
record and reviews since the 1960s have found it unreliable. **It is a plausible
story, not a foundation.**

**And the remaining tension is genuine:** a short capped row is calm but drops the
fourth thing you were reading — exactly the item you most need help resuming. A
dismissal affordance solves the pile at the cost of **introducing a small act of
self-judgement into a screen whose entire job was to be frictionless.**

## 24. Sectioning a growing archive: time as default, taxonomy as opt-in

**Photo apps default to time and it is not laziness.** Time is the one axis that
requires zero user maintenance, can never be empty, never needs a taxonomy
decision, and maps onto autobiographical memory — **people remember *when* far
better than they remember which folder.** It also degrades gracefully: a
chronological archive with a million items still has a meaningful top.

Taxonomy has none of these properties. Genre or shelf grouping requires either
provider metadata you do not control or curation the user must perform, produces
empty and lopsided buckets, and forces an item into one bucket when it belongs in
three.

**Sticky section headers** are the cheap orientation fix for chronological grids.
NN/g's cost framing is content-to-chrome ratio — they cite roughly **13:1 as
healthy and 2:1 as abusive**, *both measured on an iPhone 11 Pro, so the
thresholds do not transfer directly to a desktop window.* For a grid it means the
header should be a single line of small text, not a bar. Partially-persistent
headers should animate in 300–400 ms.

**The psychological half matters more than the mechanics, and this is the section
worth sitting with.** A grid of covers is ambiguous in a way its designer chooses
to resolve:

- **Sorted by date finished, descending, with no counts, it reads as an
  accumulating record of things done.**
- **Sorted by date added with unread items foregrounded, it reads as a backlog** —
  and completionism converts a backlog into an obligation.

Belk's point about collecting is that **the set, once perceived as a set,
generates its own pressure to complete.** An app that displays "unread" as a
*state* rather than an *absence* has, without saying anything, implied a task.

**The costs of time-grouping are real:** it makes any *taxonomic* question
unanswerable by browsing — "what philosophy have I read" has no home. It
privileges recency permanently, so a book finished five years ago is functionally
unreachable except by search. And **the calm reading of an accumulating archive
depends on a sort order that is itself a value judgement**; the moment the user is
allowed to sort by "date added," the same grid becomes a to-do list, and the app
has no way to stop that.

## 25. Layout stability, and density controls

**A cover grid is the single worst case for layout stability**, because it is many
images of uncertain dimension arriving at uncertain times. Cumulative Layout Shift
quantifies the damage: **≤0.1 is "good," >0.25 is "poor,"** evaluated at the 75th
percentile of loads, and *images without declared dimensions are the first listed
cause.*

**The design implication is that the tile's box must be reserved before the image
exists — which is a third, independent argument for a fixed aspect ratio,
arriving from performance rather than aesthetics.**

Two smaller notes:

- **Skeleton screens** beat spinners and blank screens on perceived duration and
  emotional response, but the advantage vanishes around **5.5 seconds**, where
  skeletons and spinners tie. *(n=80, recruited on the street, nine randomised
  trials each; the author is explicit that the sample is too small to conclude
  much.)* The safer reading: **the shape of what is coming matters more than the
  animation**, and a skeleton matching the real grid's geometry is doing layout
  reservation as much as reassurance.
- **Scroll restoration** is the quietest of these and the most damaging when
  absent. In an infinite or virtualized list the back button typically dumps the
  user at the top, discarding screenfuls of already-seen content — and unlike a
  paginated list there is no page number to return to. **The restore has to be
  built deliberately.**

**On density controls:** every mature media library ships one — Finder's icon
slider (1984), Gmail's comfortable/cozy/compact, Apple Photos' pinch levels,
calibre's configurable cover grid, Plex's poster sizes. **The pattern is so
universal it reads as a requirement.** And the uncomfortable half is that
preference settings of this kind are overwhelmingly left at their defaults, so
**the default tile size carries essentially all of the design weight** and the
control mostly serves as a pressure valve for a small minority who will otherwise
complain loudly.

*(Nobody has published usage data for density controls specifically. Treat "most
users never change it" as a well-founded prior, not a fact. Weak counter-evidence
in the other direction: Letterboxd, Spotify and Apple Books ship no tile-size
control at all.)*

**The useful reframing: a density control is not a way to avoid choosing. It is a
way to make one choice survivable.** Which means the interesting design work is
picking the default with full seriousness — deciding whether the shelf is for
*recognition* (small, dense, many items) or *presentation* (large, few, calm) —
and then offering perhaps three steps rather than a continuous slider, **so each
step can be designed rather than merely computed.**

Note that density and captioning are **not independent axes**: a caption that fits
at large tiles truncates at small ones.

---

# Part VI — Calm, restraint, and the serious argument against them

## 26. Calm technology is a claim about the periphery, not about minimalism

**Weiser & Brown, "The Coming Age of Calm Technology" (1995–96).** Cited
constantly as an argument for quiet, restrained interfaces. **That is not what it
argues.**

Its mechanism is *oscillation*: **"Calm technology engages both the center and the
periphery of our attention, and in fact moves back and forth between the two."**
The periphery is defined precisely — "what we are attuned to without attending to
explicitly" — and the canonical example is engine noise while driving: unattended
until it changes, at which point it **recenters instantly** and you act on it.

**So calm is not achieved by removing information. It is achieved by giving
information a home in the periphery from which it can be recentered** on the
user's initiative or on a genuine change of state. Weiser and Brown name two
mechanisms by which "technologies encalm as they empower our periphery": one that
"easily moves from center to periphery and back," and one that "enhance[s] our
peripheral reach by **bringing more details into the periphery.**" The second is
explicitly an argument for *more* information, not less.

The payoff they name is **locatedness**: "When our periphery is functioning well
we are tuned into what is happening around us, and so also to what is going to
happen, and what has just happened… The periphery connects us effortlessly to a
myriad of familiar details." Their closing formula: **"Designs that encalm and
inform meet two human needs not usually met together."** Calm *and* informative —
not calm *instead of* informative.

**Evidence: an essay, not a study.** Argument plus three worked examples (inner
office windows; Internet Multicast as "a window of awareness"; Natalie
Jeremijenko's "Dangling String," an eight-foot piece of plastic spaghetti driven
by a motor wired to Ethernet traffic). No user studies, no measurements. Its
authority is reputational, and its influence vastly exceeds its evidentiary
weight.

**The critique that is almost never cited alongside it:** Yvonne Rogers, "Moving
on from Weiser's Vision of Calm Computing" (UbiComp 2006), argued from two decades
of practice that **the calm framing had proved largely unbuildable and had quietly
stalled the field**, proposing "engaging" rather than calm UbiComp instead.

**The structural problem for software:** the periphery is cheap in physical space
and expensive on a screen. Weiser's examples all exploit ambient physical
channels — a hallway window, a dangling string, sound in a room — costing no
screen real estate and no focal attention. **A desktop application has one
rectangle, and everything in it is either visible (and competing) or hidden (and
therefore not peripheral at all, just absent).** Software's honest equivalents of
the periphery are narrow: title bars, colour-as-state, a single always-present
low-contrast region, and **the ambient information that persistence itself carries
— where you left off.**

**Treating "calm" as licence to delete information rather than to demote it is the
standard misreading, and it produces apps that are serene and useless.**

## 27. Notification level is a designable dimension with a scale

The most practically useful thing the ambient-display literature produced is the
observation that **"how loudly does this speak" is a continuous, specifiable
property.**

Matthews, Dey, Mankoff, Carter & Rattenbury (UIST 2004) proposed an ordinal scale:

> **ignore → change blind → make aware → interrupt → demand attention**

Each step buys more certainty the user noticed, at the cost of more attention
taken. Pousman & Stasko (AVI 2006) folded this into a four-dimension taxonomy:
information capacity, notification level, representational fidelity (abstract vs
literal encoding), and aesthetic emphasis.

**Why this matters more than it sounds: it converts a values statement ("we want
to be calm") into a per-element specification.** Every piece of information can be
assigned a target notification level and audited against it. A change that renders
without a transition, without a sound, and without moving anything else is *change
blind* — the user will see it next time they look, and never before. A count in a
corner is *make aware*. A modal is *demand attention*. **Most interfaces have no
such spec, so every new feature defaults upward, because the person shipping it
wants theirs seen.**

**The discipline required is per-element: for each thing, decide what happens if
the user never notices it.** Where the answer is "nothing bad," push it down.
**Where the answer is "they lose data," it does not get to be ambient no matter
how much the design language wants it to be.**

*(Evidence: the scale is a design vocabulary — useful, uncontroversial, essentially
unfalsifiable. The empirical work behind it is thin and honestly reported as such;
Mankoff et al.'s CHI 2003 heuristic-evaluation paper exists precisely because
standard Nielsen heuristics evaluated ambient displays badly. The field's own
retrospectives concede ambient displays were notoriously hard to evaluate, since
the success condition — "the user absorbed this without noticing they did" —
resists both task-based testing and self-report.)*

A second cost worth naming: **abstract encodings require learning, and learning is
a focal-attention cost paid up front to buy peripheral awareness later.** That
trade only pays for information you will consult thousands of times.

## 28. Rams, Maeda — and Norman's counterpoint

**Dieter Rams' ten principles** (late 1970s) are quoted more than read. Read in
order, **the list is not primarily about appearance.** Numbers 6, 7 and 9 —
honest, long-lasting, environmentally friendly — are an argument about *waste*.
Rams spoke in New York in 1976 calling for "an end to the era of wastefulness"
and said he expected future generations "to shudder at the thoughtlessness in the
way in which we today fill our homes, our cities and our landscape with a chaos of
assorted junk." Principle 10 is the summary of that ethic: **less design because
more design is more stuff.** Hustwit reports Rams came to regret his own role, and
said that if he could do it again he would not have chosen to become a designer.

Which principles are genuinely operative for software:

- **4 (understandable)** and **8 (thorough down to the last detail — "nothing must
  be arbitrary or left to chance")** — directly applicable and the two most often
  ignored.
- **5 (unobtrusive)** — the calm-technology one: products "should be neutral and
  restrained, to leave room for the user's self-expression."
- **6 (honest** — refuses to "make a product appear more innovative, powerful or
  valuable than it really is"**)** — the anti-dark-pattern principle, written
  thirty years early.
- **7 (long-lasting)** — in software this means file formats and data ownership,
  not visual timelessness.

Which are decoration: **3 (aesthetic)** and **10**, the ones quoted on studio
walls. 10 is routinely mistranslated into "use less UI," a claim Rams never made —
he wrote "as little design *as possible*."

**Three costs.** The principles govern *objects*, and objects have no state, no
time dimension and no history — **nothing in the ten addresses what a product
should do on the thousandth use versus the first, which is most of what software
design is.** "Unobtrusive" and "as little design as possible" pull directly
against "makes a product useful" for any tool with genuine feature depth; Rams
never had to resolve that, because a shelving system has no power-user mode. And
most awkwardly: **the aesthetic is now a signal of virtue that can be worn by
products which violate 6 and 7 outright.** A subscription app that is hard to
cancel can be flawlessly Ramsian on screen. **The principles were an ethic; used
as a style, they are camouflage.**

**John Maeda's *Laws of Simplicity* (2006)** is worth reading in one sitting
because the second half undercuts the first. Law 1 (**Reduce**) is elaborated as
**SHE — Shrink, Hide, Embody**: make it smaller; hide what is not needed *now*
behind a considered door; and *embody* the perceived value that reduction removed,
**so the thing does not read as cheap.** That third step is the one everyone drops
and it is the answer to Norman's objection below. Law 2 (**Organize**) is SLIP —
Sort, Label, Integrate, Prioritize — grouping reduces *apparent* count without
reducing *actual* count. Law 3 (**Time**) is the observation that **latency is
experienced as complexity.**

Laws 5, 6 and 9 are the intellectually honest half and are almost never quoted.
**Law 5: simplicity is only legible against contrast — an interface with no dense
regions has no calm regions either. Law 9: some things can never be made simple.**

**"Shrink and Hide" is where the trouble lives.** Hiding *relocates* complexity; it
does not remove it, and where it is relocated determines whether you helped.
**Tesler's law of conservation of complexity** makes the point sharply: every
system has an irreducible complexity, and the only question is who absorbs it —
the engineer once, or every user every time. **Hiding a control behind a menu is a
good trade for a control used monthly and a terrible one for a control used
hourly.** Maeda's laws contain no rule for telling those apart. Frequency data
does.

### Norman's counterpoint

**Donald Norman, "Simplicity Is Highly Overrated" (2007) and *Living with
Complexity* (2010).** The argument has two legs, usually conflated.

**The empirical leg** is about buying behaviour: visiting Korean department stores
he found appliances with visibly more controls than functionally identical Western
products, and was told the complexity "is a symbol: it shows their status."
**"Yes, we want simplicity, but we don't want to give up any of those cool
features."** People select for *apparent capability* at the point of choice and
for *ease* at the point of use, and these are different moments with different
criteria.

*(This leg has real independent support: Thompson, Hamilton & Rust, "Feature
Fatigue" (JMR, 2005), found experimentally that consumers choose feature-rich
products before use and report lower satisfaction after use. That also **bounds**
the claim — the preference reverses with experience, which argues for a restrained
default plus reachable depth rather than for maximal features.)*

**The conceptual leg is the more durable one:**

> "It's not complexity that's the problem, it's bad design. Bad design complicates
> things unnecessarily and confuses us. Good design can tame complexity."
>
> "**Real complexity does not lie in the tools, but in the task.**"

His central example is the silversmith's planishing hammer — deceptively simple as
an object, meaningless except as one member of a large tool set whose size mirrors
the size of the task. **A tool simpler than its task has not removed complexity; it
has exported it to the user, who now improvises.** His constructive position sits
in an essay title: **"Make it Simple? No! Make it Understandable."** — with the
corollary that "when people make things 'simple' by minimizing controls, they make
it much more difficult to work or to understand."

> **Live disagreement — reduce the control surface, or make a complex one
> understandable?**
> *Reduce:* Maeda's Law 1, Case's Principle 7, Rams' Principle 10 all say the
> same thing — every control you show costs the user something, and most do not
> earn it. Feature fatigue is the empirical support.
> *Understand:* Norman — a tool simpler than its task exports the difference to
> the user, who improvises worse than the designer would have. Tesler's law is the
> formal version: complexity is not destroyed by hiding, only relocated.
> **The synthesis most practitioners use — restrained default, discoverable depth,
> nothing removed — is closer to Maeda's own Shrink/Hide/*Embody* than either
> camp's rhetoric admits.** The genuinely unresolved part is *where* the hidden
> depth lives and how the default hints that it exists. There is no principle for
> that, only frequency data and taste.

**Worth noting how narrow the real disagreement is.** Norman is arguing *within*
the same value system as Rams: nothing in his position endorses gratuitous
ornament, engagement mechanics or hidden state. The argument is about whether the
visible control surface should shrink — not about whether the user should be
confused.

## 29. The overjustification effect — the strongest evidence against gamification, and exactly how far it reaches

**Lepper, Greene & Nisbett (1973)** is the cleanest statement. Nursery-school
children who already liked drawing were split three ways: **promised** a "good
player" award, **given the same award unexpectedly**, or given nothing. In free
play two weeks later, **the promised-reward group drew markedly less than the
other two.** Being paid for something you already wanted to do reframes it as
something you do for pay, and the motivation does not come back when the pay
stops.

**Deci, Koestner & Ryan (1999)** aggregated **128 experiments**. Engagement-,
completion- and performance-contingent tangible rewards all significantly
undermined free-choice intrinsic motivation. Effect sizes by contingency:
engagement ≈ −0.40, performance ≈ −0.28, completion ≈ −0.44; **all tangible
rewards ≈ −0.34 overall** — small-to-moderate, consistent in sign, larger for
children than college students. **Verbal rewards and positive feedback ran the
other way, enhancing intrinsic motivation.**

**The critical distinction is between the *informational* and the *controlling*
aspect of a reward.** Feedback that tells you that you did well supports
competence. A reward contingent on doing the thing announces that the thing is a
means to the reward.

Translated to interface mechanics:

- **Streaks** are completion-contingent rewards *with a loss-aversion multiplier* —
  their power comes from the pain of breaking, not the pleasure of continuing,
  which makes them the most controlling form on the list.
- **Badges and points** are engagement- or completion-contingent.
- **Progress bars and counts of outstanding items** are more ambiguous: a bar for a
  task the user has *chosen* and is executing is informational; a bar or count for
  a goal the *system* set is controlling.

**The predicted damage is not that people do less while the mechanic is live** —
usually they do more. It is that **they do less once it stops, and that they come
to construe the activity as owed rather than wanted.**

**Self-determination theory's positive counterpart** is the more useful half for a
designer: the needs are **autonomy** (I chose this), **competence** (I can see I
am getting better) and relatedness. **Design that satisfies competence without
contingency — showing what you did, not what you owe — gets the motivational
benefit without the undermining mechanism.**

**How far this reaches, honestly.** The 1999 meta-analysis is one of the most-cited
papers in psychology, but the studies are overwhelmingly **short-term laboratory
experiments, mostly with children or undergraduates, on tasks with high
pre-existing interest** — Deci et al. deliberately restricted to initially
interesting tasks, which is where the effect must appear if it exists at all.
Cameron & Pierce's competing meta-analyses included low-interest tasks and
concluded the undermining effect is real but **narrow and avoidable.** Much of
this literature predates the replication crisis; effect sizes of d ≈ 0.3 from
small 1970s–80s samples deserve the usual discount.

**And the honest counter-evidence.** Sailer & Homner's meta-analysis of gamified
learning (2020, 40 experiments) found significant positive effects — cognitive
g = 0.49, motivational g = 0.36, behavioural g = 0.25. **But their own
high-rigour subgroup analysis is the finding that matters: restricted to
methodologically strong studies, the cognitive effect survived and the
motivational and behavioural effects became non-significant.**

**Read together, the defensible position is:** gamification can improve *learning*
outcomes; its motivational benefits are fragile and largely novelty; and **its
risk of undermining pre-existing intrinsic motivation is greatest precisely where
that motivation already exists.** For a voluntary, self-directed activity someone
already loves, that is the worst possible risk profile.

**The counterweight to keep in view:** for genuinely dull, low-interest tasks the
evidence supports rewards, so a blanket anti-gamification rule will misfire on
onboarding, data cleanup and other chores.

## 30. Deceptive design's soft end — the patterns well-intentioned apps fall into

Brignull's catalogue lists eighteen types. Most are commercial and irrelevant to a
local, unmonetised tool — hidden costs, fake scarcity, disguised ads. **Three are
not, and these are the ones a well-intentioned app walks into.**

**Nagging** — "the user tries to do something, but they are persistently
interrupted by requests to do something else that may not be in their best
interests." Gray et al. (CHI 2018) make nagging one of five top-level strategies
precisely because **it needs no deception to do harm.** Onboarding tips that
reappear, "you have unimported books," update prompts, backup reminders, "rate
this app" — each individually defensible, **collectively a machine for converting
a tool into a supervisor.**

**Addictive design** — Tristan Harris's mechanisms are the specification:
intermittent variable rewards, bottomless feeds and autoplay that remove natural
stopping points, instant interruption over respectful delivery. **His most
transferable observation is that what an interface removes is often a *stopping
cue*, and stopping cues are what let a session end without willpower.**

**Confirmshaming** — emotionally loading the decline option. **The soft,
self-directed version is the one calm apps commit: framing the user's own state as
a deficit.** "You haven't finished 12 books." "3 notes need review." **This is
confirmshaming with the user's past self as the shamer.** The mechanism is not the
number; it is that **any number displayed against an implied target converts a
place into a ledger.**

Mathur et al.'s crawl of ~11,000 shopping sites found 1,818 dark-pattern instances
on 1,254 sites (11.1%), and — the finding with the most structural bite — **22
third-party services that sell dark patterns as a product.** Manipulation is
largely not designed; it is installed.

**Two tensions that keep this honest:**

**The taxonomy has no theory of the legitimate case.** A backup reminder is
nagging until the day the disk fails. An empty-state prompt is a call to action or
a helpful signpost depending entirely on frequency and dismissibility. **The
operative distinction is not the presence of a prompt but whether declining it is
*durable*** — a request that respects "no" permanently is guidance; one that asks
again next week is nagging by Brignull's own definition.

**And a strict no-numbers rule can itself become a dark pattern of omission.**
Withholding information the user actively wants, in service of the designer's
aesthetic of calm, is paternalism — and paternalism is what "autonomy" in
self-determination theory is against. **The defensible line is that the *system*
must not set the target; the user asking for a count is not the same act as the
system displaying one unbidden.**

*(A note on the surrounding rhetoric: the descriptive work here is strong —
Mathur et al. is real large-scale measurement, Gray et al. a rigorous content
analysis, and the taxonomies have been adopted into FTC and EU DSA regulation. The
**harm claims are much weaker**: how much harm a given pattern does to a given
person is largely asserted, and the broader "screens are harming us" literature
Harris sits in is genuinely contested, with Orben & Przybylski's
specification-curve reanalyses putting the best current estimates of
smartphone-use effects on adolescent wellbeing at very small. **Use the taxonomies
as a checklist; do not use the surrounding rhetoric as fact.**)*

## 31. The aesthetic-usability effect, and why it argues for *more* testing

**Kurosu & Kashimura (1995)** rated 26 ATM interface variants with 252
participants and found apparent usability correlated more strongly with apparent
*beauty* than with inherent usability. Tractinsky's Israeli replication (1997)
reproduced it in a different culture, dispatching the objection that it was a
Japanese aesthetic-deference artefact.

**But the boundary conditions are stated roughly a thousand times less often than
the effect.** Hassenzahl (2004) found judgements of "goodness" tracked *perceived
usability* and moved with experience, while beauty tracked *identification*
(whether the product expresses who you are) and stayed stable — i.e. **beauty is a
self-oriented judgement largely independent of function, not a proxy for it.** Tuch
et al. (2012) manipulated usability and aesthetics orthogonally and found that
**after interaction, usability drove perceived aesthetics rather than the
reverse.** The original studies were correlational and *pre-use*.

**The defensible reading is narrow and still useful:** visual restraint buys
goodwill on first contact and forgiveness for *small* friction, does not buy
forgiveness for *large* friction, and — the part that should worry you —
**actively suppresses bug reports during user testing.** People are less likely to
*report* problems they are actually having with a beautiful interface.

**So a calm-looking product needs more aggressive usability testing, not less.**

---

# Part VII — Serving beginners and experts in one interface

## 32. The perpetual intermediate — the beginner/expert axis is the wrong axis

**Alan Cooper, *About Face* (1995).** The claim is about *population dynamics*,
not about screens. If you plot users by skill, **the distribution is not
bimodal.** Beginners are a *transient* state — people either become competent
quickly or abandon the product, so the beginner population is constantly drained
from both ends. True experts are a small self-selected tail. **What is left, and
what is stable, is a large mass of people competent at the tasks they do weekly
and permanently ignorant of everything else: perpetual intermediates.**

The design consequence is a three-part budget: optimise the main interface for
intermediates; provide a low-cost on-ramp beginners graduate off, which does not
clutter the intermediate view; and give experts accelerators that stay out of the
way.

**Constantine & Lockwood's usage-centred design** (*Software for Use*, 1999)
attacks the same problem from the opposite end and is worth holding alongside.
They deliberately refuse to model *people* at all: they model **user roles** and
**essential (abstract) use cases** — a task stripped to its intent, free of any
assumption about the interface. **The claim is that skill level is the wrong
variable because it is a property of a person, whereas what determines the right
design is the structure of the task.** Two people of wildly different skill doing
the same task want the same affordance.

**Evidence:** practitioner taste, argued persuasively, with no controlled study.
Cooper offers no data for the distribution he asserts. It is nonetheless
*consistent* with measured evidence from elsewhere: **Lane et al. (2005) found
only ~6% of users favoured keyboard shortcuts across commands despite years of
experience** — which is exactly what a large permanent-intermediate population
looks like.

**What it costs:** "design for intermediates" is easy to say and hard to falsify —
it can license any decision. The Constantine/Cooper split is a genuine trade:
abstract role-and-task models generalise better and resist designer projection,
but they are colder and give you no guidance about *motivation*, which is
precisely what personas are good at.

## 33. Progressive disclosure, and the measurable price of hiding

The mechanism is attentional, not aesthetic: every element competes for the same
limited scan, so showing "only a few of the most important options" shortens the
visual search for the 90% case and makes the frequent path the obvious path.

**Nielsen separates two shapes:** *progressive* disclosure is **hierarchical** —
most users never open the second level (a print dialog's "Show Details"); *staged*
disclosure is **linear** — a wizard, where everyone walks through every step. He
frames the benefit differently for each: **progressive disclosure buys
learnability, staged disclosure buys simplicity.**

There is a non-obvious claim worth pulling out. Nielsen argues the default screen
is **a teaching artefact** — what you put on it tells the user what the app is
for. **Removing something from the default is an editorial statement, not just
decluttering.**

Two conditions must hold or the pattern backfires: the split must be genuinely
80/20, because a wrong split taxes everybody with a click for something they
always want; and **the disclosure control must carry strong information scent** —
the user has to be able to *predict* what is behind it, or the hidden features are
functionally absent from the product.

**The cost is real and has been measured.** NN/g's 2016 hidden-navigation study
(179 participants, 6 sites): hiding main navigation roughly **halves
discoverability** — used in 27% of desktop cases versus 48–50% when visible or
partly visible — made desktop users **at least 39% slower**, mobile users 15%
slower, and raised perceived task difficulty **21%**.

**On the famous "no more than two levels" rule:** that is Nielsen's expert
judgement rendered as a rule. He gives no study for it, and it gets repeated with
far more confidence than its provenance supports. **Treat it as a smell test, not
a limit.**

*(Two provenance notes worth knowing. The term is usually credited to **Kristina
Hooper Woolsey of Apple's Human Interface Group, 1985**, published in Norman &
Draper's *User Centered System Design* (1986) — and even there it is hedged as
"the seminal idea," not a coinage. Jack Carroll's IBM training-wheels work is a
**distinct** concept — blocking advanced functions for novices — and should not be
described as the source of the term. Nielsen's own 2006 article credits nobody.)*

**The deepest cost is invisible.** Progressive disclosure trades discoverability
for calm, and **you cannot get both.** Anything at level two is something a
meaningful fraction of users will never find — that is the whole point and the
whole danger. And **because hidden features are cheap to add, the pattern quietly
removes the pressure that would otherwise force a team to cut something.**

The generalisation is worse than that, and it comes from §36: **Findlater &
McGrenere (2010) found personalisation "trades off awareness of unused features
for performance gains on core tasks" — and that *higher* prediction accuracy makes
it worse.** That result applies to **any hiding scheme**, including progressive
disclosure. **An interface that hides well is an interface that quietly shrinks
your model of the app.**

## 34. Habituation, modes, and the monotony argument nobody accepts

**Jef Raskin, *The Humane Interface* (2000).** The argument is a chain and each
link matters:

1. Human attention has a **single locus**; the interface should not occupy it.
2. Skill is **habituation** — repeated gestures migrate out of conscious control,
   which is what frees the locus of attention for the actual work.
3. Therefore **habituation is the entire source of expert speed**, and it is
   destroyed by anything that makes the same gesture mean different things at
   different times.

**His definition of a mode is stricter and more useful than the folk one:** an
interface is modal when the current state is *not* in the user's locus of
attention **and** the same gesture will produce different results depending on
that state. Crucially, **"an interface is not modal as long as the user is fully
aware of its current state."** Hence **quasimodes** — states held open by
continuous physical effort, like Shift — which are modeless by his definition
because *your own muscle is the state indicator*.

**The least-quoted and most contrarian piece is *monotony*:** Raskin argues each
atomic task should have exactly **one** way to invoke it. Multiple paths (button +
menu item + shortcut) look generous but **fragment rehearsal**, so no single path
gets habituated. This is the direct opposite of "give experts an alternative
route," and it makes him the sharpest available critic of the layered-interface
tradition.

**Evidence:** habituation and automaticity are well-established psychology, not
Raskin's invention. His *design* conclusions — especially monotony — are reasoned
assertion, and **monotony in particular has essentially no empirical support and
is widely ignored in practice.** The catastrophic consequences of mode error are
documented outside HCI: the BEA report on Air France 447 turns on autopilot mode
confusion.

**What it costs:** modelessness trades away expressive density. **vi's modality is
precisely what lets single unmodified letters be commands**, which is why modal
editing survives forty years of Raskin-style criticism and keeps being
reimplemented. And quasimodes cost physical effort — they do not scale past a
handful of modifiers before you are chording.

*(A note on the argument's shape: neither camp has ever produced a controlled
comparison. Forty years of principle and anecdote.)*

## 35. Accelerators vs mnemonics, and rehearsal — how shortcuts actually get learned

**Two different things get called "shortcuts" and conflating them wrecks both.**

| | **Mnemonics / access keys** (Alt+letter) | **Accelerators / shortcut keys** (Ctrl+letter, F-keys) |
|---|---|---|
| intended to be memorised? | **No** | **Yes** |
| assignment | a character early in the *label*, for findability | first or most memorable character of the command's *keyword* |
| documented | in the UI, by underlining | in menus, in a cheat sheet |
| scope | current window | app-wide, consistent |
| localised | **yes** | **never** |
| serves | accessibility and navigation | speed |

**The structural choice above that is which shortcut *system* you use:**

- **Modal (vi)** — unmodified letters become a command vocabulary. Enormous
  density and composability; a hard mode boundary and a permanent mode-error tax.
- **Chorded (Emacs)** — no modes, but a combinatorial explosion of simultaneous
  holds; hence prefix keys like `C-x` as a namespacing hack.
- **Single-key** (Gmail, most TUIs) — fastest to habituate, tiny namespace,
  collides with typing.
- **Leader keys** (Vim's `<Leader>`, almost universally rebound to Space) — the
  compromise. A leader converts a *chord* into a *sequence*, trading simultaneity
  for an unbounded namespace and — critically — **making the shortcut space
  discoverable, because a leader press can pop a menu of what follows.** The cost
  is latency and a timeout.

### The rehearsal principle — the deepest idea here

**Gordon Kurtenbach (1993; Kurtenbach & Buxton, CHI '94):** *guidance should be a
physical rehearsal of the way an expert would issue the command.* **The novice
path should not be a different path; it should be the expert path performed slowly
with a visible prompt.**

**And this is unusually well-measured.** Kurtenbach & Buxton's longitudinal traces
of real users showed marks at 0.18s vs menus at 1.097s for one user and 0.40s vs
1.543s for another, with a sharp transition to marks after roughly **650
selections.**

**ExposeHK (Malacria, Bailly, Harrison, Cockburn & Gutwin, CHI 2013) is the
decisive keyboard result.** With hotkeys overlaid on controls the instant a
modifier is held: **94% of correct toolbar selections and 99% of menu selections
were completed with hotkeys**, against **50% with audio feedback and 35% with
tooltips** — and **over 81% of *first block* selections already used hotkeys.**

The paper also names why the usual approaches fail: **revealing a hotkey after a
mouse click makes users "rehearse pointing, not hotkey use."**

**The forty-year-old precedent worth copying exactly** is Emacs's
`suggest-key-bindings`: when a command you ran via `M-x` has a binding, Emacs
mentions it in the echo area *afterwards*, for a default of **2 seconds**, and you
can set the variable to nil. **It teaches after success, briefly, and it is
switchable off. That is the whole grammar of non-nagging.**

**What rehearsal costs:** it violates Raskin's monotony head-on, deliberately
maintaining two paths to the same command. It also **costs screen** — ExposeHK
works by overlaying bindings on visible controls, which presupposes there *are*
visible controls. **A chrome-light interface has nowhere to hang the overlay.**

> **Live disagreement — is the mouse faster than the keyboard?**
> *Tognazzini's claim*, from unpublished Apple research: "Test subjects
> consistently report that keyboarding is faster than mousing. The stopwatch
> consistently proves mousing is faster than keyboarding" — with a cognitive
> explanation: choosing among abstract symbols takes about two seconds, and users
> experience amnesia for that interval, so the keyboard *feels* instantaneous
> while being slower.
> *The correction:* this is routinely misquoted as "the mouse is always faster."
> Tog's comparison concerns **cursor movement and command selection under decision
> load** — arrow/special-function keys versus direct pointing — **not memorised
> hotkeys versus menus.** Where *that* has been measured the keyboard wins
> decisively: Lane et al. (2005) clocked shortcuts at **1.362s**, toolbars at
> **2.169s**, menus at **3.129s**; Malacria et al. measured hotkey selection at
> 2.74s against 4.16s for pointing.
> **The two are reconcilable if you read Tog narrowly. Almost nobody does.**

## 36. Adaptive interfaces mostly failed; adaptable ones mostly worked

**Adaptive menus (the system reorders items for you) fail for a specific reason:
they destroy the spatial constancy that habituation runs on.**

**Findlater & McGrenere (CHI 2004)**, 27 participants, three split-menu variants,
real MS Word frequency data:

| | block-2 mean selection | ranked first |
|---|---|---|
| **Static** | **306.5 ms** (fastest) | 15% |
| **Adaptable** (user-controlled) | 318.8 ms | **55%** |
| **Adaptive** (system-controlled) | 331.6 ms (slowest) | 30% |

Static beat adaptive significantly in every presentation order. **So the fastest
condition was the least liked, and the most preferred condition sat between the
two on speed.** *(The epigram "users prefer the menu they are slowest with" is a
tidier claim than the data supports — adaptive was slowest, adaptable was
preferred.)*

**Their 2010 paper found the deeper cost** (already flagged in §33):
personalisation "trades off awareness of unused features for performance gains on
core tasks," and **higher prediction accuracy makes it worse.** Under a
high-accuracy adaptive menu, participants were fastest on old items and **slowest
on new ones (3.7s vs 2.9s control)**, with recognition of unused features falling
to **20.7% from 27.0%**.

**Adaptable — user-controlled — survives contact with real life.** McGrenere's
six-week field study: **74% of participants spent at least half their
word-processing time in their self-built Personal Interface**; **81% of all 485
functions ever added were added within the first two days**; and people
personalised on only **3.8 days total.** *Customisation is a small one-time act,
not an ongoing burden.*

**Scarr's CommandMaps (CHI 2012) closes the loop:** a flat, spatially stable
layout of *everything* gave experienced users **1.57s vs 2.11s (ribbon) and 2.40s
(menus)**, with errors of **0.6% vs 5% vs 9%** — **and cost novices nothing.**

**But adaptation is not dead — it was applied to the wrong variable.** Findlater's
own screen-size study found high-accuracy adaptive menus **16% faster on small
screens** (785s vs 937s), where visual search actually costs something, while on
large screens they gave nothing and *low*-accuracy adaptation actively hurt (965s
vs 821s, p=.001). And **ephemeral adaptation** — predicted items appear at once,
the rest fade in over 500ms — beat both static menus and colour highlighting
**while moving nothing.**

**The lesson: adapting *salience* is fine. Adapting *position* is not.**

**What adaptable costs:** it moves work to the user, and most users never do it
unprompted. Findlater's own conclusion was that **"easy to use mechanisms are not
sufficient"** — people needed to be *shown* customisation by example before they
valued it. And spatially stable flat layouts trade calm for density: CommandMaps
only works if you are willing to show a lot at once, **which is the direct
opposite of progressive disclosure.**

## 37. The command palette — what it buys, and three real costs

**Lineage:** Emacs `M-x` (1970s–80s) → TextMate's fuzzy "Go to File" (2004) →
**Sublime Text 2 beta, 1 July 2011** ("It uses the same fuzzy matching as Goto
Anything does, meaning most commands are accessible with just a few key presses")
→ VS Code (2015) → the ⌘K convention now everywhere. Independently validated in
research as **GEKA** (Hendy, Booth & McGrenere, GI 2010).

**A palette works because it flips the memory task.** Shortcuts require **recall**
(you must produce the key from nothing); menus require **recognition** but impose
navigation; **a palette lets you recall *approximately* and then recognise
*exactly*** — which is why fuzzy matching is not a nicety but the load-bearing
part. It also collapses N places into one: there is exactly one thing to learn.

The conventions have converged: a single global key; fuzzy/subsequence matching
with frecency-weighted ranking; **scoping prefixes** that turn one input into
several modes without a mode switch (VS Code's `>` commands, `@` symbols, `#`
workspace symbols, `:` line number, bare text for files); grouped labelled
results; Backspace-on-empty goes back; and — the teaching device — **the
keybinding printed on the right of each row.**

**Evidence:** the convention set is industry consensus with **no controlled
comparison behind it** — nobody has run "palette vs. no palette" as an experiment.
But the pieces are measured. GEKA matched toolbar drop-downs and dialogs for speed
by block 3, had statistically equivalent error rates to mouse methods (254 vs 256
errors across 1,440 uses), and was overwhelmingly preferred for multi-parameter
commands. Its formative study also quantifies the problem palettes solve: **even
advanced users actively used shortcuts for only 42% of commands (11 of 26).**

**Three real costs:**

1. **It hides structure.** A palette is a flat namespace. It gives you no model of
   how the app is organised, and **it does not build the spatial memory that Scarr
   et al. measured as the fastest retrieval mechanism there is.**
2. **Naming becomes the entire UX.** **Furnas et al. (1987)** measured that two
   people spontaneously choose the same term for the same thing with probability
   **below 0.20**, and that single-word access yields **80–90% failure rates.** If
   your only route to a feature is its name, you have made the vocabulary problem
   your primary interaction risk — **which is why aliases and keyword lists are not
   optional.**
3. **It cannot be discovered by looking.** A palette rewards people who already
   suspect the feature exists. **A palette-only feature has worse discoverability
   than a hidden menu**, whose cost NN/g measured at roughly half the discovery
   rate.

**A palette is a superb *second* route and a poor *only* route.** Its real risk is
that it becomes an excuse not to design navigation at all — **every command gets a
palette entry and nothing gets a home.**

## 38. Defaults, preferences, and the argument on both sides

**Havoc Pennington, "Free Software and Good User Interfaces" (2002)** is where
"settings are a failure to decide" comes from, though he never phrases it that
way. His actual claims are more specific and more useful. Preferences:

1. **hide each other** — "too many preferences means you can't find any of them";
2. **"really substantively damage QA and testing"**, because bugs live in
   *combinations* of options, so the test matrix multiplies;
3. **make integration and good UI difficult**, by removing the ability to assume
   anything;
4. **keep people from fixing real bugs** — a preference treats the symptom, which
   is why it feels like a resolution when it is an abdication.

**His decision procedure is the transferable part:** find the underlying annoyance;
ask whether it can be solved for everyone without a preference; ask whether the
inefficiency is real or trivial; then **"figure you have some fixed number of
slots for preferences; is the preference in question worth 'spending' one of those
slots?"**

Spolsky's framing: "Every time you provide an option, you're asking the user to
make a decision" — to someone who did not come to answer questions. His test is
whether the option relates to the user's actual *work* (fine) or to
*implementation detail* (not fine).

**The counter-argument is not that preferences are free. It is that the judgement
about which are trivial is not yours to make.** Torvalds's 2005 message is the
sharpest version: *"this 'users are idiots, and are confused by functionality'
mentality of Gnome is a disease… if you think your users are idiots, only idiots
will use it."*

**And on the empirics, the counter-argument is stronger than the rhetoric.**
Spolsky's specific claim that power users don't customise **is contradicted by
measured evidence**: McGrenere's field study found 74% of participants living
mostly in a self-built interface, and Findlater's lab work found adaptable was the
*preferred* option by a wide margin.

**The synthesis that has actually won in practice is an opinionated default plus
an escape hatch deliberately less convenient than the default** — GNOME's own
settlement is exactly this shape (a small polished Settings app, long tail pushed
to Tweaks, dconf-editor and extensions). Feature flags and an "Advanced" pane are
the same move: **they let you decide *for the interface* while not deciding *for
the user*, at the price of admitting the escape hatch is unsupported surface.**

**The trade is between a coherent product and a habitable one.** Every preference
you refuse buys testability, a describable default experience, and the ability to
change things later — and costs you some users who leave. **Every preference you
add is permanent: removing one is a breaking change to somebody's habituated
setup, which by Raskin's argument is the most expensive thing you can break.**

*(A necessary caution on the number everyone cites here. Jared Spool's "<5% of
users change any setting" comes from an unpublished blog account of an internal
sampling of submitted Microsoft Word config files, with no methodology, sample
frame or date ever released. **The direction is almost certainly right. It should
not be quoted as science.**)*

## 39. Reversibility is what actually licenses expert speed

This is the least glamorous idea in the set and **the one that makes the rest
possible.**

Speed comes from habituated, unconsidered action. Unconsidered action produces
mistakes. **Therefore any interface that wants expert speed must make mistakes
cheap — otherwise users rationally slow down, and you have built accelerators
nobody dares use.**

**Confirmation dialogs do not solve this; they are actively counterproductive, and
the reason is the same habituation mechanism that produces expertise.** Aza
Raskin: *"After clicking 'Okay' countless times in response to the question, we'll
probably click 'Okay' this time too, even if we don't mean to."* And escalating
makes it worse: *"the more in-your-face the warning is, the faster we'll want to
get away from it (by clicking 'Okay') and the more mistakes we'll make."*

**A confirmation dialog is a mode imposed on a habituated gesture — precisely the
configuration Jef Raskin defined as maximally error-producing.** His rule is flat:
**"Never use a warning when you mean undo."**

Nielsen's heuristic #3 states the same principle as a right — users "need a
clearly marked 'emergency exit'." Note **"clearly marked": an undo the user does
not know exists provides no confidence and therefore no speed.**

**The design consequence is an ordering: build reversibility *before* you build
accelerators.** An irreversible action behind a one-key shortcut is a trap; the
same action behind a shortcut with a visible, time-bounded undo is a feature.

**What it costs:** undo is expensive to build honestly and the cost is
architectural, not cosmetic — it requires that every mutation be modelled as a
reversible operation, which constrains your data layer forever after. Some actions
genuinely cannot be undone, and **pretending otherwise with a fake undo is worse
than a confirmation.** And for the small class of catastrophic, unrecoverable,
low-frequency actions, a deliberate speed bump is correct; Raskin's rule read as
an absolute would forbid it.

*(Evidence note: nobody appears to have directly measured "users go faster when
undo exists." The causal link from reversibility to speed is inference — though a
well-motivated one.)*

---

# Part VIII — The accessibility floor

Treated more briefly than the layout material, but with the parts that actually
bite a keyboard-heavy desktop app in a webview.

## 40. WCAG 2.2 is the stable floor; WCAG 3.0 is not a plan you can build against

**WCAG 2.2** reached Recommendation 5 October 2023, republished 12 December 2024.
It added nine success criteria to 2.1 and removed one (4.1.1 Parsing, now obsolete
because browsers recover from malformed markup uniformly).

**For a local, offline desktop app the load is uneven. Three bite hard:**

- **2.4.11 Focus Not Obscured (AA)** — the one that actually fails real apps. Any
  sticky header, pinned toolbar, resizable pane divider or non-modal notification
  strip can cover a focused element as you tab past it. *(`scroll-padding-top` on
  the scroll container is the one-line fix almost nobody applies.)*
- **2.5.7 Dragging Movements (AA)** — bites any drag-to-reorder, drag-to-resize
  splitter, or drag-and-drop. Each needs a **click-only** equivalent, not merely a
  keyboard one; the criterion is explicitly about single-pointer users (head
  pointer, eye gaze, trackball).
- **2.5.8 Target Size Minimum (AA), 24×24 CSS px** — bites dense icon rows and
  inline affordances. *(The Understanding document notably does not explain why 24
  rather than the 44×44 of AAA's 2.5.5. It is a negotiated compromise.)*

Three barely apply to a local app: 3.3.8 Accessible Authentication (no login),
3.3.7 Redundant Entry (no multi-step forms), 3.2.6 Consistent Help (it only
demands consistent *placement* of help if help exists).

**2.4.13 Focus Appearance was published at AAA rather than AA**, which the spec
does not explain — practitioners widely read this as the working group conceding
it was too hard to test reliably, and the Understanding document all but says so:
*"If you need to use complex mathematics to work out if a focus indicator is large
enough, it is probably a sign that you should use a larger indicator instead."*

**WCAG 3.0 will not ship for years** — Roselli's April 2026 estimate is 2030 at
the earliest. Treat it as a signal about direction, never as a target.

**The honest limit of conformance-driven design:** it optimises for what is
testable, and what is testable is not what is important. **WCAG has no criterion
for "this interface is calm," "this interface is learnable," or "this interface
does not induce anxiety"** — the entire COGA body of knowledge sits outside the
normative standard for exactly this reason. Chasing AA can also produce actively
worse design: the classic case is 1.4.3's 4.5:1 threshold pushing designers toward
pure-black-on-pure-white, **a known halation trigger for astigmatic and dyslexic
readers.**

## 41. Focus is application state made visible — not decoration on it

**Sara Soueidan's framing:** the focus indicator is to keyboard users what the
mouse cursor is to mouse users. **Not an accent, not a hover state — the pointer
itself. Deleting it is deleting the cursor.**

Four things a real indicator must do:

- **Be selective.** `:focus-visible` shows the ring for keyboard focus and
  suppresses it for pointer clicks, while always showing it for text inputs. **This
  dissolves the historical excuse that motivated `outline: none`.**
- **Survive forced colors.** `outline` is preserved in Windows High Contrast Mode;
  **`box-shadow` is forced to `none`** and `border-color`/`background-color` are
  overridden. **A shadow-only ring vanishes for exactly the users most likely to
  need it.** Soueidan's universal indicator pairs both —
  `outline: 3px solid black; box-shadow: 0 0 0 6px white;` — so it reads against
  any background and degrades to the outline alone.
- **Have enough area.** 2.4.13 wants an area at least equal to a 2 CSS px perimeter
  of the component, with 3:1 contrast between focused and unfocused states — *and*
  1.4.11 separately wants 3:1 against adjacent colours.
- **Stay visible** (2.4.11 above).

**For SPAs, focus must also *move* on route change**, or it silently resets to
`<body>` and the next Tab starts from the top of the document.

**And this is unusually well-evidenced.** Gatsby ran moderated user testing with
disabled participants across five prototypes. **Focusing the new heading tested
best for screen reader users** — "it would save time and make it clear what
happened" — while resetting focus to the top of the app was "very overwhelming."
**But magnification users' needs conflicted:** prototypes "pretty much fell apart
when zoomed way in." The shipped recommendation is a hybrid — a skip-link
component that receives focus (small enough not to be cut off at high zoom, and a
real tab stop) plus a decoupled live-region announcement.

**That conflict between user groups is the honest finding, and it is why no single
technique is "the" answer.**

**What it costs:** a focus ring bold enough to be unmissable at 400% zoom on a
dark background is loud enough to feel like an error state at 100%.

## 42. One tab stop per composite widget

**The ARIA APG's rule is a single sentence with enormous consequences: Tab and
Shift+Tab move *between* components; arrow keys move *within* them.** A composite
widget — listbox, tree, tablist, grid, toolbar, radiogroup — contributes **exactly
one stop** to the document tab sequence, no matter how many items it contains.

**This is the difference between a shelf of 200 covers being one Tab press or two
hundred. Tab-stop count is the real ergonomic metric of a keyboard interface, and
almost nobody measures it.**

Two implementations:

- **Roving tabindex** — the active item carries `tabindex="0"`, siblings carry
  `-1`; arrow keys move the 0 and call `.focus()`. Real DOM focus moves, so the
  browser scrolls the item into view for free and `:focus-visible` works normally.
- **`aria-activedescendant`** — DOM focus stays on the container. Simpler in some
  frameworks, but you must implement scroll-into-view yourself and cannot style
  the active child with `:focus`.

**When focus leaves and returns, the widget should restore the *last* active item,
not reset to the first — that is the property that makes a multi-pane app feel
like a place rather than a form.**

**The warning that matters:** roving tabindex is a promise you must keep
completely. **A role is a promise.** `role="listbox"` tells assistive tech that
arrow keys, Home/End and type-ahead all work. **Ship the role without the full
keyboard contract and you have made things *worse* than a plain list**, because
you removed the user's fallback expectations. Heydon Pickering's repeated advice
is to check whether the pattern is needed at all — a nested list of links needs no
roving tabindex, and the APG's own tree-view page says a tree is the wrong choice
when the content is "only a navigation list of links."

*(Chrome 150 shipped a declarative `focusgroup` attribute that does roving
tabindex without JavaScript. Roselli's July 2026 testing found real problems —
`focusgroup="none"` children still take Tab focus, `role="presentation"` does not
suppress the exposed role, and the navigation axis does not adapt when the visual
layout contradicts the widget-type default. **Not production-ready.**)*

## 43. Headings are the navigation; landmarks are oversold

**The single most useful empirical number in this field.** WebAIM Screen Reader
User Survey #10 (Dec 2023–Jan 2024, n=1,539) asked how users find information on a
long page:

| method | share |
|---|---|
| **navigate headings** | **71.6%** |
| Find feature | 13.6% |
| read through | 6.4% |
| navigate links | 4.8% |
| **navigate landmarks/regions** | **3.7%** |

Heading *levels* are rated useful by **88.8%**. Meanwhile **36.7% seldom or never
use landmark navigation.**

**Headings are a nineteen-to-one favourite over landmarks. A correct h1→h2→h3
outline is worth more than every ARIA landmark you will ever write.**

Landmarks still matter — they scope the page and give the heading tree somewhere
to live, and SPA route-change focus targets hang off them — but the APG caps them
at roughly **seven per page**, requires unique labels when a role repeats, and
forbids putting the role name in the label ("Site Navigation Navigation").

**There is no browser-level automatic heading outline.** The document outline
algorithm never shipped; `<hgroup>` and ARIA attempts to revive it failed; and the
new `headingoffset`/`headingreset` attributes are not it (JAWS and TalkBack have
bugs above level 6). **Heading level is an authoring decision. Always.**

**For names, Roselli's priority ladder, best to worst:** native HTML (`<label>`,
inner text, `value`) → `aria-labelledby` pointing at visible text → visually-hidden
text → `aria-label`. **`aria-label` is last** because it is not machine-translated,
is invisible to voice-control users (who say what they see), silently overrides
other labels, and cannot be found by anyone.

**The tension worth naming:** optimising for heading navigation pushes toward more
headings and more explicit structure, **which fights the visual minimalism a calm
interface wants.** Every heading you hide visually but keep semantically is a
divergence between what sighted and non-sighted users perceive, **and those
divergences accumulate into two different applications.**

## 44. No ARIA is better than bad ARIA — and there is now hard data

**"A role is a promise."** `role="button"` on a `<div>` "is a promise that the
author has also incorporated JavaScript that provides the keyboard interactions
expected for a button." **ARIA changes what assistive technology is told; it
changes nothing about behaviour.** It is, in the APG's phrase, *"a CSS for
assistive technologies"* — and it can cloak as easily as enhance.

**The 2026 WebAIM Million makes it empirical.** Across one million home pages:

- Pages **with** ARIA averaged **59.1** detected errors; pages **without** averaged
  **42**.
- ARIA attribute density rose **27% in one year** to over 133 per page — six times
  the 2019 level — while average errors rose 10.1% to 56.1 and the share of pages
  with detectable failures rose to **95.9%**, reversing years of improvement.

*(Correlational, not causal — complex pages have both more ARIA and more problems.
But the direction has held across every edition, and it is the opposite of what
ARIA advocacy would predict.)*

**Live regions are the exhibit.** They look declarative and are not. Roselli's
January 2026 support matrix: **JAWS and Narrator treat every live region as
polite regardless of `aria-live="assertive"`; TalkBack treats them all as
assertive**; `role="alert"` rarely prepends "alert"; Orca does not announce alerts
at all. `aria-atomic`, `aria-relevant` and `aria-busy` lack reliable cross-platform
support. **The region must exist in the DOM *before* content is injected or the
update is simply lost** — a race condition, not an error. And announcements are
transient: once made, gone, unreplayable.

**The maxim is often over-read into "never use ARIA."** Some things have no HTML
equivalent — `aria-expanded`, `aria-current`, `aria-live`, accessible names for
composite widgets. **The real rule is narrower and harder: use ARIA only where you
have tested the specific role/attribute in the specific screen reader/browser
combinations you care about, and prefer a plainer pattern over an untested rich
one.** That is expensive, and it is why so much shipped ARIA is decorative.

## 45. The user has already configured their machine

`prefers-reduced-motion`, `prefers-contrast`, `forced-colors` **are not styling
hooks. They are the operating system relaying an accommodation the user has
already chosen, once, globally.**

**Motion.** Val Head's three factors determine whether an animation triggers
vestibular symptoms: **relative size** ("a small button with a 3D rotation
probably won't cause trouble, but a full-screen wipe transition covering the
entire screen likely would"), **mismatched direction or speed** (parallax; motion
opposing scroll), and **distance covered.** **Opacity, colour and blur changes are
near-risk-free.** This is why `prefers-reduced-motion` means **reduce, not
remove**: swapping a slide for a cross-fade preserves the continuity cue while
removing the trigger. The blunt `animation-duration: 0.01ms !important` global
override is a safety net, not a design.

*(Evidence note: the prevalence figures usually quoted — ~8 million US adults with
chronic balance problems, ~2.4 million with chronic dizziness — are population
estimates for balance disorder generally, **not measured web-animation harm.** The
three-factor model is Head's synthesis from practitioner reports, not a controlled
study. It is the best guidance available and weaker than its ubiquitous citation
implies. **The right to be free of triggering motion is not contested; the precise
thresholds are entirely unmeasured.**)*

**Forced colors is stricter than most developers realise.** It overrides `color`,
`background-color`, `border-color`, `outline-color`, SVG `fill`/`stroke`; forces
`box-shadow` and `text-shadow` to `none`; forces gradient `background-image` to
`none`. **Any UI that encodes meaning in a shadow, a gradient or a background tint
loses that meaning.** The repair is the system colour keywords (`Canvas`,
`CanvasText`, `ButtonFace`, `Field`, `Highlight`, `GrayText`, `AccentColor`, …)
inside `@media (forced-colors: active)`, used to restore borders where shadows
were.

**Anything encoded only in colour was already broken; forced colors is just where
you find out.**

**What this all costs:** respecting all four preference axes means maintaining four
or five visual variants of every component — dark, light, forced-colors,
high-contrast, reduced-motion — **and the combinatorics are where design systems go
to die.**

## 46. Layout that assumes its own dimensions is the failure mode

Three criteria, one underlying demand: **the container must be sized by its
content, never the reverse.**

- **1.4.10 Reflow (AA)** — no two-dimensional scrolling at **320 CSS px** width or
  256 px height. *(320px is 400% zoom on a 1280px viewport.)* Exceptions exist for
  genuinely 2D content — data tables, maps, code with meaningful indentation — but
  apply **only to that content**; surrounding headings and controls must still
  reflow.
- **1.4.4 Resize Text (AA)** — 200% text enlargement without loss.
- **1.4.12 Text Spacing (AA)** — the underrated one. Users can override to
  line-height **1.5×**, paragraph spacing **2×**, letter-spacing **0.12em**,
  word-spacing **0.16em**, and nothing may be lost. **Any fixed-height row, badge
  or button that exactly fits its label at your line-height clips under this.** It
  is a stress test you can run in a bookmarklet in thirty seconds.

**Multi-pane layouts fail this in a specific way.** A three-pane app at 400% zoom
has a 320px viewport: **three panes cannot coexist.** The layout must collapse to
one pane, which means **panes must be a responsive arrangement of independently
navigable regions, not a fixed skeleton.** Fixed-height panes with internal
`overflow: hidden` are the classic failure — text grows, the container does not,
content is clipped rather than scrolled.

**On px vs rem, a correction worth having.** Josh Comeau's clarification: **browser
zoom scales px perfectly well.** What px ignores is the browser's *default font
size* setting — the user who once set 16px→20px. His split: **rem for font sizes,
media queries and vertical rhythm; px is fine for borders, shadows and horizontal
padding**, where scaling actively hurts (padding that grows with font size eats
the measure at exactly the moment the user needed more of it).

*(Nobody has published how many users change the browser default font size versus
using zoom. That number would settle the argument, and its absence means **the rem
orthodoxy is asserted with more confidence than its evidence supports**, even
though the underlying mechanism is real.)*

## 47. Cognitive accessibility: largest population, weakest evidence, loudest claims

**W3C COGA's "Making Content Usable"** is explicitly non-normative: *"Following
this guidance is not required for conformance to WCAG."* That is not a weakness —
**it is an admission that cognitive accessibility resists the pass/fail testability
WCAG requires.**

Its eight objectives, of which three are load-bearing for a reading-and-notes tool:
**do not rely on memory; help users focus; avoid mistakes / enable undo.**
Concretely: consistent placement across screens, no session timeouts or silent
data loss, chunked content, **labels visible at point of use rather than
remembered**, and undo everywhere.

**The typography claims are where honesty is required.** The British Dyslexia
Association recommends sans-serif faces, 12–14pt (16–19px), 1.5 line spacing,
60–70 character lines, left-aligned unjustified text, and **dark text on an
off-white or pastel — explicitly not pure white — background.** Most of that is
sound general typography. *(Note that last point sits in direct tension with WCAG
1.4.3, which rewards maximum luminance contrast.)*

**But the specific claim that dyslexia-designed fonts help does not survive
testing.** Kuster et al. (2018), two experiments — 170 children with dyslexia at
text level; 102 with plus 45 without at word level — concluded: *"the Dyslexie font
neither benefits nor impedes the reading process of children with and without
dyslexia."* **This is the clearest case in the field of an intervention whose
adoption vastly exceeds its evidence.**

*(Scope note: the sample is Dutch primary-school children aged roughly 7–12. The
null is well established for that population and has not been demonstrated for
adults.)*

**What actually helps is what the BDA's *other* recommendations describe — larger
type, generous line spacing, a controlled measure, reduced luminance contrast.
Not the letterforms.** The defensible position: **do not ship a dyslexia font as
an accessibility claim; do ship adjustable size, line-height and measure, which is
what the evidence supports** — and note that subjective preference is a legitimate
reason to offer a font *option*, just not to call it an accommodation.

**The unresolved tension:** cognitive accessibility pulls toward more explanation,
more labels, more confirmation — **and every one of those adds visual density,
which is itself a cognitive load and directly opposes calm.** COGA's answer is
personalisation (objective 8) — let the user choose — **but every
user-configurable option is another decision the user must make, which is the same
problem one level up.**

## 48. Where accessibility and power-user design are the same work — and where they diverge

**The convergence is mechanical, not metaphorical.** Everything a power user wants
from a keyboard interface is something a screen reader or switch user *requires*:

| power user calls it | the standard calls it |
|---|---|
| "I never touch the mouse" | 2.1.1 Keyboard |
| "I can see where I am" | 2.4.7 Focus Visible, 2.4.11 Focus Not Obscured |
| "it's fast" | APG composite widget keyboard interface (§42) |
| "I memorise the layout" | COGA objective 1; SC 3.2.3 / 3.2.6 consistency |
| "go to pane 2" | screen reader heading navigation (§43) |

**One implementation serves both. This is the strongest practical argument for
accessibility work in a keyboard-heavy app: it is not a tax on the power-user
path, it *is* the power-user path.**

*(A caution on how this is usually argued: the "curb-cut effect" framing is an
analogy to physical infrastructure, widely repeated and never tested on software
UI. The famous statistic is also routinely restated into something stronger than
its source — Blackwell's line is that **nine out of ten "unencumbered pedestrians"
go out of their way to use a curb cut**, sourced to a journalist citing an
unnamed observational study at one Florida shopping mall. It is a claim about what
unimpaired people *choose*, not about the composition of curb-cut users, and it is
folklore rather than measurement. The convergence argument stands on its own
mechanics; it does not need the analogy.)*

**The divergence is a single, sharp point: single-character shortcuts.**
Power-user design loves them — `j`/`k`, `g g`, `/` to search. **WCAG SC 2.1.4
Character Key Shortcuts (Level A)** says: if a shortcut uses only letter,
punctuation, number or symbol characters, one of three must be true — a mechanism
to **turn it off**; a mechanism to **remap** it to include a non-printable key; or
**the shortcut is active only while the relevant component has focus.**

**The reason is concrete.** Speech-input users dictate text; the dictation software
emits letters. The W3C's own example: a colleague saying "Hey Kim" near a live
microphone fires `archive` (Y), `navigate` (K) and `mute` (M).

**All three remedies are cheap; none is free. "Active only on focus" is usually the
elegant answer, because it also makes the shortcut discoverable in context.**

**The convergence has a real limit beyond 2.1.4.** A power-user interface rewards
*density* and *modality* — and **both are hostile to cognitive accessibility (COGA
objective 6: do not rely on memory) and to screen reader users, for whom a modal
interface with invisible state is close to unusable.** `role="application"`, which
switches screen readers out of browse mode into raw key passthrough, is the extreme
case: it hands you the whole keyboard and **takes away every navigation affordance
the user has spent years learning.**

**The safe boundary: speed features must be additive over a fully navigable
default, never a replacement for it.**

---

# Part IX — Case studies

Eight tools, chosen for what their layout decisions *cost* rather than for what
they got right.

## 49. Epicenter design — 37signals

**"Getting Real" (2006), ch. 47.** The method inverts the usual order of layout
decisions. Instead of drawing the frame — header, sidebar, nav, footer — and
pouring content into whatever hole is left, **you identify "the true essence of
the page," design that first and alone, and only then ask what must surround it.**

> "If you're designing a page that displays a blog post, the blog post itself is
> the epicenter. Not the categories in the sidebar, not the header at the top, not
> the comment form at the bottom, but the actual blog post unit."

**The mechanism is about decision order, not aesthetics.** Chrome is cheap to add
and expensive to remove, because **once a sidebar exists it acquires occupants:
every feature with no natural home gets filed there, and the sidebar silently
becomes the app's junk drawer.** Designing the epicenter first forces every later
element to argue for its space against something already known to be essential.

Shape Up adds the abstraction-level discipline: **breadboard using only *places*,
*affordances* and *connection lines***, and sketch with a marker so thick that
"adding detail is difficult or impossible," which keeps the argument on structure
instead of pixels.

**What it costs:** epicenter design **optimises each screen in isolation.** An app
is a *set* of screens sharing chrome, and a sequence of individually-epicentred
screens can end up with inconsistent navigation and no coherent global model —
**beautiful rooms and a confusing house.** It also biases hard toward
single-purpose screens: **genuinely comparative work has no single epicenter**, and
forcing one produces an app where every task requires navigation.

## 50. VS Code, Zed, Linear — how professional tools stage density

**VS Code's workbench** names six parts: an **activity bar** (far left), a
**primary side bar**, a **secondary side bar**, the **editor** (splittable n
ways), a **panel** below the editor, and a **status bar**.

**The load-bearing move is that the activity bar is a mode selector for one
region, not a navigation tree.** It converts an unbounded set of tools into a
fixed-width column of icons, and whichever tool is selected gets the whole side
bar. **Density is therefore *staged*: at rest you see one tool's worth of surface,
never all of them. This is what lets an IDE hold hundreds of features without
looking like one.**

**Zed generalised it** by removing fixed roles: three docks (left, right, bottom),
any panel assignable to any, plus a zoom action that expands a pane to fill the
window.

**Linear went the other way, and the decision is the interesting part.** Its
redesign **explicitly deprioritised changing navigation architecture**, reasoning
that it would demand engineering investment and force users to relearn behaviour.
Instead they tuned the existing sidebar-plus-topbar to "reduce visual noise,
maintain visual alignment, and increase the hierarchy and density of navigation
elements."

Their best small idea: **the theme system went from 98 variables per theme to
three** — base colour, accent colour, contrast — with a high-contrast accessibility
variant falling out of the same three knobs. *(LCH was retained from the previous
system, not adopted as part of the redesign — this is frequently misreported as an
HSL→LCH switch.)* **The generalisable point: a colour system parameterised on
perceptually-uniform axes lets accessibility be a *setting*, not a second theme to
maintain.**

**What the frame costs:** it is where features go to hide, and **a dock is a
standing invitation to add one more panel nobody opens.** Full rearrangeability
buys power-user fit at the cost of a **shared vocabulary** — no two users' screens
look alike, screenshot-based support degrades, documentation has to say "wherever
you put the terminal." Linear's opposite choice preserves legibility but means the
IA can only be fixed by a migration nobody wants to schedule.

*(Evidence note: convergent practice across nearly every professional desktop tool
of the last decade — strong evidence of a stable local optimum — but **essentially
no published usability data comparing skeletons head to head.** Linear's rationale
is first-party and reasoned; Zed's post is a changelog offering no rationale at
all, which is itself telling about how much of this is inherited rather than
derived.)*

## 51. Things 3 — time horizons instead of due counts

This is the most directly relevant case study for any app trying to avoid
obligation framing, and its mechanism is more specific than "restraint."

**Things' top level is four *horizons*, not four priorities:**

- **Today** — "to-dos that you want to start before the day ends."
- **Upcoming** — "a timeline of your to-dos, organized by when you'll start them,
  when they have deadlines, or when they'll repeat next."
- **Anytime** — "home for all of the to-dos you could start at any time."
- **Someday** — things "you might like to get to, but you're not sure when."

**The load-bearing move is separating *When* (a start date — when you intend to
pick this up) from *Deadline* (an externally imposed date).** Almost every
competitor collapses these into a single "Due" field, **which has a structural
consequence: every item you schedule becomes an item that can be *late*.**

**Splitting them makes scheduling an act of planning rather than an act of
promising** — and it means the overwhelming majority of a Things library **can
never turn red**, because most items only ever carry a When.

**Someday is the second mechanism, and it is subtler than it looks.** It is a
blessed, legitimate destination for work you are not going to do soon — **a place
to *put* something rather than a failure state** — and it does not accumulate a
visible count. The app's own framing: *"There's too much for you to get done in
one day, but that's no reason to stress."* The stated goal is feeling **in
control**, not clearing a queue.

**What the restraint costs — and it is genuine cost, not free virtue.** Things
shipped without collaboration, without meaningful automation for years, and with a
reputation for glacial release cadence; the same discipline that keeps it calm
makes it inflexible and has cost it users. **And the When/Deadline split is harder
to teach than a single Due field** — new users reliably misuse it until someone
explains the model. **That is a real onboarding tax paid to avoid a guilt tax.**

*(Evidence: design taste with an exceptional reception record — Apple Design Award,
unusual longevity, a paid-up-front business surviving on craft rather than
engagement — but Cultured Code published no study. The psychological claim that
visible due-counts produce obligation and avoidance is plausible and consistent
with goal-conflict literature, but it is **inference, not measurement**, and it is
repeated in design circles with far more confidence than that.)*

## 52. Arc — the novelty tax, measured

**The most useful product-design retrospective of recent years, because it arrives
with instrumented numbers.**

Arc rethought browser IA from scratch: a vertical collapsible sidebar instead of a
horizontal tab strip, Spaces instead of windows, Live Folders, tabs that expire.
In 2025 The Browser Company announced Arc would be maintained but not further
developed.

Josh Miller's post-mortem names the failure a **novelty tax** — the product was
different enough to require relearning, and the payoff did not cover the cost. He
quotes Scott Forstall's verdict: **Arc "felt like a saxophone — powerful but hard
to learn."** And he concedes the product lacked a coherent centre.

**The adoption data is the part worth memorising.** Among *daily active* users — a
self-selected enthusiast base that had already chosen an unusual browser:

| feature | adoption |
|---|---|
| multiple Spaces | **5.52%** |
| Live Folders | **4.17%** |
| Calendar Preview on Hover | **0.4%** |
| *(successor product, for contrast)* chat with tabs | 40% |
| *(successor)* personalization | 37% |

**These are the signature features — the ones in the marketing.**

**The generalisable lesson is not "don't innovate on IA."** It is that **a
rearranged information architecture must be load-bearing for the *primary* task,
because secondary-feature adoption runs an order of magnitude below what teams
assume, and every novel structure is charged against one shared learning budget.**

**Hold the numbers and the narrative apart.** The instrumented data is trustworthy
and published against the team's own interest. The *interpretation* is theirs and
is contestable: Arc also had distribution, platform and strategy problems that the
letter folds somewhat conveniently into a design narrative. **And the same numbers
can be used to justify never doing anything interesting** — Arc's vertical sidebar
was widely imitated and is arguably its lasting contribution to the category. **Low
adoption of a feature is not low value of that feature**; a 5% feature can be the
entire reason 5% of users stay.

## 53. Obsidian, Roam, and the honest critique of bidirectional linking

**Four distinct UI patterns get discussed as one, and they have very different
value:**

- **Backlink pane** — a list of notes linking here. **This one earns its keep:** a
  maintenance-free index that makes a note reachable from directions its author
  never anticipated.
- **Unlinked mentions** — notes containing the title as plain text. **This is a
  suggestion engine**, and its precision collapses for short or common titles. **It
  is where link inflation starts.**
- **Transclusion / block embeds** — inlining another note's content. **Nelson's
  original insight was that transclusion requires *stable addressing*;** markdown
  block IDs approximate that badly, which is why embeds are the main source of
  structural fragility in these vaults.
- **The graph view** takes the hardest criticism and deserves it. **A
  force-directed layout of a personal vault is a hairball whose node positions are
  artifacts of a physics simulation, not of semantics** — so the picture is not
  readable, not stable between sessions, and cannot be returned to. It is in every
  screenshot of every marketing page and is almost never how people navigate. **Its
  actual function is *legibility of effort*: it makes accumulated work visible,
  which is motivational, not navigational.** That is a real job — just not the
  advertised one.

**Dan Shipper's "The Fall of Roam" locates the deeper failure as emotional rather
than technical.** Bidirectional linking sold relief from the anxiety of *"where am
I going to put this?"* — and **that relief holds only while you believe you will
retrieve the notes later.** He found **"the need to take notes far outstripped the
need to review them,"** and once the belief lapsed the vault read as "a garbage
dump full of crufty links and pieces of text."

**Matuschak concedes the burden himself:** dense linking works only for notes
deliberately "written and organized to evolve, contribute, and accumulate over
time" — atomic, concept-oriented, in your own words. **Most captured material is
none of those, so linking it produces edges without meaning.** Maggie Appleton's
critique is sharper: the field mistook "tools that allow you to link notes
together" for *the epitome of a tool for thought*.

**Costs, stated plainly:** backlinks push a system toward flat, uniformly-sized
notes, **because links become the only structure** — which fights the folders and
hierarchies people actually reach for once a vault gets large. Unlinked mentions
and auto-suggest produce **link inflation: more edges, less signal, and a graph
that gets *less* useful the more diligently you use the feature.** And **the whole
pattern's value is back-loaded** — it costs effort now and pays only if you return.

*(What neither side has: **published measurement.** There is no study on
graph-view navigation or backlink retrieval rates in either direction, which is
remarkable for a pattern this widely shipped.)*

## 54. Opinionated versus malleable — Muse, iA, Obsidian

Two published positions in direct conflict.

**Muse (Ink & Switch)** states the opinionated position most sharply: **"a highly
opinionated approach where the tool supports only one ink type for any given media
type"** — on the argument that a customisation dialog fragments attention at
exactly the moment the user was trying to think. Its layout rule is the same
sentence applied to chrome: **"No chrome. Avoid toolbars, buttons, or other
administrative debris. Just you and your work."** Muse also treats *responsiveness*
as part of calm, targeting 120fps — the same instinct as the local-first essay's
first ideal, **"No spinners: your work at your fingertips."**

**iA Writer** is the reading-and-writing-surface version: hide the interface once
typing begins, dim everything but the current sentence, keep formatting out of the
way.

**Obsidian's counter-position is not naive.** Its manifesto: *"Malleable: tools
should adapt to your way of thinking, not the other way around."* It ships 30 core
plugins atop a large community ecosystem, betting the default is only a starting
point. **The cost shows in the community's own folklore:** vaults that become
configuration projects, plugin conflicts, breakage on update, and the fact that **no
two users' Obsidian behaves alike, which makes shared help nearly impossible.**

**The synthesis most durable tools land on: opinionated layout and defaults,
malleable content and data.** Obsidian's *other* four principles — Yours, Durable,
Private, Independent, all about plain open files and no lock-in — **are the
malleability that actually matters, and they are fully separable from UI
configurability.**

**What opinionated design costs:** it **fails specific users completely rather
than most users slightly.** A single ink type is simply wrong for someone; a fixed
measure is wrong on a 6K display. **"No chrome" costs discoverability directly** —
the recognition-over-recall problem again. Tellingly, **Muse needed physically
distinct stylus grips (a recall interface with a bodily mnemonic) to make its
modelessness workable at all, and that is not an option a keyboard-and-mouse app
has.** Meanwhile the malleable position's real cost is that **the extension surface
becomes the product's de facto architecture, and it is then impossible to change.**

## 55. The session-frequency observation

Across the case studies, one pattern is worth pulling out because it dissolves an
argument that usually goes nowhere.

**Dense tools:** Linear, VS Code, Zed, Bloomberg — all *transactional*, all
returned to many times a day.
**Airy tools:** Things, iA Writer, Muse, Bear — all *reflective*, all visited less
often for longer.

**The split correlates almost perfectly with session frequency: many short visits
favour density; few long visits favour air.** Which suggests **this is not really a
disagreement so much as two answers to two different questions — and that tools
which get it wrong are usually tools that misidentified their own session
pattern.**

Related, and worth holding next to it: **an app whose surfaces have *different*
session patterns has no reason to give them the same density.** A home surface
opened once a day and a work surface occupied for an hour are, on this reading,
different kinds of room.

> **Live disagreement — is quantified progress motivating or corrosive?**
> *Motivating, if opted into:* iA Writer keeps a word count visible on an
> otherwise stripped screen; Readwise built a product on a daily review surfacing
> **a fixed small number** of highlights, and reports that "the practice of
> consistently reviewing a few highlights per day really resonated" — note the
> design detail: **a fixed daily portion, not a backlog that grows when you skip.**
> *Corrosive when it accumulates:* Things' entire IA prevents items turning into a
> count of failure. Anki's due count is the canonical case, and the canonical
> source of abandonment when it grows.
> **The reconciling observation both sides half-state: a metric of *what you did*
> (words written, highlights seen) behaves completely differently from a metric of
> *what you have not done* (due, remaining, unread). Only the second one grows
> while you sleep.**

---

# Part X — Folklore: numbers to stop repeating

Every claim below circulates widely in design writing. Each was checked against
its primary source during this research and found wrong, misattributed, or
materially overstated. **They are collected here because knowing *which* famous
numbers are broken is more durable than knowing any of the good ones.**

| the claim as usually stated | what is actually true |
|---|---|
| "Whitespace improves comprehension by 20% (Lin, 2004)" | **Lin 2004 is a different paper about hypertext and older adults, and never manipulated whitespace.** The real source is Chaparro et al. 2004; the margins main effect is **+15.5%**, N=19, **and margins made reading ~7% slower.** |
| "Miller: 7±2 items, so menus should hold ≤7" | Miller's 1956 paper was a deliberately arch survey of several unrelated limits, not a capacity constant. **Cowan (2001) puts pure chunk capacity nearer 4** — and NN/g explicitly rejects the menu inference, because interfaces are recognition tasks, not recall tasks. |
| "Design for the F-pattern" | **The F-pattern is a symptom of unformatted text**, and NN/g calls it harmful. The pattern to engineer is layer-cake. |
| "The Gutenberg diagram / Z-pattern shows how the eye moves" | **No published eyetracking basis.** The Gutenberg diagram's own scope condition — evenly distributed, homogeneous information — disqualifies any page with hierarchy. The Z-pattern is a template circulating as a finding. |
| "Right rails get 0.8% of attention for 25% of the area" | **A single illustrative gaze plot: one user, one page, 1 of 132 fixations.** The direction is supported; the magnitude is anecdote. |
| "Banner blindness: 58% vs 94%" | **From the six-person pilot**, not the 72-participant main experiment. The main experiment produced recall figures (23.9%). |
| "Under 5% of users change any setting (Spool)" | An unpublished blog account of an internal sampling of Word config files. **No methodology, sample frame or date ever released.** Direction probably right; not science. |
| "Nine in ten curb-cut users have no mobility impairment" | The original says **nine in ten *unencumbered pedestrians divert to use* a curb cut** — a claim about choice, not population — sourced to a journalist citing an unnamed mall study. |
| "Dyslexia fonts help dyslexic readers" | **Kuster et al. (2018): "neither benefits nor impedes."** What helps is size, line spacing, measure and reduced luminance contrast. |
| "People prefer golden-ratio rectangles (Fechner)" | Fechner's ~76% went to *three* rectangles of medium proportion, not to φ. **Green's 1995 review of the replication record is the canonical debunking.** |
| "The classic typographic scale is a geometric progression" | It is not. 42pt missing, six notes in the first interval instead of five, a semitone error at 30/60, 72 rounded down. |
| "Rams said use less UI" | He wrote **"as little design *as possible*"**, in an argument about material waste, not interface chrome. |
| "Calm technology means showing less" | Weiser & Brown explicitly argue for **bringing *more* detail into the periphery.** |
| "Progressive disclosure should never exceed two levels" | **Nielsen's expert judgement rendered as a rule, with no study given.** A smell test, not a limit. |
| "The mouse is faster than the keyboard (Tognazzini)" | Tog's setting was **cursor movement under decision load**, not memorised hotkeys versus menus. Where *that* was measured: shortcuts **1.362s**, toolbars 2.169s, menus 3.129s. |
| "Users prefer the menu they're slowest with" | Adaptive was slowest; **adaptable was preferred and sat between** the two on speed. The tidy inversion is not in the data. |
| "APCA Lc 60 ≈ WCAG 4.5:1" | Only against **specific mid-light grey backgrounds** (`#d0d0d0`). On white, Lc 75 is **5.10:1**, not 7:1 — the shorthand is off by ~1.4×. |
| "WCAG 2.1 SC 1.4.8 caps the measure at 80 characters" | Correct requirement, **wrong version — it is a WCAG 2.0 criterion**, carried through unchanged. |
| "Rello & Baeza-Yates, 'Make It Big!' (CHI 2016)" | **Rello, Pielot & Marcos.** Baeza-Yates co-authored other Rello readability papers, which is where the confusion comes from. |
| "68ch is roughly 68 characters" | **`ch` is the advance width of the zero glyph** — typically 20–30% wider than the average character in a proportional face. `68ch` ≈ 85–90 real characters. |

---

# Part XI — The live disagreements, in one place

Ten questions where credible practitioners genuinely disagree, collected so they
can be argued rather than accidentally settled.

1. **8pt or 4pt base unit?** — Emerging majority: 4pt inside components, 8pt for layout.
2. **Rigid grid or intrinsic layout?** — Less binary than it sounds; Gerstner's own answer sits closer to intrinsic.
3. **Ratio-generated or hand-picked type scales?** — Honest middle: seed with a ratio, then round to the spacing grid and accept the result is no longer geometric.
4. **Baseline grids on the web?** — Practice abandoned them; **the strongest technical objection expired in 2025 and nobody revisited it.**
5. **Is modern software too sparse?** — The two sides are mostly arguing about different users.
6. **Can density be a setting?** — Nobody has published adoption data for a shipped density toggle.
7. **Detail pane right or below?** — Content wants height; a right pane on a wide window forces a ~65ch cap and wastes the width. Thunderbird refuses to choose.
8. **Overview+detail or focus+context?** — Splits by task: static comprehension vs dynamic tracking. **Preference cannot arbitrate — users prefer overviews that slow them down.**
9. **Everything visible, or one thing at a time?** — Side A is right about *comparison* tasks, side B about *production* tasks. Each camp generalises its own case.
10. **Opinionated or malleable?** — The durable split is **opinionated layout, malleable data**; the two are separable and are usually conflated.

---

# Part XII — Reading list, ranked

**Start here (three):**

1. **[Cockburn, Karlson & Bederson, "A Review of Overview+Detail, Zooming, and Focus+Context Interfaces"](https://faculty.cc.gatech.edu/~stasko/7450/Papers/cockburn-surveys08.pdf)** (2008) — defines the three archetypes precisely, then reports what the literature actually found for each, including the awkward results where preference and performance diverge.
2. **[Woods, "Visual Momentum"](https://ferd.ca/notes/paper-visual-momentum.html)** (1984) — the best theory of what a transition between views costs, and a ranked ladder of techniques from total replacement to spatial cognition.
3. **[Dyson, "Line length revisited"](https://designregression.com/article/line-length-revisited-following-the-research)** — the model for how to read every other claim in this field: ask which variable was optimised, and what it cost.

**Layout and spatial structure:**

- **Karl Gerstner, *Designing Programmes*** (1964) — where "the deliverable is the generator, not the artefact" comes from. Read it before any design-system book; everything after it is a footnote. [PDF](https://openlab.citytech.cuny.edu/langecomd3504sp2020/files/2018/10/Gerstner_DesigningProgrammes-1.pdf)
- **Müller-Brockmann, *Grid Systems in Graphic Design*** — for the *construction* method (module derived from a whole number of text lines) and the "will to systematize" passage, which is design ethics rather than layout advice. Skim the poster case studies; they date.
- **Timothy Samara, *Making and Breaking the Grid*** — the clearest taxonomy plus the anatomy vocabulary. Its second half, on when content's structure means the grid should be abandoned, is the honest counterweight to Müller-Brockmann.
- **[*Every Layout*](https://every-layout.dev/)** (Pickering & Bell) — the best statement of "layout is a system of relationships, not a set of boxes," and unusually it ships working code.
- **[Nathan Curtis, "Space in Design Systems"](https://medium.com/eightshapes-llc/space-in-design-systems-188bcbae0d62)** — the inset/squish/stretch/stack/inline taxonomy, which explains why one token cannot serve every gap.

**Hierarchy, grouping, density:**

- **[Brooks, "Traditional and New Principles of Perceptual Grouping"](https://kar.kent.ac.uk/35324/1/Brooks-GroupingChapter-OUPHandbook-REPOSITORY.pdf)** — the single best source on the question designers actually have: what happens when grouping cues conflict. Honest that no universal hierarchy exists.
- **[Wagemans et al., "A Century of Gestalt Psychology"](https://www.elderlab.yorku.ca/wp-content/uploads/2016/12/WagemansPsychBull12.pdf)** (2012) — calibrates how much of what design writing attributes to Wertheimer was ever tested.
- **[Matt Ström-Awn, "UI Density"](https://mattstromawn.com/writing/ui-density/)** — density as value ÷ time and space occupied. Turns "is this too dense?" into five separable questions.
- **[Stephen Few, "The Chartjunk Debate"](https://www.perceptualedge.com/articles/visual_business_intelligence/the_chartjunk_debate.pdf)** — the best worked example in design of reading a study properly, and a model for disagreeing with evidence you partly accept.
- **[Mark Boulton, "Whitespace"](https://alistapart.com/article/whitespace/)** — the origin of the macro/micro and active/passive vocabulary. Worth reading partly to notice it cites no evidence at all.

**Multi-pane and navigation:**

- **[Shneiderman, "The Eyes Have It"](https://hci.stanford.edu/courses/cs448b/papers/shneiderman96eyes.pdf)** (1996) with **[Furnas, "Generalized Fisheye Views"](https://cspages.ucalgary.ca/~saul/581/exer.eps/4furnas86.pdf)** (1986) — the two short primaries everyone cites and few read. Furnas gives you `DOI = API − distance`, which underlies every collapse, fold and degradation-with-distance decision you will ever make.
- **[Jensen Harris, "Designing Microsoft Outlook"](https://jensenharris.com/home/outlook)** — a rare first-person account of why a canonical three-pane layout is shaped as it is, at exactly the grain of detail this topic usually lacks.
- **Teevan et al., "The Perfect Search Engine Is Not Enough"** (CHI 2004) with **[Harrison & Dourish, "Re-place-ing Space"](https://www.dourish.com/publications/2006/cscw2006-space.pdf)** — the empirical case for orienteering, plus the discipline that "place cannot be drawn, only earned through stability."
- **Jenifer Tidwell, *Designing Interfaces*** — names the archetypes (Two-Panel Selector, Canvas Plus Palette, One-Window Drilldown). **It is much easier to reason about a tradeoff you can name.**

**Novice/expert:**

- **[Cockburn, Gutwin, Scarr & Malacria, "Supporting Novice to Expert Transitions in User Interfaces"](https://doi.org/10.1145/2659796)** (ACM Computing Surveys, 2014) — **the single best thing on this brief.** Organises every mechanism into one framework and says which have evidence.
- **[Malacria et al., "ExposeHK"](https://www.csse.canterbury.ac.nz/andrew.cockburn/papers/ehk.pdf)** (CHI 2013) — the definitive demonstration that shortcuts get learned when the novice action *is* the expert action rehearsed. **This is the paper on teaching shortcuts without nagging.**
- **[Findlater & McGrenere, "Beyond Performance"](https://www.cs.ubc.ca/labs/edapt/papers/findlater2010.pdf)** (2010) — the most important *negative* result in this literature, and the one that generalises furthest: it applies to any hiding scheme, progressive disclosure included.
- **Jef Raskin, *The Humane Interface*** — read it for the mechanism (habituation), argue with the conclusions (monotony).
- **[Havoc Pennington, "Free Software and Good User Interfaces"](https://ometer.com/free-software-ui.html)** — a decision procedure rather than a slogan. Read alongside Spolsky's "Choices" and Torvalds's 2005 reply for the whole argument in forty minutes.

**Calm and restraint:**

- **[Weiser & Brown, "The Coming Age of Calm Technology"](https://calmtech.com/papers/coming-age-calm-technology.html)** — twelve pages, and it says something quite different from what it is cited for. **Read it before quoting it.**
- **[Rogers, "Moving on from Weiser's Vision of Calm Computing"](https://doi.org/10.1007/11853565_24)** (2006) — the insider critique the field mostly ignored. The best inoculation against treating Weiser as scripture.
- **Norman, *Living with Complexity*** — the strongest counter to the restraint tradition, written from inside it. **The planishing-hammer chapter is the argument you have to answer before you delete a control.**
- **Deci, Koestner & Ryan (1999)** — read the moderator tables, not the abstract; the informational/controlling distinction is where the design guidance lives. Pair with **[Sailer & Homner (2020)](https://link.springer.com/article/10.1007/s10648-019-09498-w)**, whose own high-rigour subgroup analysis dissolves its motivational findings.

**Accessibility:**

- **[WCAG 2.2 Understanding documents](https://www.w3.org/WAI/WCAG22/Understanding/)** — far better written than their reputation. Read 2.4.11, 2.5.7, 2.5.8, 1.4.10, 1.4.12 and 2.1.4 first.
- **[ARIA APG — "Read Me First" and "Developing a Keyboard Interface"](https://www.w3.org/WAI/ARIA/apg/practices/read-me-first/)** — "a role is a promise" and one-tab-stop-per-composite are the two ideas that most change how you build.
- **[Sara Soueidan, "Designing Accessible, WCAG-Conformant Focus Indicators"](https://www.sarasoueidan.com/blog/focus-indicators/)** — the single best piece on focus.
- **[Adrian Roselli, "WCAG3 Contrast as of April 2026"](https://adrianroselli.com/2026/04/wcag3-contrast-as-of-april-2026.html)** with **[Myndex, "Why APCA"](https://git.apcacontrast.com/documentation/WhyAPCA.html)** — take the diagnosis seriously and the Lc thresholds as one expert's judgement.
- **[Heydon Pickering, *Inclusive Components*](https://inclusive-components.design/)** — the counterweight to the APG. Where the APG tells you how to build a tab widget, Pickering asks whether you should.

**Case studies:**

- **[37signals, "Epicenter Design"](https://basecamp.com/gettingreal/09.2-epicenter-design)** — two pages, and the shortest route to a working method for laying out a screen.
- **[The Browser Company, "Letter to Arc members"](https://browsercompany.substack.com/p/letter-to-arc-members-2025)** — read it for the data (5.52% / 4.17% / 0.4%) and discount the narrative. **Nothing else will recalibrate your estimate of how many people find a novel feature.**
- **[Dan Shipper, "The Fall of Roam"](https://every.to/superorganizers/the-fall-of-roam)** — the honest interior account of why a link graph stops being useful.
- **[Ink & Switch, Muse](https://www.inkandswitch.com/muse/)** with **["Local-first software"](https://www.inkandswitch.com/essay/local-first/)** — the values and the worked example of what those values do to a layout.
- **[Linear, "How we redesigned the Linear UI"](https://linear.app/blog/how-we-redesigned-the-linear-ui)** — the clearest published account of treating chrome as *the* design problem, plus the striking decision to deliberately not change navigation architecture.

---

# Appendix — Numbers worth keeping, with their firmness

**Layout and space**

| number | claim | firmness |
|---|---|---|
| 1/9, 1/9, 2/9, 2/9 | Van de Graaf canon margins (inner, top, outer, bottom) — works for any page ratio | standardised (a construction) |
| 8–32 fields | range Müller-Brockmann documents for modular grids | conventional |
| 5px × 1.5 = 7.5px | the half-pixel offset that is the only technical argument for an even base unit | measured |
| 8dp; 8/16/24/40dp | Material's baseline grid and permitted margins/gutters | standardised |
| Chrome 105 / Safari 16 / Firefox 110 | container query support (Aug 2022 – Feb 2023) | measured |
| ~5% | optical size compensation between a square and a circle of equal nominal size | folklore, but unusually consistent |
| inner = outer − padding | concentric nested corner radii; floor at 2–4px rather than square | standardised (geometry) |

**Perception and scanning**

| number | claim | firmness |
|---|---|---|
| 38 / 28 / 228 fixations | same page, three different tasks — **task dominates layout** | measured |
| 540 ms vs 743/1283 ms | common region's interference cost on connectedness-grouped targets | measured |
| 9 scales, 4 orientations, 42 feature maps, 3 conspicuity maps | Itti, Koch & Niebur's saliency architecture — what the squint test informally reads | measured |
| three | levels of visual dominance a viewer can distinguish | conventional |
| ~4 chunks | working memory (Cowan 2001), replacing 7±2 | measured — but panes are not chunks |
| ~20/second | rate of attention deployment (Guided Search 6.0) | measured |
| 0 / 90 / 250–350 ms per item | search slopes: pop-out / guided conjunction / serial foveation | measured |
| >150 ms/item | item recognition alone | measured |
| 32×32 px colour | scene recognition >80%, ~7pp below full resolution; greyscale needs ~64×64 | measured |

**Typography**

| number | claim | firmness |
|---|---|---|
| 45–75 chars, 66 ideal | Bringhurst's measure — **he is reporting consensus, not a result** | conventional |
| 80 chars (40 CJK) | WCAG 2.0/2.2 SC 1.4.8 (AAA) cap — justified on disability grounds | standardised |
| 95 cpl fastest, 35 slowest | Shaikh (2005) online news; **no comprehension effect**; preference split 30%/30% at the extremes | measured |
| 27% faster, >50% fewer regressions | dyslexic readers at ~12.7 cpl vs ~67.2 cpl (n=27) | measured |
| ~130 ms vs ~250 ms | undersweep vs ordinary fixations — **the undershoot is not wasted time** | measured |
| 1ch ≈ 20–30% wider than average char | the CSS `ch` unit is the zero glyph's advance width | measured |
| 1.5–1.6 / 1.3–1.45 / ~1.1 | leading for desktop long-form / narrow column / headings | folklore |
| ≥18pt | Rello, Pielot & Marcos's body-text recommendation; fixation duration fell up to 22pt | measured |
| p=.978 | line spacing's effect on fixation duration — **null** | measured |
| <200–250 ms | preattentive detection threshold; luminance > hue > shape/texture | measured |
| 35% / 20% | within-person spread between best and worst font; rate at which the preferred font is fastest (= chance) | measured |

**Dark mode and contrast**

| number | claim | firmness |
|---|---|---|
| d=2.17 (young), d=0.58 (older) | positive-polarity acuity advantage; **no eyestrain difference between polarities** | measured |
| 7 of 7 | cataract patients reading faster in dark mode — the real basis of the "astigmatism" claim | measured, tiny sample |
| #121212; 87/60/38% | Material 2 dark surface and emphasis opacities (white on it: 18.73:1 pure, 14.19:1 at 87%) | conventional |
| 4.5:1 / 3:1 / 7:1 | WCAG 2 AA body / large text and non-text / AAA | standardised |
| Lc 90 / 75 / 60 / 45 / 30 / 15 | APCA thresholds — **one researcher's judgement, and APCA is out of WCAG 3** | proposed, non-normative |
| 5.10:1 vs 11.67:1 | WCAG ratio needed to reach APCA Lc 75 on white vs on #121212 | measured (computation) |

**Panes, navigation, collapse**

| number | claim | firmness |
|---|---|---|
| 3.5 visible (median 3) → 6.8 on multi-monitor; 78.1% of time ≥8 open; median activation 3.77s | Hutchings et al., real usage logs, 2004 | measured |
| 52% / 64% / 75% | Furnas: navigation accuracy, two flat views → one fisheye → two fisheye | measured |
| up to 56% faster | focus+context vs overview+detail in dynamic tracking | measured |
| ≥39% slower, >20% discoverability loss, 21% harder | cost of hidden navigation on **desktop** (worse than mobile's 15%) | measured |
| 640 / 1008 px | WinUI NavigationView collapse thresholds | standardised |
| 400 / 550 / 860 sp | libadwaita collapse thresholds (sidebar / view switcher / triple-pane) | standardised (convention) |
| 1024×600 | GNOME's smallest supported desktop size | standardised |
| ~65 chars | Outlook 2003's reading-pane cap — a research-informed practitioner decision from 2003 | conventional |
| 2 panes typical, 3 max | Apple HIG on split views | standardised |
| 33/66/99 items, n=69 | spatial retrieval got **worse** from 2D → 2.5D → 3D | measured |

**Novice/expert**

| number | claim | firmness |
|---|---|---|
| 306.5 / 318.8 / 331.6 ms | static / adaptable / adaptive menu selection — **static fastest, adaptable preferred (55%)** | measured |
| 3.7s vs 2.9s; 20.7% vs 27.0% | high-accuracy adaptation's cost on new items and feature awareness | measured |
| 74% / 81% / 3.8 days | McGrenere field study: time in self-built interface / additions in first two days / days spent personalising | measured |
| 1.57s vs 2.11s vs 2.40s; 0.6% vs 5% vs 9% errors | CommandMaps vs ribbon vs menus, experienced users — **and novices cost nothing** | measured |
| 1.362 / 2.169 / 3.129 s | shortcut / toolbar / menu selection time | measured |
| ~6% | users who favoured keyboard shortcuts across commands, despite years of experience | measured |
| 94% / 99% vs 50% / 35% | ExposeHK hotkey adoption vs audio feedback vs tooltips | measured |
| ~650 selections | repetitions before the transition to expert gestural invocation | measured (n=2, longitudinal) |
| P < 0.20 | probability two people spontaneously choose the same term — **the ceiling on any name-driven interface** | measured, heavily replicated |
| 2 seconds | Emacs `suggest-key-bindings` echo duration — the grammar of non-nagging | standardised (shipping default) |

**Collections and accessibility**

| number | claim | firmness |
|---|---|---|
| CLS ≤0.1 good, >0.25 poor, at p75 | Core Web Vitals layout-shift thresholds | standardised |
| ~5.5s | point at which skeleton screens stop beating spinners (n=80, author-caveated) | measured, weakly |
| 24×24 / 44×44 CSS px | WCAG 2.2 target size AA / AAA | standardised |
| 320 × 256 CSS px | reflow requirement (= 400% zoom on 1280px) | standardised |
| 1.5 / 2 / 0.12 / 0.16 | text-spacing overrides content must survive (line height / para / letter / word) | standardised |
| 71.6% vs 3.7% | screen reader users navigating by heading vs by landmark | measured (self-selected sample) |
| 59.1 vs 42 | detected errors per page, with ARIA vs without (WebAIM Million 2026) | measured, correlational |
| ≤7 | recommended landmark regions per page | conventional |
| 5.52% / 4.17% / 0.4% | Arc's signature-feature adoption among **daily active** users | measured |

---

# A closing note

Three things this research changed most, stated as observations rather than
advice:

**First, the field's confident numbers are mostly not measurements.** The 66-character
measure, the 8-point grid, the three-level hierarchy, the two-level disclosure
ceiling, the seven-item menu — every one is craft judgement or engineering hygiene
that acquired the grammar of a finding. That does not make them wrong. Craft
consensus of a century is real evidence about what works. But it should be held
differently from an effect size, and it can be argued with.

**Second, almost every layout principle is a trade with a named victim.**
Whitespace costs the user with a lot of data. Density costs the newcomer. Hiding
costs discoverability at a *measured* 39%. A persistent rail costs attention until
it becomes invisible. Small tiles abandon anyone who does not already know their
library. Calm costs the thing you did not surface. **The literature is far better
at naming these costs than at resolving them, and the sections above that resolve
nothing are the honest ones.**

**Third, and most consistently: preference and performance dissociate.** Users
prefer overviews that slow them down. They prefer the menu they are not fastest
with. They read fastest in fonts they did not choose. They want short lines they
read more slowly. They prefer dark mode, which measures worse. **Design validated
by asking people what they want will be wrong roughly half the time — and design
validated only by stopwatch will build something nobody wants to sit with.**

The interesting work is in knowing which of the two a given surface is for.
