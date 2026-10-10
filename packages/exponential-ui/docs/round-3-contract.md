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
| tree guides | computed by the CORE from consecutive rows' `depth` (elbow, tee, pass-through); `TreeGuides` = a hidden Row part, never authored; column 14 px (the line's left edge at i·14 + 7), rounded 3 px elbow, 1 px bridge | `src/tree-guides.ts`, `layout.json` | `fixtures/tree-guides.json`, the `Row/tree:*` macro cases |
| `Chip` | NEW macro: shape pill\|rect (the old Pill + EntityChip), selected, tone, dot, icon, image, detail, removable, pressable, a `leading` slot | `macros.json` | `catalog-macros.json` |
| `Segmented` | native (ToggleGroup renamed): variant segmented (default) \| toggles \| outline \| bar; TabBar + ButtonGroup fold in | `core.catalog.json`, `recipes.json` | `catalog-components.json`, `control-geometry.json` |
| `Menu` | native (DropdownMenu + ContextMenu): `openOn press\|contextmenu`, the ONE child = the trigger or the target region, `menuItem.items` bindable (a source-fed submenu) | `core.catalog.json` | `catalog-components.json`, the React + Rust bound-submenu tests |
| `Badge` | counts and short status labels only | `core.catalog.json` | — |
| sheets | `Drawer side: bottom` (what the app calls a sheet; the Sheet name is gone) | `core.catalog.json` | `catalog-components.json` |
| removed names | round 4 (VAPP-103) REMOVED the 16 folded names (§4): no aliases, an old name is an `unknown component` | `core.catalog.json` `$comment` | `catalog.test.ts` |
| lite | Pagination, Accordion, Radio, Slider, Spinner, Table, Toggle leave the lite subset (with Carousel, the overlays, media, Chart) | `core.catalog.json` | `prompt-budget.json` |
| a11y | roles gain `tree`, `treeitem` | `a11y.json` | `round1-fixes.test.ts` |
| extension | the app extension splits RunRow into RunStatusRow + SessionRow, adds PrRow/StackRail/DiffCounts/DiffFileRow/GuideSection; IssueRow, IssueGroupBand and IssueChip are macros over Row/Section/Chip | `packages/ui/exponential-ui/extension.json` | `packages/ui/src/exponential-ui-extension.test.ts` |

Counts (after round 4): 72 components in the catalog, 70 offered (44
natives, 26 macros; 50 lite), TreeGuides and Unknown hidden.

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
for a root). A lone root or slot value is a one-element list. Any OTHER
sibling with a numeric `depth` prop (an extension row such as SessionRow or
PrRow) takes part too and gets the result as its `guide` prop
(`{elbowAt?, tee, passThrough}`) for its painter to draw. A `depth` is
static (never bindable). The pass runs over the EXPANDED tree only: rows a
data template instantiates (a List over `{path}`) are not siblings at
expansion time, and a painter draws NO elbow without `elbowAt`; author
static rows for a connected tree. A macro that offers `pressable` (Row,
Chip) takes it from an `on.press` when unset.

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
ContextMenu glyph rows are gone (round 4 removed those names).

## 4. Removed names (round 4)

Round 3 shipped these as one-release aliases; round 4 (VAPP-103) REMOVED
them before any release: there is no `deprecated` key, no alias macro, no
example case and no generated `componentDeprecated` constant. An old name
reduces to the `Unknown` placeholder with an `unknown component <name>`
issue, and a theme naming one fails to load (`unknown component`).

| removed | write instead |
|---|---|
| ListRow, CardRow, PropertyRow, PickerRow, NavRow | `Row` (card = `surface: card`; property = a `value`; picker = `value` + `placeholder` + `chevron: selector`; nav = `icon` + `selected` + a `Badge` in the trailing slot) |
| Band, RowList | `Section` (no rows / no title; `tree` for nested rows) |
| Pill, EntityChip | `Chip` (`shape: pill` / `rect`) |
| ToggleGroup, TabBar, ButtonGroup | `Segmented` (`toggles`\|`outline`\|`segmented` / `bar` + fill / `outline`) |
| DropdownMenu, ContextMenu | `Menu` (`openOn: press` with the trigger as the child / `contextmenu`) |
| Sheet | `Drawer side: bottom` |
| HoverCard | `Popover openOn: hover` |

## 5. Breaking changes

- Themes: recipes for the folded components fail to load (`unknown
  component`); move them to `Row`, `Section`, `Chip`, `Segmented`, `Menu`.
- `layout.json` `treeGuideColumn` 16 → 14; `TreeGuides` is hidden.
- `builtinIcons`: `DropdownMenu.*` / `ContextMenu.*` → `Menu.*`.
- Enums `bandVariant`, `toggleVariant` are gone; `rowChevron`, `rowSurface`,
  `chipShape`, `segmentedVariant`, `menuOpenOn` are new.
- The lite subset lost seven components; `prompt-budget.json` moved.

## 6. Per-renderer checklist

Every renderer: replay the regenerated fixtures (`catalog-components.json`, `catalog-macros.json` with the `Row/tree:*` cases,
`tree-guides.json`, `kitchen-sink*.json`, `control-geometry.json`,
`theme-*.json`, `bind-time.json`); paint `Segmented` (+ `bar`) and `Menu`
(+ `contextmenu`) under the new names; draw `TreeGuides` at 14 / 3 / 1.

- **Rust core**: `tree_guides.rs` + the post-expansion pass; `Menu` and
  `Segmented` in the layout tree, events and state.
- **gpui**: the menu and segmented painters renamed, the `bar` column item,
  the rounded + bridged guides; the conformance ratchet re-recorded against
  the new web baseline.
- **React**: `MenuNative`, `SegmentedNative`, the guides geometry, the
  snapshot and the web baseline re-recorded.
- **SwiftUI / Compose**: the leaf switches renamed (`Menu`, `Segmented`),
  the `bar` item, the guides geometry, the built-in glyph mirror, snapshots
  re-recorded (Compose on ubuntu).
