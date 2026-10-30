# Exponential UI — round 5 contract (VAPP-162)

Round 5 adds the low-level pieces the spezl drafts (a chat app with voice
calls, push-to-talk, computer-use plans and QR pairing) kept rebuilding: a
chat bubble, a call screen, a plan card, a QR code and a QR scanner, plus
five extensions of existing components. This file is the CONTRACT half:
what changed in `@exponential_app/eui` and what each renderer implements to
match. Everything here is locked by a fixture or a test.

Read §0, the sections you own, then §11 (your checklist).

## 0. What moved

| area | change | source of truth | locked by |
|---|---|---|---|
| serif | `type.family.serif` (append-only), `ui-serif` in every built-in theme, `fonts["ui-serif"]` = system, fallback `Georgia, serif` | `tokens.json`, `themes/*.json`, `generate-themes.ts` | `theme-resolved.json`, `theme-invalid.json`, the `Text` specimen extra |
| `MessageBubble` | NEW macro (list, lite): text, side in\|out, time, status sending\|sent\|read, author, slot `attachment` | `core.catalog.json`, `macros.json` | `catalog-macros.json`, `bind-time.json` |
| `Segmented` | `orientation` horizontal\|vertical (vertical `bar` = THE navigation rail); `option.count` = a badge on a `bar` item; recipe part `badge`, `when` prop `orientation` | `core.catalog.json`, `recipes.json` | `catalog-components.json`, `control-geometry.json` (`orientation=*`, `variant=bar,orientation=vertical`) |
| `CallSurface` | NEW macro (media, not lite): name, avatar, state ringing\|connecting\|talking\|ended, activity, level, bands, status, caption, muted (two-way), speaker; events mute, speaker, hangUp | same | `catalog-macros.json` (every `state=*`, `muted=*`, `speaker=*`, `bound:*`) |
| `VoiceWave` | `variant` ring\|bars; `bars` = a horizontal level meter (no child); part `bar` | `core.catalog.json`, `style.json voiceWave.bars`, `src/voice-wave.ts voiceWaveBars` | `voice-wave.json` `bars`, `control-geometry.json` (`variant=bars,size=*`) |
| `QrCode` | NEW native (media, not lite): value, size, level L\|M\|Q\|H, label; THE MATRIX COMES FROM THE CORE | `src/qr-code.ts` (Nayuki port), `style.json qrCode` | `qr-code.json` |
| `QrScanner` | NEW native (input, not lite): hint, formats, label, disabled; event `scanned {value}`; the camera is HOST-OWNED | `core.catalog.json`, `style.json qrScanner` | `catalog-components.json` |
| `TraceItem` | `traceStatus` += `done`, `skipped` | `core.catalog.json`, theme recipes | `theme-recipes.json`, `catalog-macros.json` |
| `Plan` | NEW macro (list, lite): title, description, steps (`planStep`), state proposed\|running\|done\|declined, progress, approve/decline/stop labels; events approve, decline, stop | `core.catalog.json`, `macros.json` | `catalog-macros.json` |
| `Button` | `hold` + `holdingLabel`; events += holdStart, holdEnd, holdCancel | `core.catalog.json` | `interactions-round1.json` `buttonHold` |
| icons | concepts `call-incoming` (phone-incoming), `ui-qr-code` (qr-code), `ui-scan` (scan-line), `ui-read` (check-check), `ui-hold` (hand), `voice-level` (audio-lines); `mic-off`, `phone-off` were already concept targets (`call-unmute`, `call-end`) | `packages/icons/icons.json` | the icon generator ×4 |
| strings | `cameraUnavailable`, `endCall`, `mute`, `no`, `read`, `scanQrCode`, `sending`, `sent`, `speaker`, `speakerOff`, `unmute`, `yes` | `strings.json` | generated STRING tables |
| kitchen sink | + CallSurface, two MessageBubbles, Plan, QrCode, QrScanner (every visible component appears once) | `kitchen-sink.json` | `kitchen-sink.expanded.json` |

Counts: 87 offered components (49 natives, 38 macros; 61 lite), + the hidden
TreeGuides and Unknown. Prompt budget 7,410 → 7,900 tokens (the measured
total after trimming every new description, a named exception).

## 1. The serif family

`$type.family.serif` is valid wherever `sans`/`mono` are: a recipe's
`fontFamily`, a Box/Text node style's `fontFamily`. Every built-in theme
names `ui-serif` (neutral and playful directly, exponential through the
generator, orange by inheritance); `fonts["ui-serif"]` = `{fallback:
"Georgia, serif", source: "system"}`. Painters map the generic name, like
`ui-monospace`:

