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

## Emulator run of the device script (non-debuggable build, 2026-09-28)

Same AVD (`Medium_Phone_API_36.0`, arm64, API 36, 1080x2400, density 2.625,
surface 411.4 dp). New `benchmark` build type in `app/build.gradle.kts`:
`initWith(release)`, debug-signed, `isDebuggable = false`, R8 + resource
shrinking as release, `arm64-v8a` only; `:app:assembleProductionBenchmark`
(7.1 MB APK, `pkgFlags` without `DEBUGGABLE`). `MainActivity` honours
`exp.devScreen` on `BuildConfig.DEBUG || BUILD_TYPE == "benchmark"`.

R8 note: the first benchmark build FAILED on missing `java.awt.*` classes
referenced by JNA's `Native$AWT`. So a plain release build of the spike was
broken too. Fixed with `-dontwarn java.awt.**` in `proguard-rules.pro` (plus
`-keep class com.exponential.app.ui.spike.** { *; }`, belt and braces; the
existing `com.sun.jna.**` / `uniffi.**` keeps were already there). R8 ran
fine after that and the screen renders identically
(`/tmp/vapp4-android-benchmark.png`).

Method: cold = `am force-stop` + `am start` (5 runs, the first pass per
process); full pass = 5 caption taps (every tap flips the 0.01 dp width
nudge, so taffy's cache misses); cached pass = a new `cached` chip left of
the caption (bumps a state read in the measure block WITHOUT the nudge, so
taffy answers from its cache with 0 measure calls). Debug numbers re-taken
in the same session with the same code (`installProductionDebug`), so the
comparison is like for like. Per-callback cost = (taffy minus host) / calls.

### Kitchen sink (48 nodes)

| pass | build | calls (memo hits) | taffy best (spread) | wall best (spread) | host | per JNA callback |
|---|---|---|---|---|---|---|
| cold | benchmark | 440 (248) | 46.7 ms (46.7 to 52.8; 155.9 on the very first launch after install) | 50.1 ms (50.1 to 56.4) | 10.8 to 14.3 ms | ~81 us |
| cold | debug | 440 (248) | 218.7 ms (218.7 to 230.6, 3 runs) | 232.3 ms | 38.9 to 42.1 ms | ~400 us |
| full | benchmark | 292 (175) | **11.1 ms** (11.1 to 13.3) | **12.5 ms** (12.5 to 14.4) | 3.3 to 5.1 ms | **~27 us** |
| full | benchmark, `cmd package compile -m speed` | 292 | 6.4 ms (6.4 to 11.1) | 6.9 ms (6.9 to 11.6) | 1.2 to 1.8 ms | ~18 us |
| full | debug | 292 (175) | 31.0 ms (31.0 to 51.2) | 34.8 ms (34.8 to 55.1) | 9.8 to 14.9 ms | ~72 us |
| cached | benchmark | 0 | ~1 us | 0.30 ms (0.30 to 0.56) | 0 | n/a |
| cached | debug | 0 | ~1 us | 1.08 ms | 0 | n/a |
| `layout_fixed` (no JNA) | benchmark | 268 | **0.065 ms** (0.065 to 0.12) | n/a | n/a | n/a |

### Bench tree (203 nodes)

| pass | build | calls (memo hits) | taffy best (spread) | wall best (spread) | host | per JNA callback |
|---|---|---|---|---|---|---|
| cold | benchmark | 3120 (2082) | 264.4 ms (264.4 to 283.9) | 273.4 ms (273.4 to 293.2) | 31.1 to 34.2 ms | ~75 us |
| cold | benchmark, speed-compiled | 3120 | 60.4 ms (1 run) | 62.1 ms | 6.6 ms | ~17 us |
| cold | debug | 3120 (2082) | 1418.6 ms (1418.6 to 1479.5, 3 runs) | 1458.1 ms | 126 to 142 ms | ~410 us |
| full | benchmark | 1734 (1066) | **30.7 ms** (30.7 to 35.9) | **32.3 ms** (32.3 to 37.5) | 3.9 to 4.7 ms | **~15 us** |
| full | benchmark, speed-compiled | 1734 | 35.0 ms (35.0 to 43.6) | 36.0 ms (36.0 to 44.6) | 2.7 to 3.5 ms | ~19 us |
| full | debug | 1734 (1066) | 109.4 ms (109.4 to 145.3) | 118.6 ms (118.6 to 160.3) | 17.9 to 26.6 ms | ~52 us |
| cached | benchmark | 0 | ~1 us | 0.39 ms (0.39 to 0.61) | 0 | n/a |
| `layout_fixed` (no JNA) | benchmark | 1734 | **0.26 ms** (0.26 to 0.43) | n/a | n/a | n/a |

(Call counts differ from the first table above: that run predates the
measurer-change invalidation commit. Cold passes now make 440 / 3120 calls,
full passes 292 / 1734.)

Reading:
- Non-debuggable is 3x to 4x faster than debug on the full pass (kitchen
  sink 31.0 -> 11.1 ms, bench 109.4 -> 30.7 ms) and ~5x on cold. The JNA
  upcall drops from ~50 to 70 us to **~15 to 27 us**, and full AOT
  (`speed`) does not push it below ~18 us: that is the uniffi + JNA floor on
  this emulator, not JIT warm-up.
- The conclusion from the debug run STANDS: taffy itself is 0.065 / 0.26 ms,
  the callback path is ~170x / ~120x that. The bench tree's full pass is
  ~31 ms, two frames at 60 Hz; the kitchen sink's 11 ms fits a frame only
  if nothing else runs. Kotlin's own share (host) is ~4 ms of it. A batched
  or Rust-side-memo protocol (or JNI for the one hot callback) is still
  required before the Android path is viable.
- A cached pass (nothing dirty) is 0.3 to 0.6 ms of wall on the benchmark
  build, essentially all Compose (the FFI call is ~0.1 to 0.2 ms).

### TalkBack reading order (real TalkBack 16.0, benchmark build)

How it was made to work: TalkBack developer settings, Log output level =
VERBOSE (driven with `uiautomator dump` + `input tap`); every utterance then
logs as `talkback: SpeechControllerImpl: Speaking fragment text="…"`.
`adb shell input` events do NOT reach TalkBack at all (neither `input swipe`
at 60/100/150/250 ms, nor `input tap` for touch exploration, nor
`input keycombination KEYCODE_ALT_LEFT KEYCODE_DPAD_RIGHT` although Alt +
Arrow Right is TalkBack's "next item" in the default keymap; `adb emu event
send` key events also did nothing). What works is the EMULATOR'S OWN
touchscreen: `adb emu event send EV_ABS:ABS_MT_TRACKING_ID:… 
EV_ABS:ABS_MT_POSITION_X:… EV_SYN:0:0` (0..32767 coordinates on
`virtio_input_multi_touch_1`), one DOWN, five MOVEs, one UP. TalkBack logs
`TalkBack gesture id:GESTURE_SWIPE_RIGHT detected`. Walk = touch the avatar,
then 40 to 60 right swipes, one utterance group per swipe.

| # | before (no traversal semantics) | after (`traversalIndex` + groups) |
|---|---|---|
| 0 | AC (touched avatar) | AC (touched avatar) |
| 1 | Reddit radar | Reddit radar |
| 2 | **Scan now, Button** | Kitchen sink · one taffy layout on every client |
| 3 | **Kitchen sink · one taffy layout on every client** | 3 |
| 4 | **3** | Scan now, Button |
| 5 | Sources | Sources |
| 6 to 8 | r/selfhosted 312 posts · r/opensource 88 posts · r/webdev 1.2k posts | same |
| 9 | On, Auto-scan | On, Auto-scan |
| 10 to 17 | Drafts · Cover · LIVE · Looking for a Linear alternative … · Bullet · Draft reply ready · Bullet · 2 sources cited | same |
| 18 | 62 percent, Progress 62% | same |
| 19 to 23 | All · Drafts · Sent · Archived · More | same |
| 24 to 30 | Draft reply · Title (echoes after 150 ms) · host: · Body · r/selfhosted (Drop down list) · Cancel · Send | same |
| 31 to 34 | basis 30% · grow 1 · 160 · shrink 0 · grow 2 · max 50% · 25% | same |
| then | end of surface, TalkBack wraps to the `cached` chip and the timing caption | same |

- **Before:** TalkBack's geometric sort differs from the fixture pre-order
  in exactly one spot: the header row reads `Reddit radar, Scan now, Kitchen
  sink…, 3` (the vertically centred button's top is above the subtitle's).
  The flex-demo row already read in order (its items share a top).
  (The earlier semantics test's "geometric" order that put the form first
  was an artefact of that test, not what TalkBack does.)
- **First fix attempt was WRONG:** `traversalIndex = index` on every node +
  `isTraversalGroup` on the surface only. Compose sorts the GROUP's
  flattened descendants by traversalIndex, so a leaf whose focusable node is
  an inner child (badge Text, LIVE pill, markdown, echo field, select) kept
  the default index 0 and jumped ahead of everything: the walk read
  `AC, 3, LIVE, Looking for…, Draft reply ready, 2 sources cited, Reddit
  radar, …` and the Title / host / select nodes sorted before the avatar.
- **Working fix** (`VappNodes.kt` `clipToAncestors`, the outermost modifier
  of every node): `semantics { traversalIndex = index; isTraversalGroup =
  true }` on EVERY node, `isTraversalGroup = true` on the surface Layout.
  The walk then equals the fixture pre-order from the device script step 10
  exactly, item for item.
- Remaining non-layout differences: the avatar speaks `AC` when touched (the
  inner initials Text wins over the `Alex Chen` contentDescription; TalkBack
  said `Alex Chen` when it auto-focused the avatar on open), the toggle
  reads `On, Auto-scan` (TalkBack's "State, name, type" description order),
  markdown list items add `Bullet`.
- TalkBack was disabled at the end (`enabled_accessibility_services` empty,
  `accessibility_enabled 0`, `dumpsys accessibility`: no enabled/bound
  services). Its developer "Log output level" is left at VERBOSE.

### Typing (benchmark build)

Field focused by `uiautomator dump` coordinates, verified with a fresh dump
1 s after each run; cleared between runs with `KEYCODE_MOVE_END` + 50 x DEL.

| run | method | EditText | `host:` line | result |
|---|---|---|---|---|
| 1 to 3 | `adb shell input text abcdefghijklmnopqrstuvwxyz0123456789ABCD` | exact | `host: ` + exact | **PASS** x3 |
| 4 | one `input keyevent KEYCODE_<c>` per character in an on-device shell loop (40 invocations, 0.56 s; A to D via `input keycombination KEYCODE_SHIFT_LEFT KEYCODE_<c>`) | exact | `host: ` + exact | **PASS** |
| 5 | all 36 lowercase + digit keycodes in ONE `input keyevent` call (0.05 s) | `abc…xyz0123456789` exact | same | **PASS** |

No dropped, doubled or reordered character, the caret stayed at the end.

### Files changed in this run

`app/build.gradle.kts` (benchmark build type), `app/proguard-rules.pro`
(`-dontwarn java.awt.**`, spike keep), `MainActivity.kt` (dev-screen gate),
`ui/spike/VappKitchenSinkScreen.kt` (`cached` chip),
`ui/spike/VappSurface.kt` (`cachedTick`, surface `isTraversalGroup`),
`ui/spike/VappNodes.kt` (per-node `traversalIndex` + `isTraversalGroup`).
The debug build is reinstalled on the emulator.
