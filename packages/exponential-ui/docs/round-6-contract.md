# Exponential UI — round 6 contract (EXP-1495)

Round 6 adds the low-level pieces THE plan page (#EXP-1493,
`packages/domain-contract/fixtures/plan-page.json`) needs: an anchor list, a
wide picture group, and a counter + thumbnail strip on the Carousel. This
file is the CONTRACT half: what changed in `@exponential_app/eui` and what
each renderer implements to match. Everything here is locked by a fixture
or a test.

## 0. What moved

| area | change | source of truth | locked by |
|---|---|---|---|
| `Outline` | NEW macro (navigation, lite): `items` (`outlineItem` {id, title, count}), `value` (the active id, bindable, two-way), `caption`; event `select {id}` | `core.catalog.json`, `macros.json`, neutral recipes | `catalog-macros.json`, `bind-time.json` |
| `Mosaic` | NEW macro (media, not lite): `pictures` (`picture` {src, alt}), `label`; event `select {index}` | same | `catalog-macros.json` |
| `Carousel` | `counter` (boolean) + `thumbnails` (array of url); parts `counter`, `thumbnails`, `thumbnail` (state `selected`), recipe prop `counter` | `core.catalog.json`, `recipes.json`, neutral recipes | `catalog-components.json`, `theme-recipes.json`, the kitchen sink |
| strings | `morePictures` (`+{count}`), `morePicturesLabel` (`{alt}, {count} more`), `outline`, `outlineRow` (`{title}, {count}`), `pageCounter` (`{page} / {total}`) | `strings.json` | generated STRING tables |
| kitchen sink | + Outline (after the Breadcrumb), Mosaic (4 pictures, after the Carousel); the Carousel gains `counter: true` + two thumbnails | `kitchen-sink.json` | `kitchen-sink.expanded.json` |
| web primitives | `OUTLINE_CLASSES`/`OUTLINE_GEOMETRY`/`outlineCountText`, `MOSAIC_CLASSES`/`MOSAIC_GEOMETRY`/`mosaicTiles`, `CAROUSEL_CLASSES`/`CAROUSEL_GEOMETRY`/`carouselCounterText` | `src/web/primitives/{outline,mosaic,carousel}.ts` | `recipes.test.ts` (no class literal in the React/Solid twins) |

Counts: 89 offered components (49 natives, 40 macros; 62 lite),
+ the hidden TreeGuides and Unknown. Prompt budget 7,900 → 8,034 tokens
(a named exception: the measured total, no slack).

## 1. Outline

THE plan page's left column (plan-page.json `layout.outlineRowHeight` 32,
`outlineActiveBar` 2). Parts: `root` (column, gap xxs; role navigation
labelled `caption`, else `$string.outline`) → `caption` (Text caption, one
line, hidden from AT; neutral: 11/16 medium, letter spacing 0.5, UPPERCASE,
muted, padded sm × xs) → per item `row` (a pressable Box, row, centred,
gap xs, `$control.rowCompact` (32) tall, padding-x sm, relative, clipping;
role link labelled `title` (+ `, count` via `$string.outlineRow`),
`current: location` on the active row; `$recipe` `active` = `item.id ==
value`; press → `select {id}`, and with a bound `value` the `$set` writes
the id back) → `bar` (only on the active row: absolute, inset-inline-start 0,
top/bottom 0, 2 px wide; neutral: foreground) → `title` (Text, one line,
flex 1; neutral: sm muted, active = foreground medium) → `count` (only when
`count` > 0: caption `· n`, muted, hidden from AT). Neutral row: radius sm,
hover = accent, active = accent. No `value` (or none matching) = no active
row. Scroll-spy stays in the HOST: it binds `value` to the section in view.

## 2. Mosaic

