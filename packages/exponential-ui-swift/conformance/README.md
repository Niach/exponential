# SwiftUI real-font conformance

The SwiftUI half of the renderer conformance harness
([`packages/exponential-ui/conformance`](../../exponential-ui/conformance/README.md),
round 1): every case of `fixtures/conformance-cases.json` (2 fixtures × 3
themes × dark × widths 390/600/900/1280 × ltr/rtl = 48 cases) laid out by
`SurfaceModel` with the painter's REAL measurer (CoreText) and compared with
the committed web baseline (`fixtures/conformance-baseline.json`, React in
headless Chromium with the same fonts).

| file | what |
|---|---|
| `Tests/ExponentialUITests/RealFontConformanceTests.swift` | the dump producer, the comparator port and the ratchet |
| `conformance/conformance-known-swift.json` | the budget: per case, today's divergence counts |
| `Sources/ExponentialUI/Measure/TextMeasurer.swift` | `TextFonts` (CSS font matching, font sets) + `TextShaper` (the web's text rules) |

## Run it

```bash
cd packages/exponential-ui-swift
swift test --filter RealFont                                  # the ratchet + the shaper rules
EXPONENTIAL_UI_CONFORMANCE_DUMP=/tmp/swift.json swift test --filter RealFont   # also write the dump
EXPONENTIAL_UI_CONFORMANCE_ONLY=responsive/neutral swift test --filter RealFont # a subset (substring of the key)
EXPONENTIAL_UI_CONFORMANCE_VERBOSE=1 swift test --filter RealFont               # every cascaded node too
EXPONENTIAL_UI_WRITE_FIXTURES=1 swift test --filter RealFont  # rewrite the budget after a fix
```

The test prints, per case, the ORIGINS (`FIX`: a node whose own size diverges
with nothing below explaining it, or that moved while its parent did not),
the count of the cascade they cause, tolerated rewraps and the coverage
(`only in web` / `only in swift`), then the origins grouped across cases: the
list to fix. A case whose counts exceed the budget fails; one below it asks
for the budget to be lowered. It also writes the `real-font` suite summary
(`{renderer, platform, suites: {"real-font": {cases, passed, failed}}}`) to
`<repo>/.conformance/exponential-ui-swift-real-font.json`
(`EXPONENTIAL_UI_REAL_FONT_REPORT` overrides), the shape `conformance:check`
reads once the manifest gains the suite.

## Compare a dump with the TS comparator

The dump is the shared FRAME DUMP format (`conformance/dump.ts`,
`xui-frame-dump/1`, renderer `swiftui-coretext`), so the harness's own tools
read it. `run.ts` treats any reused `--desktop` dump as the candidate:

```bash
EXPONENTIAL_UI_CONFORMANCE_DUMP=/tmp/swift.json swift test --filter RealFont   # in packages/exponential-ui-swift
bun packages/exponential-ui/conformance/run.ts --skip-web --desktop /tmp/swift.json [--details] [--only <substring>]
```

(The table is headed "desktop": it is the Swift dump.) From code:
`compareDumps(decodeBaseline(baseline), swiftDump, manifest.tolerance)` and
`formatTable` / `groupFindings` of `conformance/compare.ts`. The Swift port
of `compareCase` gives the same counts (checked: 1436 size, 6436 position,
958 origins, 96 only-web, 2640 only-swift over the matrix).

## What the producer does (= gpui's `examples/conformance_dump.rs`)

- **Fonts**: exactly the files of `conformance/fonts.json`, registered with
  CoreText for the process and installed as an EXCLUSIVE `TextFonts.Set`:
  families with faces get their files, a `substitute` (`Geist Mono`,
  `ui-monospace`, `Nunito`, `Fira Code`) the substitute's files under its own
  name, the system font and every unmapped family the `default` (Inter).
  Nothing else (a system-installed JetBrains Mono or Inter is never used).
  Faces are picked with CSS font matching (`TextFonts.match`, CSS Fonts 4
  §5.2), as Chromium does: Geist 500 → the 400 face, never CoreText's
  nearest-trait 600.
- **Input** (`caseInput`): a geometry fixture's `surface` + `data`, else the
  tree + its data file (`$comment` dropped); the root's `style.direction` =
  the case's direction. Data goes in key by key (`/<key>`).
- **Surface**: the case's built-in theme and mode, settings `locale` =
  the manifest's (`en-US`) and `mode`, width = the case width, viewport
  height 0 (UNKNOWN to the core's conditions, as gpui's view passes it:
  `orientation` = landscape), two passes.
- **Nodes**: the main tree (layer 0) pre-order through `children`, removed
  and hidden nodes skipped, frames relative to the root, rounded to 1/100 px,
  `parent` = the parent's id, `text` = the first of `text|label|title|value`
  (numbers/booleans as JS prints them, whitespace collapsed, ≤ 80 chars),
  `lines`/`lh` for the manifest's `textComponents` (frame height minus the
  vertical padding + border, ÷ the line height), then `dropCollapsed`.

