# @exp/styleguide

The reference page for the whole product's surface, in **three modes** picked
from one segmented bar at the top of the sidebar (EXP-941):

| mode | what it holds | where it comes from |
| --- | --- | --- |
| **Views** | every screen in `@exp/view-catalog`, with its web / web-mobile / desktop / iOS / Android shots side by side | photographs in `shots/` (EXP-566) |
| **Components** | the glass control set — the REAL `@exp/ui` component wherever one owns the form | rendered live at build time (EXP-698 / EXP-887) |
| **Style** | the values the controls are made of: colour, shape & size, type, motion, and the icon registry | `@exp/design-tokens` and `packages/icons` |

Everything the page does happens WITHIN the current mode: the filter, `j`/`k`,
the summary line. A hash always wins — `#pill` switches to Components and then
shows the entry — and `1` / `2` / `3` switch modes, with the last one
remembered in `localStorage`.

The output is ONE self-contained HTML file plus a copy of `shots/` — no
runtime dependencies IN THE OUTPUT, `open dist/index.html` works with no
server. (The build itself uses `@exp/ui` + React to render the islands.)

```bash
bun run dev:styleguide      # http://localhost:4173, re-reads the store per request (restart after a component edit; ?recompile=1 rebuilds the css)
bun run build:styleguide    # writes apps/styleguide/dist/
bun run shots:check         # build --check: fails on missing / undeclared shots
```

## Views

Per platform a view is either **captured**, **missing** or **n/a**:

- **missing** — the catalog declares a capture for that platform and nothing is
  in the store yet. Run the capture lane; `--check` exits 1 on these.
- **n/a** — the catalog declares no capture there. That is a deliberate gap and
  `views.json` `notes[platform]` says why; the placeholder shows that text.
- **undeclared** — a file in `shots/` that no view/platform pair claims. Either
  the catalog entry was renamed or the file is stale.

Only this mode has shots at all, so the toolbar's **Fit to height** toggle and
the 1:1 lightbox hint are hidden in the other two.

## The spec model

Components and Style are ONE array, `COMPONENTS` in `src/components.tsx`, so
every gate runs over all of it. A spec carries no mode field: `kind` is the
sub-group inside a mode and `modeOf(spec)` derives the mode from it, which
makes a mis-sorted entry impossible rather than merely gated.

- **Components** kinds: `Inputs & pickers`, `Buttons & chips`, `Lists & rows`,
  `Surfaces`, `Feedback`.
- **Style** kinds (`STYLE_KINDS`): `Colour`, `Shape & size`, `Type`, `Motion`,
  `Icons`.
- `KIND_ORDER` is the nav order within a mode; `MODES` holds the three
  `{ id, label, blurb }`.

### `leftovers` — what the product still draws by hand

The per-platform status table says whether a control EXISTS on a platform. It
cannot say whether the app bothers to use it, so a spec may carry

```ts
leftovers: [
  { file: `apps/web/src/components/inbox/inbox-view.tsx`, note: `a hand-rolled 28px muted disc` },
]
```

which renders under the table as **Still drawn by hand** and adds ONE extra
yellow dot to the nav link. `bun test` gates it: every file exists, every note
is one line of at most 120 characters. Fix the call site, delete the row —
that is the only way the dot goes away.

## Components

Nothing in this mode is a screenshot. Since EXP-887 most entries are
**islands**: the REAL `@exp/ui`
component, rendered to static markup inside a declarative shadow root and
painted by the package's own stylesheet, compiled once per build
(`@exp/ui/island`). The page shows the control the product ships, not a
lookalike of it — a lookalike drifts silently, a component cannot disagree with
itself.

