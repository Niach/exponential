# VAPP-4 · Android (Compose) findings

Device: the `Medium_Phone_API_36.0` AVD (arm64-v8a, API 36, Play image,
1080x2400, density 2.625, surface width 411.4 dp) on an M-series Mac. This is
an EMULATOR: absolute numbers are indicative only; the real-phone run is
`DEVICE-SCRIPT-android.md`. Debug build (`assembleProductionDebug`, no R8).

## Timing

All numbers from the `VappSpike` logcat line. `taffy` = `result.layout_ns`
(taffy incl. the host callbacks), `host` = time spent INSIDE the Kotlin
`Measure.measure` callback, `wall` = the whole Compose measure pass (FFI
call + the final `measure(Constraints.fixed)` of every child + placement
bookkeeping). "Full pass" = a re-measure forced by nudging the viewport
0.01 dp (see below); cold = the first pass of a fresh activity.

| tree | nodes | measure calls | taffy | host (in Kotlin) | wall | note |
|---|---|---|---|---|---|---|
| kitchen sink, cold | 48 | 303 (157 memo hits) | 111.6 ms best (5 runs: 111.6 to 200.3) | 26 to 60 ms | 125.0 ms best | first pass per activity |
| kitchen sink, full pass | 48 | 288 (155 hits) | **32.4 ms** best of 5 (32.4 to 49.6) | 10.3 to 13.6 ms | **35.7 ms** | warm |
| kitchen sink, cached pass | 48 | 0 | ~1 µs | 0 | 1.1 to 3.0 ms | same viewport, nothing dirty |
| bench 200, cold | 203 | 2084 | 348 to 932 ms | 59 to 97 ms | 368 to 974 ms | |
| bench 200, full pass, memo OFF | 203 | 2064 | **135.6 ms** best of 5 (135.6 to 193.6) | 22.8 to 41.8 ms | 142.2 ms | |
| bench 200, full pass, memo ON | 203 | 2064 (1064 hits) | **127.9 ms** best of 5 (127.9 to 156.6) | 17.0 to 25.0 ms | **137.7 ms** | |
| kitchen sink, `layout_fixed` (no JNA) | 48 | 268 | **0.065 ms** | n/a | n/a | throwaway surface, FixedMeasure in Rust |
| bench 200, `layout_fixed` (no JNA) | 203 | 1734 | **0.26 ms** | n/a | n/a | same |

`build_ns` (parse + tree build): 0.12 to 0.41 ms for both trees.

**The headline: the JNA callback path, not taffy and not Compose, is the
cost.** taffy alone does the 200-node tree in 0.26 ms. With the host measure
it takes ~130 ms, of which only ~20 ms is spent inside Kotlin (the Compose
intrinsics). The remaining ~110 ms over 2064 calls = **~50 µs per
Rust→Kotlin callback round trip** (uniffi callback interface over JNA:
`MeasureCall` serialised into a RustBuffer, JNA upcall, the `FfiSize` reply
serialised back). For the product this rules out "one FFI upcall per leaf
measure" on Android as shipped by uniffi+JNA. Options, cheapest first: (a) a
batched protocol (Kotlin pre-measures every leaf's min/max-content width and
a few heights in ONE call, Rust answers from that table and only upcalls on a
miss); (b) hand-written JNI instead of JNA for the one hot callback; (c) keep
the tree in Kotlin and run a Kotlin flex/grid (no FFI in the loop).

Other timing observations:
- taffy calls the measure function ~10x per leaf per full pass (flex + grid
  probing: min-content, max-content, definite, then the final size). A
  per-pass host memo keyed on (index, knownW, knownH, availW, mode) hit on
  52% of calls but only saves the Kotlin share (~5 ms of 135), because the
  JNA round trip happens before the memo is consulted. The memo belongs on
  the Rust side of the FFI.