## The text rules (TextShaper)

Widths are CoreText typographic advances (kerning, ligatures), ceiled to
Chromium's 1/64 px layout unit, never to whole px. Break opportunities are
UAX #14 (`CFStringTokenizer` line-break unit) minus breaks after a `/` a word
follows; trailing spaces hang; wrapping is greedy with gpui's 0.5 px slack;
max-content = the widest hard line, min-content = the widest segment; `n`
lines = `n × lineHeight`; an empty (all-space) text is 0×0 (no line box in
CSS). `TextShaper.lines(text, ts, wrap:)` returns the lines a painter should
draw verbatim; `TextShaper.baseline(ts)` = half-leading + ascent.

With these, every text node whose box is the text's own matches Chromium
within 1 px across the matrix; the remaining text-component divergences are
cascades (a sibling's width in a flex row: badges, list metas).

## Today (the budget as recorded on the VAPP-100 L4 branch)

48 cases: size 1436 · position 6436 · wrap 0 · origins 958 · only web 96 ·
only swift 2640. `responsive/*` = size 1 (≤ 900 px) / size 3, position 1
(≥ 900 px), the same as gpui's budget. `kitchen-sink/*` ≈ 49–66 size,
240–310 position (gpui: 55–71 / 239–274), most of them the natives the
round-1 painter work (L3) has not landed yet. Origins to fix, grouped:

| origin | cases | diagnosis | owner |
|---|---|---|---|
| `resp-drawer` (Drawer) width = its trigger (99 px), web stretches it to the column | 48 | the Drawer's host box is not stretched (gpui has it too) | core |
| `audio` (AudioPlayer) 64 vs 78 tall | 24 | measurer height rule (title row + controls) | L3 `SurfaceMeasurer` |
| `chart`, `chart-line`, `chart-donut` 160/200/180 vs 208/224/224 | 24 each | round-1 chart chrome (axis/legend rows) not in the measure | L3 |
| `code` (CodeBlock) | 24 | new native, not measured yet | L3 |
| `form-due` (DatePicker) 55 vs 134 wide | 24 | the `trigger` part is not measured/painted (control-geometry fails the same) | L3 |
| `form-plan` (Radio) 44 vs 48, `form-volume` (Slider) 30 vs 40 | 24 each | control rules (row gap / thumb + label row) | L3 |
| `hdr-badge`, `nav-inbox.count`, `resp-nav-inbox.count` (Box) 32 vs 23.5 wide | 24 each | the count badge gets a 32 px minimum the web does not apply (its text, 7.53 px, matches exactly; gpui has it too) | core / recipe |
| `list-item.meta.*` (Text) 0 wide / not placed | 24 | the `text` prop is a NUMBER (`412`): the measurer reads `props.str("text")` (empty) instead of the core's `FfiLeaf.text` | L3 |
| `menu` (DropdownMenu) 81 vs 101 wide | 24 | the trigger's chevron chrome (+20) is missing | L3 |
| `toggles` (ToggleGroup) full width vs 132 | 24 | the group is not sized to its items | L3 / core |
| `tree-guides` 32×20 vs 2×16, `sparkline` 96 vs 57 wide, `table` rows, `nf-files`, `page-1`, `video` | 24 each | measurer defaults for round-1 natives | L3 |
| `main-md` (Markdown) 88 vs 68 tall at ≥ 900 | 12 | at its final width the shaper gives 68; the frame keeps a height measured at a narrower width | core / L3 |
| `dialog`/`drawer`/`sheet`/`popover`/`tooltip`/`alert-dialog` 32 vs 78 tall | 12–14 | the row's tallest trigger (web 78) wraps; cascade of the trigger measure | L3 |

`only swift` (110 per kitchen-sink case) = parts the DOM paints without
`data-xui-id`, as for gpui.
