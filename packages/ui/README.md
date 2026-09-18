# @exp/ui

The web design system: the theme stylesheet, the shadcn set and the shared
feature primitives, as **raw TypeScript sources**. There is no build step — an
app imports `@exp/ui` and its own bundler compiles the `.tsx`.

Nothing in here may reach for tRPC, Electric collections or the router: this
package is the PRESENTATIONAL layer. A component that needs synced data gets
its data passed in, and the app keeps the binding beside its own state (see
`IssueChip` vs `apps/web/src/components/issue-chip.tsx`).

## What lives here

| | |
| --- | --- |
| `styles.css` | The theme: `@import "tailwindcss"`, `@theme inline`, `:root, :host`, `.dark`, the base layer, the motion/glass `@utility` recipes and the `.issue-chip` box. An app imports THIS instead of `tailwindcss`. |
| the shadcn set | 34 modules — `button`, `dialog`, `sheet`, `select`, `dropdown-menu`, `command`, `sidebar`, `calendar`, `alert`, `pill`, `glass-rows`, the colour/icon pickers … |
| primitives | `IssueChip` + `ChipRemoveButton`, `StatusGlyph`, `UserAvatar`, `TeamAvatar`, `LiveDot`, `RichTab`, `EmptyState`, `GlassCard`, `IconDisc`, `IconTooltip`, `MobilePopover` |
| pickers (EXP-941) | `PickerOption` (the ONE option shape — since EXP-958 the only one: `IssueOption` lives beside the app's status tables in `lib/domain.ts`, `GlassPickerOption` is gone), `Combobox` + `ComboboxList` (searchable single/multi select on `MobilePopover` + `Command`: trailing check for single, the leading `ui-selected`/`ui-unselected` pair for multi, `noneLabel` instead of sentinel values, `width` as an enum, `listClassName` for the host that caps the list; four triggers — `pill`, `field`, `row` = the glass form ladder's picker row, `inline` = one word of a muted sentence that collapses to text at one option — plus `renderTrigger`; a closed single-select is `searchable={false}`, which is what the status/priority menus and every settings row are now) + `ComboboxMenuItems` (EXP-957: the same rows as items inside a Radix context or dropdown menu, `menu="context" \| "dropdown"`; a multi row's `PickerOption.checked` may be `"indeterminate"`, a single arm `indeterminate` marks nothing — both for a bulk edit over rows that disagree; the arithmetic and the glyph live in `combobox-core.tsx`, exported by no arm), `SearchField` (an `Input` with the search glyph and a clear button, `size` md/sm), `SegmentedControl` (the capsule from an option array), `DatePicker` (`YYYY-MM-DD` in and out, never a `Date`), `useTypeahead` + `TypeaheadMenu` + `TypeaheadRow` (the free-text menu under a textarea or editor; EXP-959: `anchor` = the caret rect in viewport coordinates, and the menu portals to `document.body` at fixed coordinates, flipping above the caret when the room below runs out, closing through `onAnchorLost` on an outside scroll; that arm stamps `data-editor-autocomplete`, `TYPEAHEAD_PORTAL_SELECTOR`, which dialog hosts whitelist in `onInteractOutside`) |
| helpers | `cn` / `getInitials`, `MENU_SURFACE_CLASS`, `GLASS_CARD_CLASS`, `BARE_FIELD_CLASS` (a field undressed inside a row), `avatar-color`, `label-colors`, `board-icons`, `session-dot`, `status-icons`, `icons.generated` (the committed `@exp/icons` output), `useIsMobile`, `useSheetDrag` |

`src/icons.generated.ts` is written by `bun run --filter @exp/icons generate` —
never hand-edit it.

## Entry points

```jsonc
"." : "./src/index.ts"      // the barrel — the ONLY way in
"./styles.css"              // the theme, imported by the app's own stylesheet
"./island"                  // src/island.ts
```

Inside the package every import is **relative**. The `@/*` path in
`tsconfig.json` exists only so the shadcn CLI can write files; nothing resolves
through it at runtime.

That also means an app test's `vi.mock("@exp/ui", …)` overrides only what the
APP imports from the barrel: a package component that reads `./use-mobile`
relatively keeps the real hook. Mock the surface you assert on, not the
package's internals.

## Adding a component

```bash
cd packages/ui && bunx shadcn@latest add <name>
```

Then:

1. rewrite the generated `@/…` imports to relative ones (`./cn`, `./button`, …);
2. add `export * from "./<name>"` to `src/index.ts`;
3. if it needs a class recipe another surface already has, reuse the constant
   (`MENU_SURFACE_CLASS`, `GLASS_CARD_CLASS`, the `SEGMENTED_*` set in
   `tabs.tsx`) instead of copying the string; a tinted 48px glyph circle is
   `IconDisc`, never a hand-rolled `rounded-full bg-primary/10`.

## Islands

An **island** is one component's RESTING state rendered to static markup and
dropped into a page that does not run the web app: the styleguide's Components
group today (EXP-887), the marketing docs next. A lookalike hand-written in
HTML/CSS can disagree with the product silently — the component cannot disagree
with itself.

`@exp/ui/island` is its own entry point because it pulls `react-dom/server` and
the Tailwind compiler; nothing in the barrel does.

```tsx
import {
  ISLAND_CLIENT_SCRIPT, compileUiCss, renderIsland, renderIslandCssTemplate,
} from "@exp/ui/island"

const css = await compileUiCss({ base: import.meta.dir })   // ONCE per page, ~1s
head += renderIslandCssTemplate(css)                        // ONCE, in <head>
body += renderIsland(<IssueChip identifier="EXP-887" … />)  // once per specimen
body += `<script>${ISLAND_CLIENT_SCRIPT}</script>`          // ONCE, end of <body>
```

| | |
| --- | --- |
| `compileUiCss({ base, sources? })` | The package stylesheet compiled for ONE host page, minified. `base` is the CALLER's directory, so its own fixtures are scanned; the package's own `@source` pulls in every component it renders. Throws if the output contains `</style`. |
| `renderIsland(element)` | `<div data-ui-island><template shadowrootmode="open"><div class="${ISLAND_ROOT_CLASS}">…</div></template></div>` |
| `renderIslandCssTemplate(css)` | The inert `<template id="ui-css">` carrier. |
| `ISLAND_CLIENT_SCRIPT` | Builds ONE `CSSStyleSheet` from the carrier, adopts it into every island's shadow root, and polyfills `attachShadow` where declarative shadow DOM did not run. |
| `ISLAND_ROOT_CLASS`, `ISLAND_CSS_TEMPLATE_ID` | The wrapper class and the carrier id, for hosts that assert on them. |

The **shadow root** is what keeps the web theme (`--border`, `--accent`,
preflight …) from colliding with the host page's own CSS — marketing has
hand-written tokens of the same names. Tailwind v4 writes `@theme` vars to
`:root, :host` and preflight to `html, :host`, and this package's `:root` token
block is written `:root, :host` for the same reason, so both reach the tree.

### Limits

- **Resting state only.** `renderToStaticMarkup` runs no effects and no events,
  so a Radix **portal** — an open menu, dialog, popover, sheet, `SelectContent`
  — renders NOTHING. Pick a fixture whose resting state is the specimen (a
  closed `IconPicker` IS its trigger), or keep that entry hand-written.
- **`AvatarImage` never renders**: Radix only swaps the image in once the
  browser has decoded it, so avatar fixtures use initials.
- **A `Select` with a value shows an empty trigger** — `SelectValue` resolves
  against items that only exist inside the (portalled) content. Pass
  `renderValue`, or use the placeholder arm.
- **The host paints the ground.** `ISLAND_ROOT_CLASS` carries no background:
  `bg-app-gradient` is the app body's fixed `::before` pair, and a fixed layer
  inside a shadow tree is wrong.
- **Fonts and rem come from the HOST.** Inter is not loaded in the gallery
  (`font-sans` falls back), and the package's `html { font-size }` rule matches
  nothing inside a shadow tree — the host page has to mirror it or every island
  renders at the wrong scale.
- **Utilities are scanned, not guessed.** A class a fixture builds at runtime is
  not in the compiled CSS; write literals.

### Recipe

1. render the component with a fixture — no callbacks that matter, no live data;
2. `compileUiCss` once per page with `base` = the directory holding the fixtures;
3. carrier in `<head>`, script at the end of `<body>`, one `renderIsland` per
   specimen.

## Kept deliberately separate

Duplication that looks removable and is not:

- **`dialog` vs `sheet` vs `alert-dialog`** — three different Radix roots with
  different a11y semantics (`alertdialog` traps and has no dismiss). `dialog`
  already absorbs the mobile bottom-sheet arm; the standalone `sheet` is the
  side/bottom panel primitive.
- **`dropdown-menu` vs `context-menu`** — again different Radix roots (one is
  pointer-anchored, the other trigger-anchored). They share what they actually
  share: `MENU_SURFACE_CLASS`.
- **the `SEGMENTED_*` class constants in `tabs.tsx`** — already the single
  source `GlassTabsRow` reuses; they are constants, not a second component.
- **`hover-card`** — one importer today (the issue preview), and the preview
  needs its open/close delays; folding it into `popover` would lose them.
- **`Button size="xs"`** — 5 of its 8 call sites are destructive or row-shaped
  ghost buttons that no `Pill` mode covers.