What stays hand-written HTML/CSS driven by `@exp/design-tokens`
(`src/components.tsx` + `src/component-styles.ts`) is what no single component
owns: the **compositions** (app shell, settings page header, comment card,
sheet shell, composer, markdown blocks, tab bar, bulk bar, usage
bar, session bar, relations card, the GitHub connect pair) and the **token**
swatch tables. Two entries — **sheet shell** and **dialog** — have their web
symbol in `packages/ui` and still keep a demo, because a closed Radix portal
renders nothing at all statically; `PORTAL_ONLY_IDS` names them and the test
checks the exception stays honest. The **menu** (and the **issue context
menu** under Special) is no exception since EXP-1074: `MenuSpecimen` draws the
menu at rest on plain elements wearing the SAME row classes the Radix items
wear, in both densities the `menu` tokens define.

`shots/` holds nothing for these, `views.json` declares nothing — the two
synthetic modes have no catalog entry at all — and `--check` never sees them.

The set is deliberately SMALL, and shrinking it counts as progress. There is no
chip and no header button: both were the **pill** under a second name, so the
pill is now one 2×3 matrix — size `md` 32 or `sm` 24 × mode `action` / `select`
/ `readonly` — and a status chip is `sm readonly`, a header action `sm action`,
a conversation tab `sm select`. The other four entries that are not a single
control are **rich tab** (the terminal and top strips, the only place a tab
carries a state and a close), **text area** (the field's recipe grown, borderless
inside a group), **composer** (ONE surface for comments, steering and support
replies, with an opaque variant for mobile bottom bars) and **markdown blocks**
(the chat-sized narration / bubble / plan / question / tool / fold set the steer
feed is built from).

Two rules the entries carry rather than restate, both from EXP-771 (the first
narrowed by EXP-862):

