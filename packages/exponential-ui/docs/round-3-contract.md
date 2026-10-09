# Exponential UI — round 3 contract (VAPP-102)

Round 3 folds the catalog onto the vocabulary the Exponential app settled on
in its UI cleanup (EXP-1248, EXP-1249, EXP-1251): ONE row, ONE section band,
ONE chip, ONE segmented control, ONE menu. The catalog had split each of
these several ways; the app has one of each. This file is the CONTRACT half:
what changed in `@exponential-at/ui` and what each renderer implements to
match. Everything here is locked by a fixture or a test.

Read §0, the sections you own, then §6 (your checklist).

## 0. What moved

| area | change | source of truth | locked by |
|---|---|---|---|
| `Row` | NEW macro: leading/trailing slots, identifier, title/subtitle/meta/value(+placeholder), chevron, selected, density, surface flat\|card, pressable, depth | `core.catalog.json`, `macros.json` | `catalog-macros.json` |
| `Section` | NEW macro: a band header (title, caption, count OFF by default, glyph, leading/trailing slots, collapsible/open, tint) over hairline-divided rows (a `List` native, the windowed backend); `tree` = the rows nest by depth | same | `catalog-macros.json`, `kitchen-sink.expanded.json` |
| tree guides | computed by the CORE from consecutive rows' `depth` (elbow, tee, pass-through); `TreeGuides` = a hidden Row part, never authored; column 14 px, rounded 3 px elbow, 1 px bridge | `src/tree-guides.ts`, `layout.json` | `fixtures/tree-guides.json`, the `Row/tree:*` macro cases |
| `Chip` | NEW macro: shape pill\|rect (Pill + EntityChip), selected, tone, dot, icon, image, detail, removable, pressable, a `leading` slot | `macros.json` | `catalog-macros.json` |
| `Segmented` | native (ToggleGroup renamed): variant segmented (default) \| toggles \| outline \| bar; TabBar + ButtonGroup fold in | `core.catalog.json`, `recipes.json` | `catalog-components.json`, `control-geometry.json` |
| `Menu` | native (DropdownMenu + ContextMenu): `openOn press\|contextmenu`, the ONE child = the trigger or the target region, `menuItem.items` bindable (a source-fed submenu) | `core.catalog.json` | `catalog-components.json`, `bind-time.json` |
| `Badge` | counts and short status labels only | `core.catalog.json` | — |
| `Sheet` | the alias is `Drawer side: bottom` (what the app calls a sheet) | `macros.json` | `catalog-macros.json` |
| deprecated aliases | 16 components carry `deprecated: "<Replacement>"`: still reduce (a one-node macro over the replacement), never offered | `core.catalog.json` `$comment` | `catalog.test.ts`, every `<Alias>/example` case |
| lite | Pagination, Accordion, Radio, Slider, Spinner, Table, Toggle leave the lite subset (with Carousel, the overlays, media, Chart) | `core.catalog.json` | `prompt-budget.json` |
| a11y | roles gain `tree`, `treeitem` | `a11y.json` | `round1-fixes.test.ts` |
| extension | the app extension splits RunRow into RunStatusRow + SessionRow, adds PrRow/StackRail/DiffCounts/DiffFileRow/GuideSection; IssueRow, IssueGroupBand and IssueChip are macros over Row/Section/Chip | `packages/ui/exponential-ui/extension.json` | `packages/ui/src/exponential-ui-extension.test.ts` |

Counts: 88 components in the catalog, 70 offered (46 natives, 42 macros
including the 16 aliases; 50 lite), TreeGuides hidden.

## 1. Row and Section

```json
{ "id": "runs", "component": "Section", "props": { "title": "Running", "tree": true, "collapsible": true },
  "children": ["r1", "r2", "r3"] }
{ "id": "r1", "component": "Row", "props": { "identifier": "EXP-12", "title": "Release 0.18", "meta": "4 runs", "pressable": true } }
{ "id": "r2", "component": "Row", "props": { "title": "Notarize", "depth": 1, "icon": "status-in-progress" } }
{ "id": "r3", "component": "Row", "props": { "title": "Upload", "depth": 1, "value": "queued", "chevron": "right", "pressable": true } }
```

