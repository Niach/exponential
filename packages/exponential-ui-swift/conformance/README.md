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

## Today (the budget after the VAPP-100 integration)

48 cases: size 1196 · position 6728 · wrap 0 · only web 24 · only swift 2640
(gpui's `fixtures/conformance-known.json`: size 1564 · position 6402 · only
web 24). `responsive/*` equals gpui case for case. `kitchen-sink/*`: size
38–57 per case (gpui 55–71); position 253–327 (gpui 242–309): the vertical
cascade of the core-side origins below runs further down the page now that
more nodes match. The origins left, grouped:

| origin | diagnosis | owner |
|---|---|---|
| `chart`, `chart-line`, `chart-donut` 160/200/180 vs 208/224/204 | the core pins the Chart's `height` style to the PLOT height (`layout_tree/misc.rs`); the web adds the title + legend rows | core |
| `form-volume` (Slider) 30 vs 40 | the `track` gets the recipe's bar `height: 6` as its frame; the web's slider root is the thumb's 16 | core |
| `form-plan` (Radio) 44 vs 48 | the items row gap | core / recipe |
| `page-1` (Image) 180 vs 182.25 | Image/Video never get a height-for-width question (`measure.rs` lists them as fixed) | core |
| `main-md` (Markdown) 88 vs 68 at ≥ 900 | the measurer answers 68 at the final width; the frame keeps a height asked at a narrower width | core |
| `toggles` (ToggleGroup) full width vs 132, `sparkline` 96 vs 57, `tree-guides`, `resp-drawer` | stretch / shrink of a leaf in its row | core |
| `hdr-badge`, `nav-inbox.count`, `pill-1` | the badge / pill 32–36 px minimum the web does not apply | core / recipe |
| `table` rows, `nf-files`, `code` | part heights of the core-built natives | core / recipe |

`only swift` (110 per kitchen-sink case) = parts the DOM paints without
`data-xui-id`, as for gpui.
