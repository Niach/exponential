# VAPP-4 spike · lane contract

Four painters (iOS SwiftUI, Android Compose, desktop gpui, web CSS) render ONE
fixture, `packages/domain-contract/fixtures/vapp-kitchen-sink.json`, and must agree
without talking to each other. This file is the contract. The Rust core is
`apps/desktop/crates/vapp-spike` (read `src/lib.rs`, `src/measure.rs`,
`src/surface.rs`); the mobile facade is `spikes/vapp-ffi` (read `src/lib.rs`).

## The tree

`{ id, kind, style?, props?, children? }`. `id` is unique and stable. Kinds:

| kind | container? | props | measured size (host) |
|---|---|---|---|
| `box`, `card` | yes | — | never measured; `card` = a box painted as the platform's card (glass card surface + hairline + radius 16) |
| `text` | no | `text`, `variant` = title\|body\|muted\|caption\|label | the platform's text at the node's `fontSize/fontWeight/lineHeight` (from `PlacedFrame`), wrapped at `wrap_width` |
| `button` | no | `label`, `variant` = primary\|outline\|ghost | the platform's button (height token `controlLg` 36) |
| `textfield` | no | `placeholder`, `echo` (bool) | the platform's single-line field (36 tall). `echo: true` = the typing-test field, see below |
| `textarea` | no | `placeholder` | multi-line field, ≥ 72 tall (style `minHeight`) |
| `toggle` | no | `label`, `checked` | label + switch in a row |
| `select` | no | `options[]`, `value` | the platform's picker/select trigger (36 tall) |
| `listrow` | no | `title`, `meta` | a flat list row: title left, meta right, 32 tall |
| `badge` | no | `count` | count badge, 20 tall |
| `pill` | no | `label`, `selected`, `tone` (`live`) | the platform's Pill (28 tall); `tone: live` = the LIVE pill with a dot |
| `avatar` | no | `name`, `size` | initials avatar, `size` × `size` |
| `image` | no | `placeholder` (a colour), `alt` | a tinted rectangle (no network); its frame comes from the parent's `aspectRatio` |
| `divider` | no | — | 1 px hairline, full width |
| `progress` | no | `value` 0..1 | the platform's linear progress, 8 tall |
| `markdown` | no | `text` (GFM) | the platform's read-only markdown renderer |

## Styles

Only keys in `vapp_spike::WHITELIST` appear. Numbers are px/pt/dp. Colours are
`#rrggbb` or `$group.token` into the design tokens (`$palette.background`,
`$semantic.green`, …). The core resolves `@media (min-width)` against the
SURFACE width you pass to `set_viewport` (not the screen) and `:pressed`
against the ids you pass to `set_pressed`. Painters never look at `style`;
they paint `PlacedFrame.visual` (+ `font_size/font_weight/line_height`).

## The layout pass (natives)

1. `Surface(treeJson)` ONCE per screen (keep it in state). `nodes()` ONCE → your
   per-index view models (parse `props_json` once).
2. On every layout: `set_viewport(width, 0)` (0 = as tall as the content; the
   screen scrolls), `set_pressed(ids)`, then `layout(measure)`.
3. `measure(call)` is called for LEAVES only, with the node INDEX. Reply the
   CONTENT size of that control:
   - width constraint = `call.wrap_width` (known → definite available → nil =
     max-content). `available_width_mode == MinContent` = the narrowest the
     control can be (text: its longest word). Both known ⇒ never called.
   - SwiftUI: `subviews[index].sizeThatFits(ProposedViewSize(width: wrap, height: nil))`;
     min-content = `width: 0`, max-content = `width: nil`.
   - Compose: intrinsics ONLY (`minIntrinsicWidth` for MinContent,
     `maxIntrinsicWidth` for MaxContent, `minIntrinsicHeight(w)` for the height);
     a `Measurable` may be `measure()`d only once per pass (do it after taffy
     returns, with `Constraints.fixed`). Intrinsics are Int px → divide by density
     before replying; taffy works in dp. Call `set_rounding(false)` and round once
     yourself when converting the final frames to px.
   - Leaf styles carry NO padding: the control's own chrome is inside the size
     you report (border-box). taffy adds nothing for leaves.
4. Paint `frames` in the order returned (pre-order): parents first (background,
   border, radius, opacity, clip when `overflow_hidden`), then children. Frames
   are ABSOLUTE surface coordinates (x, y, width, height). Absolute-positioned
   nodes already sit last among their siblings in the fixture.
5. Accessibility order = the same order. Give leaves an accessibility label
   (text/label/title/placeholder) so the sweep can read it.
6. Pressed: a Button reports press-down/up → `set_pressed([id])` → re-layout.
   `:pressed` in this fixture only changes `opacity`, so a painter MAY apply the
   pressed visual locally without a relayout; note which you did.
7. Timing: wrap the `layout()` call with a monotonic clock; also read
   `result.layout_ns` (taffy incl. callbacks) and `result.measure_calls`.
   Bench mode: `bench_tree_json(200)` at the screen width. Log
   `nodes, measure_calls, layout_ns, wall_ns, build_ns` on every pass.

## Direct open + captures

- iOS: launch args `-uiTesting -uiTestingScreen kitchen-sink` open the screen
  (pattern: `Exponential/UI/Onboarding/OnboardingView.swift:381`); `-uiTestingScreen kitchen-sink-bench`
  opens the 200-node bench. Styleguide lane step name: `snapshot("sg_vapp-kitchen-sink")`.
- Android: intent extra `exp.devScreen=kitchen-sink` (`adb shell am start -n <pkg>/.MainActivity --es exp.devScreen kitchen-sink`)
  + the same value through the instrumentation `shots` flow. Step name:
  `screenshot("sg_vapp-kitchen-sink")` (ios + android names MUST match, gated by
  `packages/view-catalog/src/views.test.ts`).
- Desktop: `EXP_DEV_SCREEN=kitchen-sink` (`navigation::parse_dev_screen`);
  `EXP_DEV_VAPP_BENCH=200` swaps in the bench tree.
- Web: `/vapp-kitchen-sink` (signed in); `?geometry=1` = fixed-measure geometry
  mode; `?width=390` = force the surface width.

## The typing test (`echo-field`)

Host-owned text: the local field owns the string. Every edit bumps a local
revision `r` and schedules a fake host echo `{r, value}` 150 ms later. When an
echo arrives, apply it ONLY if `r == latestRevision` (stale echoes are dropped).
The test types 40 characters `abcdefghijklmnopqrstuvwxyz0123456789ABCD` as fast
as the platform's test driver can, waits 400 ms, and asserts the field shows
exactly that string. Report pass/fail + how the characters were injected.

## The fixed measure (geometry mode, web ↔ taffy)

`FixedMeasure` in the core: text = 8 px per char, ONE 20 px line (no wrap);
button = 8·len(label)+24 × 36; pill = 8·len+20 × 28; listrow = 8·(len(title)+len(meta))+16 × 32;
textfield/select 160×36; textarea 160×72; toggle 44×24; badge 20×20;
avatar size×size; image 320×180; divider 0×1; progress 160×8; markdown =
8·widest line × 20·lines. A known dimension wins. `spikes/vapp-layout/frames-{390,900}[-rtl].json`
are the core's frames with rounding OFF.

## Findings

Append your numbers and observations to `spikes/vapp-layout/FINDINGS-<lane>.md`:
timing table (nodes, measure calls, taffy ns, wall ns, device/simulator),
a11y order result, typing result, RTL observation (does your Layout flip
placements under RTL? taffy already mirrors), every place your platform
diverged from the web reference and why, and any whitelist property you could
not honour.