**Row** parts, in order: `root` (a pressable Box: flex row, gap sm, min
height `$control.row` / `$control.rowCompact` by `density`, padding
horizontal md and NO vertical padding (the `body` carries xs (flat) / md
(card) so the `guides` part spans the row's full height and the lines of
consecutive rows meet across the divider), radius md (flat) / lg (card))
→ `guides` (a `TreeGuides` native, only when `depth > 0`, FILLED BY THE
CORE) → the `leading` slot → `icon` (sm, when no slot is given) →
`identifier` (Text `code`) → `body` (`title` Text body, `subtitle` Text
muted) → `meta` (Text muted) → `value` (Text body, or muted while it shows
`placeholder`, aligned end) → the `trailing` slot → `chevron` (`right` =
`ui-chevron-right`, `selector` = `ui-selector`). Recipe props: `selected`,
`density`, `surface`, `depth`. A11y: role `button` + `selected` when
`pressable`, otherwise `none`. `on.press` implies `pressable`.

**Section** parts: `root` (column, gap xs) → `header` (only with a `title`;
flex row, height `$control.row`, padding horizontal md, radius md; a heading
(level 3), or a `button` with `expanded` when `collapsible`, whose press
routes to `change {open}` and writes a bound `open` back) → `chevron`
(collapsible only, `ui-chevron-right`, rotates when open) → the `leading`
slot → `icon` → `title` (Text label) → `caption` (Text muted) → `count`
(Text muted, only when given) → `spacer` → the `trailing` slot; then `body`
= a `List` native (`divided: true`, `gap: none`, role `list`, or `tree`
when `tree`) holding the author's children (a data template rides through),
shown while `open` (default true). Recipe props: `tint`, `tree`, `open`,
`collapsible`. The old `Band` = a Section without rows; the old `RowList` =
a Section without a title.

## 2. The tree guides the core computes

`treeGuides(depths)` (`src/tree-guides.ts`; Rust `tree_guides.rs`) is the
app's ×4 rule verbatim: a row at depth `d ≥ 1` gets its elbow in gutter
level `d − 1`; the elbow TEES on when another row at depth `d` follows
before the walk leaves the subtree (a row at depth `≤ d − 1`); every
ancestor level `L < d − 1` whose subtree continues after the row passes
straight through. `fixtures/tree-guides.json` locks the vectors.

`applyTreeGuides(root)` runs ONCE at the end of `expandMacros`
(`expand_macros`), when every node is native: over every node's children in
order, a Row root (`recipe.macro` Row, part root) contributes its numeric
`recipe.props.depth` (floored; anything else, including a non-Row sibling,
counts as 0 and ends every subtree); the Row's `guides` part gets
`{depth, elbowAt?, tee, passThrough}` in that key order (`elbowAt` omitted
for a root). A `depth` is static (never bindable). Template items get their
guides from the instantiated siblings the same way at layout time only where
a renderer instantiates them into the expanded tree (the Rust core); a
React data template today draws roots only.

**Geometry** (`layout.json`): `treeGuideColumn` = 14 (the `guides` part is
`depth × 14` wide, stretched to the row; column `i`'s line is at
`x = i·14 + 7`, the elbow's stub runs to `i·14 + 14` so it ends at the
child's own lead glyph), `treeGuideRadius` = 3 (the elbow's corner),
`treeGuideBridge` = 1 (every guide vertical starts that many px ABOVE the
part's top edge, paint only, so a Section's hairline divider never breaks the
line). Stroke width and colour = the `TreeGuides/line` recipe.

## 3. Chip, Segmented, Menu

**Chip** `shape: pill` = radius full, height `$control.pill` (the old
Pill); `rect` = radius md, height `$control.chip` (the old EntityChip).
Parts: root → the `leading` slot → dot → image (16 px, round) → icon (when
no image) → label (caption) → detail (muted) → remove (a ghost icon Button,
`$on press → remove`). Recipe props `shape`, `selected`, `tone`.

**Segmented** keeps ToggleGroup's contract (`items`, `type`, bound `value`
comma-joined when multiple, `size`, `fill`) with `variant`: `segmented` (the
DEFAULT: the pill track), `toggles` (bare toggle buttons, ToggleGroup's old
default), `outline` (joined outline buttons, the old ButtonGroup), `bar` (the
old TabBar: full width, each item a column of icon over a caption label,
role `navigation` with `aria-current=page` on the selected item). Recipe
parts `root`, `item`, `icon`, `label`; `when` props `type`, `variant`, `size`,
`fill`. Content-sized (align-self start) unless `fill` or `bar`.