| renderer | `ui-serif` |
|---|---|
| web (React, Solid, element) | `font-family: ui-serif, Georgia, serif` (the family, then the fallback list: `fontStack` already does this) |
| SwiftUI | `Font.system(size:weight:design: .serif)` (New York); TextKit measure with the same descriptor (`.withDesign(.serif)`) |
| Compose | `FontFamily.Serif`; the TextShaper's generic switch gains `ui-serif` / `serif` |
| gpui | a system serif: `New York` on macOS (fall back to `Georgia`, then `Times New Roman`); the measure's availability check treats `ui-serif` like `ui-monospace` |
| conformance | `conformance/fonts.json` maps `ui-serif` to the substitute `Inter` (no serif face is committed), so the harness compares like with like |

Specimen: `exponential-ui-text` case `fontFamily · serif` (a lead Text with
`style.fontFamily: "$type.family.serif"`).

## 2. MessageBubble

Parts: `root` (column, full width, `alignItems` flex-start for `in`, flex-end
for `out`; role group labelled by `author`, else `time`) → `bubble` (column,
gap xxs, max width 80 %, padding md × sm, radius xl, the top-start corner 6 px
on `in` bubbles = the app's tail) → `author` (caption, one line; `in` only)
→ the `attachment` slot → `text` (Markdown) → `footer` (row, aligned end,
gap xs; shown when `time` or `status`) → `time` (caption) → `status` (Icon
sm at 14 × 14; `out` only; `sending` = `ui-clock`, `sent` = `ui-check`,
`read` = `ui-read` (check-check), labelled `$string.sending|sent|read`).
Recipe props `side`, `status`. Neutral recipes: `in` = the card fill
(`$color.card`) with a hairline `$color.border` (the app's agent bubble on
THE card surface), `out` = `$color.secondary` with the same hairline (the
app's own bubble); author/time/status muted.

## 3. Segmented `orientation` (the rail) and `option.count`

Round 3's ONE Segmented rule stays: no new component. `orientation:
vertical` lays the items out as a column: every variant stacks its segments
top to bottom (the `segmented` track becomes a vertical pill, `outline`
joins vertically: the shared border runs horizontally); a vertical `bar` is
THE navigation rail: content-sized in height, full height of its parent is
the author's style, each item still icon over caption label, the item width
= the rail width, role `navigation`, `aria-current=page` on the selected
item, ArrowUp/ArrowDown move focus. Geometry (neutral, every theme inherits):
root `{variant: bar, orientation: vertical}` = width 72, no top rule, a
hairline on the trailing (right) edge; item = width 72 × height
`$control.tabBar` (56). `control-geometry.json` `variant=bar,orientation=vertical`
locks the item box per theme. The core lays the rail out (Rust
`layout_tree`: a vertical bar is a column of fixed items, not a filling row;
TS has no layout reference for Segmented, the Rust lane owns it).

`option.count` (number, bindable): a `bar` item paints a count badge at its
icon's top-end corner (the badge's centre on the icon box's top-right
corner, mirrored under RTL), text = the number (`99+` above 99), hidden at 0
or when unset; part `badge` (neutral: min width 16, height 16, padding xxs,
radius full, primary fill, xs medium text). Every other component and
variant ignores `count`. A11y: the count reads after the label
("Chats, 3").

## 4. CallSurface

THE app call screen (`packages/ui` `VoiceCallSurface` + `CallControls`) in
catalog terms. Parts: `root` (column, centred, full width and height,
padding xl / xl3) → `head` (column, centred, max width 384) → `wave`
(VoiceWave `xl`, hidden from assistive tech; activity = `muted` when
`muted`, else ringing → `thinking`, connecting → `thinking`, ended → `idle`,
talking → the `activity` prop, default `speaking`) → `avatar` (Avatar lg,
the wave's child) → `name` (Text title, heading level 2, one line) → `status`
(muted, one line; the HOST ticks the timer, the core has no clock) →
`caption` (centred, at most 4 lines, live polite) → `spacer` (flex 1) →
`controls` (row, centred, 32 px apart): `muteTip` Tooltip → `mute` (icon
`call-mute`, `call-unmute` while muted; label `$string.mute` /
`$string.unmute`; `$set` writes a bound `muted` back; context `{muted}` =
the new value), `speakerTip` → `speaker` (only when the `speaker` prop is
given; `call-speaker` / `call-speaker-off`, `$string.speaker` /
`$string.speakerOff`, context `{speaker}`), `endTip` → `end` (`call-end`,
`$string.endCall`, event `hangUp`); while ringing ONLY `cancel` (column, gap
xs) → `end` (labelled `$string.cancel`) → `cancelLabel` (caption, the word
written under the button, hidden from assistive tech). Every round button
is a Button `size: icon` with node style 56 × 56, radius full, its Icon `lg`
(24) as the custom child; the tooltip text = its accessibility label.
Recipe props `state`, `muted`. Neutral: `mute`/`speaker` = secondary fill,
foreground icon; `mute` when `muted: true` = INVERTED (foreground fill,
background icon); `end` = THE red one (destructive fill at full strength,
destructiveForeground); name xl2 semibold.

## 5. VoiceWave `variant: bars`

`variant: ring` is today's look. `bars` = a horizontal level meter of
`count` (20) rounded vertical bars, same `activity`/`level`/`bands`/phase
inputs, NO child (a child stays in the tree, unpainted, like `sm`). Sizes
(`style.json voiceWave.bars.sizes`): sm 16 tall, 2 px bars, 2 px gaps (78
wide); md 24/3/3 (117); lg 40/4/4 (156); xl 56/5/5 (195); the root recipe's
`{variant: bars, size}` rules carry the same boxes (`control-geometry.json`).
Bar i: drive d (listening | speaking: (2 + 6·level)/8; thinking: 2.5/8;
idle | muted: 0), band b (bands[i mod len] clamped, 1 when none or
thinking), envelope e = 1 − 0.5·|2i/19 − 1|, shape s = 0.5 + 0.5·sin(1.7i +
phase·(1 + (i mod 3)·0.5)), fraction f = 0.12 + 0.88·d·b·e·(0.35 + 0.65·s);
height = round2(H·f), centred vertically, x = i·(barWidth + gap), radius
barWidth/2, filled with the `bar` part colour (primary; muted foreground when
idle or muted) at the root's opacity. Phase = `voiceWavePhase` (0 when idle,
muted, reduced motion). `voiceWaveBars(input)` is the reference,
`voice-wave.json` `bars` (8 cases) locks every rect.

## 6. QrCode

The core owns the matrix: `qrMatrix(value, level)` (`src/qr-code.ts`, a
port of Project Nayuki's QR Code generator, MIT) = byte mode over the UTF-8
bytes, the smallest version 1–40 that fits, the level EXACT (no boost), the
lowest-penalty mask → `{size, version, mask, rows}` ('1' dark, no quiet
zone); `null` for an empty value or one version 40 cannot hold at that level
(2,953 bytes at L … 1,273 at H). An unknown level is `M`. The Rust lane
ports the same algorithm (`qr_code.rs` + FFI `qr_matrix_json`), replaying
`qr-code.json` (13 cases) bit for bit; every SDK painter takes the matrix
from the core (web: the TS reference; natives: the FFI), never from a
platform QR library.

Paint (`style.json qrCode`): root = a centred column, gap 8 (`labelGap`);
the `plate` is `size` px square (default 160), radius = the plate recipe
(`$radius.md`); module = size / (n + 2·4) px (quiet zone 4 modules each
side), fractional, painters snap module EDGES to device pixels so the outer
size stays exact; DARK MODULES ON A LIGHT PLATE ALWAYS: of the resolved
`plate` and `module` colours the lighter (relative luminance) fills the
plate and the darker the modules, so a dark theme still scans. `null`
matrix = the empty plate. The `label` = a caption Text (muted, centred)
under the plate AND the img's accessibility label (no label = decorative).

## 7. QrScanner

The component is the viewfinder CHROME; THE CAMERA IS HOST-OWNED (like
FileUpload's bytes). Each renderer defines a renderer-level host hook in its
own package API (NOT `catalog/host.json`): the host renders its live feed
into the `feed` region, decodes codes of the given `formats` (default
`["qr"]`) and hands the renderer the decoded string, which the renderer
dispatches as `scanned {value}` (once per distinct value until it changes
or the scanner remounts; `disabled` stops reporting and shows the chrome at
the root's disabled opacity). Geometry (`style.json qrScanner`): root =
centred column, gap 12; `viewfinder` = a square (aspect 1) as wide as the
root up to 280 px, radius `$radius.xl`, clipping; `feed` = the host's video,
or without one the `feed` colour (foreground) at alpha 0.06; four `corner`
brackets at the inside corners, two strokes 24 px long, 3 px wide, primary,
rounded caps; `scanLine` = a 2 px primary bar inset 16 px left and right,
sweeping top → bottom → top over 2,000 ms (ease standard, alternate) while a
feed shows; reduced motion or disabled = static at the vertical centre. No
host camera = no feed, no line: the `placeholderIcon` (`ui-scan` =
scan-line, 32 px, muted) centred and the caption `$string.cameraUnavailable`
under it. `hint` = a muted Text centred under the viewfinder. A11y: img
labelled `label` (default `$string.scanQrCode`); a decoded value is
announced politely.

Lane hooks (each lane names its own, documented in its README): React /
Solid / element `qrScanner` host option `{start(feedElement, formats,
onValue) → stop}`; SwiftUI an environment value (`\.exponentialQrScanner`)
returning a feed view + an async value stream; Compose a `CompositionLocal`
(`LocalQrScanner`); gpui a host trait method. Absent = the placeholder.

## 8. TraceItem `done`/`skipped` and Plan

`traceStatus` = running | error | pending | done | skipped (appended).
`done` = the line at full strength, its icon tinted `$color.success`;
`skipped` = dimmed like pending (line opacity 0.6) and the title struck
through (`textDecoration: line-through`). Recipes only; the template is
unchanged (`shimmer` still only for running).

**Plan** parts: `root` (column; group labelled by `title`) → `card` (a Card)
→ `header` (column, gap xxs) → `title` (Heading h3) → `description` (muted)
→ `steps` (a Trace; `$each` step → `step` = TraceItem with icon, title,
description = `detail`, status) → `footer` (proposed only; row, end, gap sm:
`decline` outline Button `{declineLabel|$string.no}` → event `decline`,
`approve` default Button with `ui-check` `{approveLabel|$string.yes}` →
`approve`) → `running` (running only; row, gap md: `progress` = Progress
`value: progress`, `max: 1`, flex 1, then `stop` = destructive Button with
`ui-stop` `{stopLabel|$string.stop}` → `stop`). `done`/`declined` = no
footer. Recipe prop `state`. `steps` is static (an `$each` over a bound
array emits nothing; bind each step's status instead).

## 9. Button `hold`

With `hold: true` a press NEVER fires `press`. Pointer down (or Space /
Enter key down; key repeat ignored) fires `holdStart` and paints the
`pressed` state and `holdingLabel` (when given); while held the pointer may
wander up to 24 px (THE slop) outside the box; release (pointer up within
the slop, or key up of the key that started it) fires `holdEnd`; the pointer
leaving the box by more than the slop (the slide away), a pointer cancel,
Escape or losing focus fire `holdCancel`; after a cancel nothing fires until
the next down. `disabled` fires nothing. Touch: the platform's long-press
and scroll gestures must not steal a hold (claim the pointer on down).
`interactions-round1.json` `buttonHold` (10 cases, step by step: the events
each step fires, `pressed`, the painted label) is replayed by every lane.

## 10. Breaking changes

None for authors. Renderers: the new enums and components appear in the
generated constants; `traceStatus` gained two values; the kitchen sink grew
(replay baselines, bench node counts and allocation counts move).

## 11. Per-renderer checklist

Every renderer: replay the regenerated `catalog-components.json` (579),
`catalog-macros.json` (312), `bind-time.json`, `theme-*.json`,
`control-geometry.json` (400), `kitchen-sink*.json`; paint the five new
specimens (`exponential-ui-message-bubble`, `-call-surface`, `-plan`,
`-qr-code`, `-qr-scanner`, view-catalog entries added) and the new extras
(TraceItem done/skipped, Segmented rail with counts, VoiceWave bars ×4,
Button hold, serif Text); re-record the shots ×4.

- **TS reference (done here)**: `qrMatrix`, `voiceWaveBars`, the fixtures.
  Left for the React lane: re-record `fixtures/conformance-baseline.json`
  (the web DOM dump of the grown kitchen sink and the new cases) and the
  ratchets in `conformance-known.json`.
- **React / Solid / element**: QrCode (SVG of `rows`, the plate swap),
  QrScanner (the `qrScanner` host option, the placeholder, the sweep via a
  CSS animation honouring `prefers-reduced-motion`), VoiceWave bars, the
  Segmented column + rail + badge, Button hold (pointer capture, slop,
  Escape/blur, `interactions-round1.json buttonHold` in
  `interactions-fixture.test.tsx`), `ui-serif` in `fontStack`; Solid and the
  element stay parity-gated against React.
- **Rust core**: `qr_code.rs` (the port, `qr-code.json` bit for bit, FFI
  `qr_matrix_json`), `voice_wave_bars` (`voice-wave.json bars`, FFI), the
  vertical Segmented in `layout_tree` (a vertical bar = a column of fixed
  72 × 56 items, not content-sized in width), Button hold state + events
  (`interactions_fixture.rs` replays `buttonHold`), the new enums/strings
  from the generated catalog.
- **gpui**: paint QrCode (module rects from the FFI-free core call),
  QrScanner (host trait, placeholder), VoiceWave bars, the rail + badge,
  Button hold (mouse capture, 24 px slop), the serif mapping; re-record the
  conformance ratchet against the new web baseline.
- **SwiftUI**: QrCode (Canvas of the core matrix, pixel-snapped), QrScanner
  (environment hook, AVFoundation stays in the HOST, not the package),
  VoiceWave bars, the rail + badge, Button hold (a `DragGesture(minimumDistance: 0)`
  with the slop), `.serif`; snapshots re-recorded.
- **Compose**: the same five; QrScanner through `LocalQrScanner` (CameraX
  stays in the host), Button hold via `awaitEachGesture` with the slop,
  `FontFamily.Serif`; snapshots re-recorded on ubuntu.