- **Shape says what a control does.** A circle marks the PRIMARY action and
  nothing else: play / start, send, the rail's New issue and Search, a mobile
  FAB, the "+" that adds. Every secondary icon button is a **ghost icon
  button**: no circle, no border, hover fill only. That is the "…" overflow,
  close, the folder and file-list toggles, the chevrons (back, fold, reorder),
  trash and remove, and anything refresh-shaped. A rounded square at the radius
  ladder's MD step is a PICKER trigger: the **icon picker**, the colour picker
  beside it (EXP-862 made the board form's two triggers one control repeated),
  and every cell of the glyph grid. Colour swatches stay circles, because a
  colour has no shape to read, and text capsules stay at 9999.
- **Chrome sits on the ground, not in the card.** The title strip above the
  content card and the **session bar** below it are 36px bands on the bare page
  gradient with no fill and no border, their chips inset 8; the card stops 6px
  short of the bottom band, and the band runs to the window bottom. See **app
  shell**, which draws both symmetrically.

**Settings page header** is the third entry of that kind: the one header every
settings page opens with on web and desktop, on a centred 56rem column (896 at a 16px root) inside a
full-width scroll region.

**GitHub connection** and **Add-repository picker** (FEED-42) are the Settings › Repositories block and
its picker, one canonical form on all four clients (tap adds, the ✕ always confirms).

EXP-903 added two islands for primitives the app used to copy-paste: **glass card**, the ONE translucent
card box — shown bare and again as the grouped-rows variant the call site makes with `divide-y` — and
**icon disc**, the 48px heading circle in all four tones, whose wash and glyph colour are one choice.
EXP-904 added **floating chrome** (`FAB_CHROME_CLASS`, the phone bar's glass paint; size and radius stay at the
call site) and **attachment thumbnail**, the composers' 64px pending tile with its corner remove badge.

EXP-941 added the picker set the app had been copy-pasting: **combobox** (the
ONE searchable picker — its demo puts both triggers beside the two bare
`ComboboxList`s, because a closed portal renders nothing, and the test pins the
two selection languages: a trailing check for single, the leading circle pair
for multi), **search field**, **date picker**, **typeahead menu** and **alert**,
plus entries for primitives the page had never shown at all (checkbox, switch,
select, colour picker, field label, team avatar, live dot & status glyph, empty
state, skeleton, dialog). **Segmented control** now renders the real
`SegmentedControl`, in both its floating and its `embedded` form.

Under each control is a per-platform table naming the ONE symbol and file that
is supposed to match it on Web / Desktop / iOS / Android, marked `ok`,
`leftover` (it exists but still disagrees; the note says how) or `n/a` (that
platform deliberately has none). `bun test` gates the parts that rot: every
named file exists, every platform is accounted for, notes stay one short line,
**a web symbol under `packages/ui/` is an island and nothing else is**, every
island renders real markup, the page carries one shadow root per island and the
stylesheet exactly once, the page's root font size still mirrors the package's,
the hand-written demos carry no inline styles and no unused `.cmp-` rule, the
retired names never come back, the pill demo shows all six size × mode
combinations read off the real `Pill`, and `component-styles.ts` contains no
colour literal — every value is a `var(--…)` declared from the tokens, and every
radius is a ladder step.

### Adding a component entry

1. **The component lives in `packages/ui/src/<name>.tsx`** and is exported from
   `src/index.ts`. If it is not in the package it is not an entry — put it there
   first, or the gate will refuse the spec.
2. **One spec in `src/components.tsx`**: id, title, a Components `kind`, blurb,
   the four platform rows (`web` pointing at `packages/ui/src/<name>.tsx`),
   optional `leftovers`, and `island: () => <Name …fixture />`. The fixture is
   the RESTING state: no live data, `noop` callbacks, glyphs through
   `conceptIcon`.
3. `bun test` (from `apps/styleguide`, or `bun run test:shots` from the root),
   then `bun run build:styleguide` and open `dist/index.html`.

The island limits are `@exp/ui`'s (see its README): a closed Radix **portal**
renders nothing, `AvatarImage` never renders (use initials), a `Select` shows an
empty trigger once it HAS a value (so the entry shows the placeholder arm),
Inter is not loaded here, and the island never paints its own ground — the
`.cmp-demo` canvas does.

EXP-895's `FileDiffList` slots straight in: a fixture array of files beside the
spec, `island: () => <FileDiffList files={FIXTURE} />`, four platform rows, done
— no CSS, no lookalike, no capture lane.

## Style

A Style entry documents a VALUE, not a control, so it names whichever file
happens to SPEND the token and is normally a hand-written swatch table driven
by `@exp/design-tokens` — `tokens-palette` (the surface palette, the fixed
semantic accents and the diff pair), `tokens-radius`, `tokens-size`,
`tokens-type` and `tokens-motion`. Adding a colour to a token group adds its
swatch, its `.cmp-swatch .box.fill-*` rule and its `:root` var at once: all
three are generated from the same object (`swatchFills` / `colorVars`), because
a demo here may not carry a colour literal.

**The one island exception (EXP-941):** a Style entry whose web file lives
under `packages/ui/` MAY be an island. `tokens-icons` is the reason — the icon
registry is best documented by rendering its own glyphs, and its source is a
generated `.ts`, not a component file. Everywhere else the rule still holds
both ways: a web symbol under `packages/ui/*.tsx` is an island, and nothing
else is.

### Adding a Style entry

1. Pick the `kind` (`Colour`, `Shape & size`, `Type`, `Motion`, `Icons`).
2. Name the four files that hold the value — the generated token or icon
   outputs, not a call site that happens to read them.
3. Hand-write `render`, using only `.cmp-*` blocks and `var(--…)`; declare any
   new var in `styles.ts`'s `:root` from the tokens, or the gate fails.

`SHOTS_DIR` points both commands at another store (scratch copies, tests);
by default it is the repo-root `shots/`.

Deploys as a Coolify STATIC app serving `apps/styleguide/dist` — no runtime, so
the build step is the whole deploy.

Keyboard: `1` / `2` / `3` switch mode, `j`/`k` or arrows move within it, `/`
focuses the filter, clicking a shot opens it 1:1, `Esc` closes.