**Menu** keeps the `menuItem` contract (item | checkbox | separator | label |
submenu, one level) with `openOn`: `press` (the ONE child is the trigger; no
child = a default outline Button from `label`/`icon`, no chevron) or
`contextmenu` (the child is the target region: right-click, long-press,
Shift+F10 or the Menu key open it at the pointer). `menuItem.items` is
bindable: a `{path}` whose rows a host feeds (`bindDataModel` from an
`exp:` source) becomes the submenu. Parts `trigger`, `content`, `item`,
`separator`, `label`, `shortcut`, `check`, `submenuIndicator`; built-in
glyphs `Menu.check`, `Menu.submenuIndicator`. The DropdownMenu and
ContextMenu glyph rows are gone (their owners are aliases now).

## 4. Deprecated aliases (one release)

| alias | expands to |
|---|---|
| ListRow, CardRow, PropertyRow, PickerRow, NavRow | `Row` (card = `surface: card`; property = a `value`; picker = `value` + `placeholder` + `chevron: selector`; nav = `icon` + `selected` + a `Badge` in the trailing slot, role link) |
| Band, RowList | `Section` (no rows / no title; `guides` → `tree`) |
| Pill, EntityChip | `Chip` (pill / rect) |
| ToggleGroup, TabBar, ButtonGroup | `Segmented` (`toggles`\|`outline`\|`segmented` / `bar` + fill / `outline`) |
| DropdownMenu, ContextMenu | `Menu` (`press`, the trigger slot becomes the child / `contextmenu`) |
| Sheet | `Drawer side: bottom` |
| HoverCard | `Popover openOn: hover` |

An alias is `kind: macro`, `lite: false`, never in the prompt, the docs,
the specimens or `componentNames()`; `catalog-components.json` keeps ONE
case per alias (`<Alias>/example`) so every renderer proves it still
expands. The generated constants carry `componentDeprecated` (the
replacement, `""` for a live component); `docs/components.generated.json`
carries the `deprecated` map. Over a MACRO replacement the alias is
transparent (the expanded root's recipe names the replacement); over a
NATIVE one (Segmented, Menu, Drawer, Popover) the root keeps the alias as
its recipe macro.

## 5. Breaking changes

- Themes: recipes for the folded components fail to load (`unknown
  component`); move them to `Row`, `Section`, `Chip`, `Segmented`, `Menu`.
- `layout.json` `treeGuideColumn` 16 → 14; `TreeGuides` is hidden.
- `builtinIcons`: `DropdownMenu.*` / `ContextMenu.*` → `Menu.*`.
- Enums `bandVariant`, `toggleVariant` are gone; `rowChevron`, `rowSurface`,
  `chipShape`, `segmentedVariant`, `menuOpenOn` are new.
- The lite subset lost seven components; `prompt-budget.json` moved.

## 6. Per-renderer checklist

Every renderer: replay the regenerated fixtures (`catalog-components.json`
with the alias cases, `catalog-macros.json` with the `Row/tree:*` cases,
`tree-guides.json`, `kitchen-sink*.json`, `control-geometry.json`,
`theme-*.json`, `bind-time.json`); paint `Segmented` (+ `bar`) and `Menu`
(+ `contextmenu`) under the new names; draw `TreeGuides` at 14 / 3 / 1.

- **Rust core**: `tree_guides.rs` + the post-expansion pass; `Menu` and
  `Segmented` in the layout tree, events and state; `deprecated` on the def.
- **gpui**: the menu and segmented painters renamed, the `bar` column item,
  the rounded + bridged guides; the conformance ratchet re-recorded against
  the new web baseline.
- **React**: `MenuNative`, `SegmentedNative`, the guides geometry, the
  snapshot and the web baseline re-recorded.
- **SwiftUI / Compose**: the leaf switches renamed (`Menu`, `Segmented`),
  the `bar` item, the guides geometry, the built-in glyph mirror, snapshots
  re-recorded (Compose on ubuntu).