- An unchanged viewport re-layouts with ZERO measure calls (taffy's cache).
  The FFI exposes no `mark_dirty(index)`, so a leaf whose CONTENT changes
  (edited text, a font-scale change, a new label) is never re-measured
  unless the viewport or a style changes. Needed before this is usable.
  The bench taps work around it by nudging the width 0.01 dp.
- Cold passes are 3x to 7x the warm ones (JNA library load + JIT + first
  text layout).

## Measure approach (intrinsics only)

`VappSurface` is ONE `Layout` whose children are every node in `nodes()`
order (containers too, as their own background boxes; children are flat
siblings, not nested). Measure callback, px/dp via `density`:
width = `knownWidth` ?: MinContent → `minIntrinsicWidth(∞)`, MaxContent →
`maxIntrinsicWidth(∞)`, Definite → `min(maxIntrinsicWidth(∞), available)`;
height = `knownHeight` ?: `minIntrinsicHeight(widthPx)`. `measure()` is called
exactly once per child AFTER taffy returns, with `Constraints.fixed` of the
frame rounded to px; placement with `place` (not `placeRelative`).
`set_rounding(false)`; rounding happens once, frame by frame, when converting
dp to px (`roundToInt` on x, y, w, h independently, so a 1 px seam between
siblings is possible; not visible in the captures).

Leaves that could not do intrinsics: **none**. `MarkdownView` wraps its blocks
in `SelectionContainer` + `Column` (no `SubcomposeLayout`), and every leaf
here answered intrinsics. A guard catches `IllegalStateException` from
intrinsics and falls back to `fixed_intrinsic(kind, props)`; the `fallbacks`
field in every log line stayed `[]`.

Text styles: font size/weight/line height only reach the host through a
`PlacedFrame`, but Compose needs them BEFORE measuring (intrinsics). The
state seeds them from one `layout_fixed()` pass at construction and
re-publishes after a real pass only if they changed. An FFI accessor for the
resolved text style per node would remove that detour.

## Typing test (`echo-field`)

- Compose test (`VappSpikeTest.typingFortyCharsFast`): 40 x
  `performTextInput(<one char>)` back to back (4.9 s total, the test driver
  is slow per call), then `waitUntil` host == string, 400 ms wait. **PASS**:
  field = `abcdefghijklmnopqrstuvwxyz0123456789ABCD`, host line = `host: ` +
  the same.
- adb shell: focused the field by coordinates (from `uiautomator dump`), then
  `adb shell input text abcdefghijklmnopqrstuvwxyz0123456789ABCD` (0.28 s for
  all 40 key events), 500 ms wait, `uiautomator dump`: EditText =
  `abcdefghijklmnopqrstuvwxyz0123456789ABCD`, host TextView = `host: ` + same.
  **PASS**, no drop, reorder or caret jump. (Screenshot
  `/tmp/vapp4-android-typing.png`.)
- The field owns a `TextFieldValue`; every edit bumps a revision and launches
  a 150 ms delayed echo that applies only if its revision is still the latest.
  Taffy never re-measured during typing (known width, fixed height), so typing
  costs nothing on the layout side.

## Accessibility order

`VappSpikeTest.semanticsOrder` walks the UNMERGED semantics tree under the
surface in child order. Tree order = fixture pre-order, as designed
(composition order = node order): `Alex Chen, AC, Reddit radar, Kitchen sink
· one taffy layout on every client, 3, Scan now, Sources, r/selfhosted,
312 posts, … , 25%`. The first six after the avatar match the contract:
**PASS**.

Geometric order (sort by top, then left, of bounds in root) is **NOT** equal:
it starts `Title (echoes after 150 ms), host:, Body, …, 25%, Reddit radar,
Alex Chen, Scan now, AC, 3, Kitchen sink…`. The form/flex-demo nodes sorting
first is odd (they sit below the fold; their reported `boundsInRoot` looks
clipped/collapsed for off-screen content, not investigated further). The
header part is expected: the vertically centred avatar, badge and button
have a different `top` than the title/subtitle column.
TalkBack's own ordering is geometry-based inside a container, so without
`isTraversalGroup`/`traversalIndex` a real sweep will likely read the header
as `Reddit radar, Alex Chen / Scan now…` rather than pre-order. Fix if the
product needs pre-order: `Modifier.semantics { traversalIndex = index }` on
every node + `isTraversalGroup` on the surface (cheap, we know the index).

TalkBack itself (`com.google.android.marvin.talkback` is installed on the
image): enabled via `settings put secure enabled_accessibility_services`,
opened the screen, and TalkBack put its focus on the scroll container
(whole-surface green frame). **`adb shell input swipe` right-swipes were not
recognised as TalkBack gestures** (focus never moved, same on a system
dialog), `input keycombination ALT+DPAD_RIGHT` did nothing, and TAB only moves
INPUT focus (Scan now → Auto-scan → …). No spoken utterances are logged. So
the real TalkBack reading order is NOT measured here; it is step 8 to 10 of
the device script. TalkBack disabled again afterwards.

## RTL

`forceRtl` sets the ROOT `style.direction = "rtl"` in the tree JSON (the core
reads the direction from the root only) AND `LocalLayoutDirection = Rtl` for
the leaves' insides. taffy mirrors every frame; the Layout uses `place`, so
nothing flips twice. Screens look right: header, list rows (meta left),
toggle (switch left), grid, pills (wrap from the right), form row (Send left,
select right), flex demo mirrored, progress fills from the right. The LIVE
pill stays top-RIGHT (`position: absolute; right: 8` is physical, same as the
web). Latin text in an RTL paragraph gets the usual bidi reordering
(`.self-hostable, with agents`, `posts 312`), which the web does too with
`dir=rtl`.

## What paints what

| kind | Compose primitive |
|---|---|
| box | `Box` + `drawBehind` (bg, border, radius from the frame's visual, read in the draw phase) |
| card | `Modifier.glassCard()` (+ the visual) |
| text | `Text` at the frame's fontSize/weight/lineHeight; muted/caption = onSurface @ Secondary |
| button primary | the `GlassSubmitButton` paint (solid `Palette.Primary`, radius 10) at 36 dp, own `Box` |
| button outline | `Modifier.glassButton()` capsule, 36 dp |
| button ghost | text only, radius-10 ripple |
| textfield / textarea | `GlassTextField` (single line / `minLines = 3`) |
| toggle | label `Text` + M3 `Switch` (`glassSwitchColors`), row `toggleable` like `SwitchRow` |
| select | glass trigger (CardFill, hairline, radius 12, chevron) + `GlassDropdownMenu`/`GlassMenuItem` |
| listrow | `Row` + `Modifier.flatRow()`, 32 dp, title / meta |
| badge | 20 dp capsule, `RowFillActive` |
| pill | `GlassPill` (Select mode; LIVE = Readonly + opaque + `LiveDot`) |
| avatar | `InitialsAvatar` (hue hashed from the name) |
| image | tinted `Box` + `ExpIcons.editorImage`, `contentDescription = alt` |
| divider | `GroupDivider()` |
| progress | M3 `LinearProgressIndicator`, 8 dp, rounded |
| markdown | `MarkdownView` |

Pressed: buttons collect `collectIsPressedAsState()` → `state.pressed` (read
inside the measure block) → `set_pressed([id])` → RE-LAYOUT; the core's
`:pressed` opacity is applied at placement via `placeWithLayer { alpha }`.
No local visual.

## Divergences from the web reference / the contract

1. **textfield is 56 dp, not 36**: `GlassTextField` is an M3 `TextField`
   (min height 56, 16 dp content padding). Forcing 36 clips the text. The
   textarea likewise measures ~88 dp at `minLines = 3`.
2. **The echo field's frame includes the `host: …` caption** (field + 4 dp +
   16 dp line), because the caption is part of that leaf.
3. **button primary is not literally `GlassSubmitButton`**: that composable
   hard-codes `fillMaxWidth()` + 14 dp vertical padding (48 dp). Same paint,
   own 36 dp box.
4. **pill forced to 28 dp**: `GlassPill` Md is 32 dp, Sm 24 dp; the caller
   height wins.
5. **Container children are flat siblings**, so a parent's `opacity` does NOT
   cascade to its children (none in this fixture) and `overflow: hidden` is
   emulated by clipping each descendant to the ancestor's rounded rect in the
   draw phase (works for `media`: the image takes the 12 dp corners).
6. **boxShadow is not painted** (none in the fixture).
7. Markdown renders at the app's reading size (16 sp body), so the markdown
   block is taller than on the desktop/web reference.
8. The divider is the 0.5 dp app hairline, not 1 px.
9. Signed out, `exp.devScreen` renders the screen as an overlay over the auth
   flow (so the spike needs no backend); signed in, it is pushed as the
   `kitchen-sink?bench={bench}&rtl={rtl}` route.

## Build notes

- JNA 5.17.0 `@aar`; debug is `abiFilters arm64-v8a` (release untouched);
  R8 keeps `com.sun.jna.**` + `uniffi.**`.
- `libvapp_spike_ffi.so` = 950,272 bytes (928 KiB) uncompressed arm64, plus
  JNA's `libjnidispatch.so` 176,520 bytes. Debug APK 28.6 MB.
- **The generated bindings do not compile under Kotlin 2.4**:
  `VappException.Invalid(val message: String)` hides `Throwable.message`
  without `override`. Workaround in `app/build.gradle.kts`
  (`patchVappUniffi`): the generated file stays untouched on disk, Kotlin
  compiles a patched copy (field renamed to `reason`) from
  `build/generated/vapp-uniffi`. Real fix: rename the field in
  `spikes/vapp-ffi/src/lib.rs` (`VappError::Invalid { reason }`) and
  regenerate; the patch then becomes a no-op copy.

## Captures

- `/tmp/vapp4-android-kitchen-sink.png` (+ `-bottom.png`, scrolled)
- `/tmp/vapp4-android-kitchen-sink-rtl.png` (+ `-bottom.png`)
- `/tmp/vapp4-android-typing.png` (after `adb shell input text`)
- side-by-side pairs: `/tmp/vapp4-android-kitchen-sink{,-rtl}-pair.png`

The `sg_vapp-kitchen-sink` styleguide step compiles but was not run: the
styleguide walk needs the seeded web backend at `10.0.2.2:5173`, which was
down during this lane.
