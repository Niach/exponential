## Spike report: StyleX-subset layout on taffy, painted by SwiftUI + Compose + gpui

Branch `exp/VAPP-4` (no PR). Full write-up: `spikes/vapp-layout/REPORT.md` on the branch, per-lane detail in `FINDINGS-{core,desktop,web,ios,android}.md`, authoring probes in `spikes/vapp-authoring/`.

### Verdict: GO, with one design change for #VAPP-36 / #VAPP-42 / #VAPP-40

One taffy layout in Rust, painted by four renderers, gives the same kitchen sink on web, desktop, iOS and Android (`shots/vapp-kitchen-sink/`, five webps committed). Geometry parity is exact where it can be measured: the browser's frames match taffy's within 1/64 px on all 48 nodes at 390 and 900 px, LTR and RTL. Accessibility order is correct on every platform once the natives pin it. The typing test passed on all four (40 of 40 characters, in order, with the 150 ms echo). RTL works in taffy 0.12 as is.

What does NOT pass as designed is the measure protocol. "One FFI upcall per leaf measurement" is too slow on Android with uniffi over JNA (about 50 µs per round trip, 130 ms for a 200-node surface on the emulator) and marginal on iOS (3.8 µs per call, 12 ms for 200 nodes on the simulator, Debug). taffy itself needs 0.26 ms for that tree; it just asks every leaf 10 to 15 times per pass (min-content, max-content, definite widths, then the known size), so 200 nodes means 2000 to 3000 callbacks. The contract must switch to batched or Rust-side text measurement (options below). With that, the 2 ms budget is reachable: the Rust part is 0.3 to 0.5 ms.

### Numbers (200-node bench tree, 133 leaves)

| platform | measure calls | taffy incl. callbacks | wall | note |
|---|---|---|---|---|
| Rust only, fixed measure (Mac, release) | 2596 to 2992 | 0.44 to 0.47 ms | n/a | the engine |
| Desktop gpui (Mac, release) | 1942 cold / 1778 per new width / 0 steady | 3.0 / 1.1 ms / 19 µs | same | gpui's line cache absorbs repeats |
| iOS simulator (Debug) | 3058 cold / 0 warm | 11.5 ms / 7 µs | 12.3 ms / 0.84 ms | 3.8 µs per Swift callback; warm wall = marshalling 203 frames |
| Android emulator (Debug) | 2064 / 0 cached | 128 ms / 1 µs | 138 ms / 1 to 3 ms | ~50 µs per JNA callback; only ~20 ms is Kotlin measuring |
| Android, Rust only (no JNA) | 1734 | 0.26 ms | | |
| real iPhone / mid-range Android | pending | | | device scripts on the branch, asked below |

Kitchen sink (48 nodes): Rust 0.15 ms; desktop 0.25 ms per new width; iOS 2.8 ms cold / 0.27 ms warm; Android 32 ms full pass / 1 to 3 ms cached.

### Geometry parity (browser vs taffy, fixed fake measure)

48/48 nodes within ±1 px at 900, 390, 900-rtl, 390-rtl; max delta 0.006 px (Blink's 1/64 px unit). Three rules make CSS agree with taffy: every Box restates taffy's defaults (`box-sizing: border-box; display: flex; flex-direction: row; position: relative`), control chrome is paint or inside the measured leaf size, and `overflow: hidden` maps to `overflow: clip`. Ablations: browser-default content-box → 1/48 match; a real 1 px card border → 7 to 9/48; `min-width: 0` (the RN default) → 43/48 at 390.

### Accessibility order

- Web: DOM order = pre-order. Desktop: AccessKit tree = element order (roles + labels on every leaf; no screen-reader run).
- iOS: SwiftUI orders a custom Layout's elements GEOMETRICALLY; fixed with `accessibilitySortPriority(count - index)`. An in-app UIAccessibility walk then reads the exact pre-order (XCUITest's snapshot ignores sort priority, so the test walks the real tree). VoiceOver by ear: pending.
- Android: semantics tree = composition order = pre-order (verified); TalkBack sorts geometrically inside a container, so `traversalIndex` + `isTraversalGroup` are needed (not applied yet). TalkBack could not be driven from adb: pending by ear.

### Typing (host-owned field, 150 ms echo, 40 fast characters)

Web 9/9 (Playwright key events, 5 ms delay). Desktop pass (input handler + cliclick). iOS simulator 3/3 (XCUITest typeText). Android emulator pass twice (Compose test and `adb shell input text`, 0.28 s for 40 keys). The rule: the client owns the string, every edit carries a revision, an echo applies only if its revision is still the latest. Layout never runs during typing.

### RTL

