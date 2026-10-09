# @exp/styleguide

The reference page for the whole product's surface, in **three modes** picked
from one segmented bar at the top of the sidebar (EXP-941):

| mode | what it holds | where it comes from |
| --- | --- | --- |
| **Views** | every screen in `@exp/view-catalog` (bar the site-owned `exponential-ui-catalog` group), with its web / web-mobile / desktop / iOS / Android shots side by side | photographs in `shots/` (EXP-566) |
| **Components** | the Exponential app's specialised controls (the generic ones live on [ui.exponential.at](https://ui.exponential.at/components/), VAPP-93) — the REAL `@exp/ui` component wherever one owns the form | rendered live at build time (EXP-698 / EXP-887) |
| **Style** | the values the controls are made of: colour, shape & size, type, motion, and the icon registry | `@exp/design-tokens` and `packages/icons` |

Everything the page does happens WITHIN the current mode: the filter, `j`/`k`,
the summary line. A hash always wins — `#issue-chip` switches to Components and then
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

**Generic specimens live on ui.exponential.at (VAPP-93).** Every control that
is part of the Exponential UI SDK's core catalog — buttons, fields, checkbox,
switch, select / combobox / the picker shell, date picker, pill, badge, avatar,
card, separator, dialog, sheet, alert, empty states, skeleton, meters, rings,
segmented controls and tabs, the row family (list, card, property, picker,
input and toggle rows), group bands, disclosure headers, tree guides, entity
chips, the composer and the menu — is documented at
[`https://ui.exponential.at/components/`](https://ui.exponential.at/components/),
where it renders live through the SDK beside its four platform shots. Their
per-platform status tables moved with them, verbatim, to
`apps/ui-site/src/data/app-parity.ts`, one row per old id. The view-catalog
group `exponential-ui-catalog` (the core-catalog specimens and the kitchen
sink) is owned by the site too: the Views nav links there instead of listing
it, while `--check` still gates its shots like every other view.

**This page holds the app extension:** the Exponential app's own specialised
components — the board / status / assignee / … pickers, issue chips and group
bands, run rows and marks, the work bar and header, the session bar, results
tiles, the diff cards, usage bars, the GitHub surfaces, brand marks, the auth
shell, the decision dialogs — plus toasts (host-owned, no core component) and
the Style mode. `components.test.tsx` fails if a moved id comes back.

Nothing in this mode is a screenshot. Most entries are **islands**: the REAL
`@exp/ui` component, rendered to static markup inside a declarative shadow root
and painted by the package's own stylesheet, compiled once per build
(`@exp/ui/island`). The page shows the control the product ships, not a
lookalike of it.

What stays hand-written HTML/CSS driven by `@exp/design-tokens`
(`src/components.tsx` + `src/component-styles.ts`) is what no single component
owns: the **compositions** (app shell, settings page header, comment card,
markdown blocks, tab bar, bulk bar, session bar, relations card, the GitHub
connect pair) and the **token** swatch tables. `PORTAL_ONLY_IDS` would name an
entry whose `packages/ui` symbol is a closed Radix portal (renders nothing
statically) and so keeps a demo; it is empty since its two members, sheet and
dialog, moved to the site. The **issue context menu** and the composer's "+"
(in **composer-dialog**, rows off `composer-menu.json`) draw the menu at rest
through `MenuPanel`: the same entries and row classes the live `Menu` renders,
picker bodies as `PickerMenuRows`.

Two rules the entries carry rather than restate (EXP-771, EXP-862): **shape
says what a control does** — a circle marks the PRIMARY action only, every
secondary icon button is a ghost, a rounded square at MD is a picker trigger,
a text button is a rounded rectangle at MD (EXP-1176) and the capsule belongs
to the pill — and **chrome sits on the ground, not in the card**: the title
strip and the **session bar** are 36px bands on the bare page gradient with no
fill and no border (see **app shell**).

Under each control is a per-platform table naming the ONE symbol and file that
is supposed to match it on Web / Desktop / iOS / Android, marked `ok`,
`leftover` (it exists but still disagrees; the note says how) or `n/a` (that
platform deliberately has none). `bun test` gates the parts that rot: every
named file exists, every platform is accounted for, notes stay one short line,
**a web symbol under `packages/ui/` is an island and nothing else is**, every
island renders real markup, the page carries one shadow root per island and the
stylesheet exactly once, the page's root font size still mirrors the package's,
the hand-written demos carry no inline styles and no unused `.cmp-` rule, the
retired names never come back (the chip, the header button, every id that moved
to the site), and `component-styles.ts` contains no colour literal — every
value is a `var(--…)` declared from the tokens, and every radius is a ladder
step.

### Adding a component entry

0. **Is it generic?** If it maps onto a core catalog component
   (`packages/exponential-ui/docs/components.generated.json`), it belongs on
   ui.exponential.at, not here.
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