THE wide picture group (plan-page.json `mosaicTiles` + `mosaic`). No
`$each`: the macro addresses `props.pictures.0/1/2` directly (the TS and Rust
`lookup` index arrays by a numeric segment). Parts: `root` (row, full width,
`aspectRatio` 1.6; role group labelled `label`; neutral gap 10) →
`lead` (picture 0: 1 picture = the whole group, 2 = a half (flex 1), 3+ =
`flexBasis` 60 %, no grow/shrink; `$recipe size` large|half) → `pair`
(picture 1, ONLY when exactly 2: flex 1, `size` half) → `side` (3+ only:
column, flex 1, neutral gap 10) → `top` (picture 1) + `bottom` (picture 2,
both flex 1, `size` small) → `more` (on `bottom` when 4+: absolute inset 0,
centred; neutral fill `#0000008c` = black 0.55) → `moreLabel` (Text
`$string.morePictures` = `+{count}`, count = n − 3; neutral white xl
semibold). Every tile = a pressable Box (relative, clipping, min 0; role
button labelled the picture's `alt`, the `+N` tile `$string.morePicturesLabel`)
holding `image` (Image `fit: cover`, absolute inset 0, 100 % × 100 %, hidden
from AT); press → `select {index}` (0, 1 or 2; the host opens its lightbox
over every picture). Neutral tile: radius lg (10), hairline border, muted
fill under the picture, hover opacity 0.92. An empty `pictures` = an empty
group box (hosts draw none).

## 3. Carousel `counter` and `thumbnails`

Both only with TWO OR MORE pages (like the controls row).

- `counter: true` = a pill over the TRACK's top-END corner (mirrored under
  RTL = top-left), inset `$spacing.sm` (8) from the top and the end edge,
  above the pages (the end corner keeps it off the page's own heading, which
  starts at the top-start corner, like a media gallery's counter), never
  intercepting a press or a swipe: part `counter`, text
  `$string.pageCounter` {page: current + 1, total: n}, hidden from AT (each
  page is already `page i of n`). Neutral: height 20, padding-x sm, radius
  full, fill `#0000008c`, text `#ffffff` xs medium, tabular figures.
- `thumbnails: [url…]` = a strip under the track, BEFORE the controls row:
  part `thumbnails` (row, neutral gap sm, start-aligned, scrolling sideways
  when it overflows; the current thumbnail scrolled into view), one
  `thumbnail` per PAGE (URLs past the page count are ignored; a missing one
  = the empty muted square): a button 52 × 52 (neutral: radius md, a 2 px
  border, transparent → `$color.foreground` when `selected` = the current
  page, muted fill, opacity 0.7 → 1 when selected or hovered), the picture
  covering it inside the border; a press goes to that page (fires `change
  {page}` like a dot). A11y: each a button `$string.pageOf`, the current
  one `current`/`aria-current`.

The Rust core lays both out (`layout_tree` containers: the counter is an
overlay box of the track, out of flow; the strip a row of fixed 52 × 52
items between the track and the controls), so FFI/gpui painters read the
frames; the web renderers lay out with CSS.

## 4. Per-renderer checklist

Every renderer: replay the regenerated `catalog-components.json`,
`catalog-macros.json`, `bind-time.json`, `theme-recipes.json`,
`kitchen-sink*.json`; paint the two new specimens (`exponential-ui-outline`,
`exponential-ui-mosaic`) and the Carousel extras; re-record what the grown
kitchen sink moves (baselines, snapshots, bench node counts).

- **Macros (Outline, Mosaic)**: nothing to paint beyond Box/Text/Image;
  check the Box `aspectRatio`, `position: absolute` + `inset` and
  `insetInlineStart` paths, Image `fit: cover` inside a sized box.
- **React / Solid / element**: Carousel counter + strip (CSS), the
  `selected` thumbnail recipe, `change` on a thumbnail press; re-record
  `fixtures/conformance-baseline.json`; the primitive twins `Outline`,
  `Mosaic` (+ the carousel extras) on `@exponential_app/eui/web/primitives`.
- **Rust core / FFI**: the generated catalog, the Carousel counter + strip
  frames in `layout_tree`, the macro fixtures.
- **gpui / SwiftUI / Compose**: paint the counter + strip from the core's
  frames and recipes; thumbnail press → `change`; snapshots re-recorded.