taffy 0.12 honours `direction: rtl` in flexbox, grid, block and absolute placement (per node, not inherited; the core copies the root's direction down). Physical properties stay physical exactly like CSS (`right: 8` keeps the badge on the right). SwiftUI mirrors a custom Layout's placements itself, so the iOS painter un-flips; Compose (`place`), gpui and the browser do not double-flip.

### The v1 style whitelist

Layout: `display` (flex, grid, block, none), `flexDirection`, `flexWrap`, `justifyContent`, `alignItems`, `alignContent`, `alignSelf`, `justifySelf`, `flexGrow`, `flexShrink`, `flexBasis`, `gap`, `rowGap`, `columnGap`, `width`, `height`, `minWidth`, `minHeight`, `maxWidth`, `maxHeight` (px, `"N%"`, `"auto"`), `aspectRatio`, `position` (relative, absolute), `top`, `right`, `bottom`, `left`, `inset`, `padding` + Top/Right/Bottom/Left/Horizontal/Vertical, `margin` + the same (`"auto"` allowed), `gridTemplateColumns`, `gridTemplateRows` (px, number, %, fr, auto, min-content, max-content, minmax(), repeat()), `gridTemplateAreas` (string rows), `gridArea`, `gridColumn`, `gridRow`, `overflow`, `direction` (root).
Visual: `backgroundColor`, `color`, `borderWidth` (the one visual key with layout effect), `borderColor`, `borderRadius`, `opacity`, `boxShadow` (preset token), `fontSize`, `fontWeight`, `lineHeight`, `textAlign`. Colours: `#hex` or `$group.token`.
Conditions: `"@media (min-width: Npx)"` against the SURFACE width, `":pressed"`; nested objects, resolved on the client.
Changes the spike forces: add logical `marginInlineStart/End`, `paddingInlineStart/End`, `insetInlineStart/End` (taffy has only physical rects; the client resolves them from the root direction); paint order = tree order with absolute nodes last among siblings (validated), no `zIndex`; per-kind control heights as tokens, not px. Confirmed unnecessary for v1: `calc()`, named lines, `gridAutoFlow`, `justifyItems`, `order`, multi-value shorthands.

### Where the platforms diverged

Nothing in the engine. Line breaking (gpui breaks at `/`, SwiftUI Text at a 0 proposal breaks inside words); control chrome sizes (web Badge 16 / Pill 24, iOS pill 24 / toggle 31 / progress 4, Android text field 56 because it is a Material TextField); flat painting on iOS and Android (parent opacity does not cascade, `overflow: hidden` is emulated by clipping descendants); each platform's markdown renderer sizes itself; pressed = CSS `:active` on web vs a relayout on the natives; `$semantic.*` has no CSS variable yet.

### Authoring package: our own `vapp-css`

`@stylexjs/stylex` 0.19.1 without the compiler THROWS on `create` and `defineVars`; `@stylexjs/dev-runtime` (latest 0.11.1) fails to load against it and would only yield class-name maps plus CSS. `react-strict-dom` 0.0.55 native (with a hand-made react-native shim, 178 MB installed) does return plain resolved objects but DROPS `gridTemplateAreas`/`gridTemplateColumns`, only understands StyleX's value-level condition form, and resolves `@media` on the host against the screen. `vapp-css.ts` (about 150 lines, zero deps): StyleX's `create`/`props` shape with typed keys, identity + whitelist validation, ordered merge keeping conditions nested for the client; validated all 28 kitchen-sink styles. Decision: ship it in `packages/vapp-sdk`, key list generated from the same source as the Rust whitelist.

### Recommendations

1. Measure protocol, in order of preference: (a) text shaping in Rust (parley or cosmic-text on the platform's system fonts; only controls measured natively, batched in one call); (b) if text stays native, a batched two-phase protocol (one upcall for every text leaf's min/max-content width, a taffy pass, one upcall for heights at the decided widths) plus a Rust-side memo per (index, known, available); (c) on Android, hand-written JNI for the hot callback instead of JNA. Never (c) alone.
2. `Surface` API (already in the facade): `mark_dirty(index)`, `invalidate_measures()`, measurer-identity check (iOS found stale sizes when the measurer changed); still needed: a text-style accessor before the first pass (Compose intrinsics need the font first), `set_direction`, visuals sent once per node and deltas after (iOS warm wall is marshalling).
3. Painters MUST pin the accessibility order (`accessibilitySortPriority` on iOS, `traversalIndex` on Android) and group cards for screen readers.
4. Desktop keeps painting the shared frames (gpui's `Styled` cannot express grid areas or mixed tracks).
5. `frames-{390,900}[-rtl].json` seed `vapp-layout.json`; the geometry-diff script seeds #VAPP-13.

Pending on real hardware (does not change the verdict): iPhone bench captions + VoiceOver sweep + hand typing (`DEVICE-SCRIPT-ios.md`), a mid-range Android phone for the same plus TalkBack (`DEVICE-SCRIPT-android.md`).
