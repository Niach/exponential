# VAPP-4 findings: web (real CSS reference)

Lane: web + authoring. Browser: Playwright Chromium (bundled with
`@playwright/test` in the repo), built app (`bun run build`, served by
`bun .output/server/index.mjs` on :5173 behind Caddy https://localhost:3000),
signed in as the seeded demo user.

Code: `apps/web/src/components/vapp-spike/` (`fixture.ts`, `measure.ts`,
`style-to-css.ts`, `render.tsx`), route
`apps/web/src/routes/_authenticated/vapp-kitchen-sink.tsx`
(`?geometry=1`, `?width=N`, `?rtl=1`), scripts
`apps/web/scripts/vapp-spike/{screenshot,geometry-diff,typing}.ts`
(run from `apps/web` with `bun scripts/vapp-spike/<name>.ts`).

## How the browser is made to agree with taffy

- ONE `<style>` per surface: a base rule per node (`.vapp-n-<id>`), then every
  `@media (min-width: N)` as `@container vapp (min-width: N)` in ascending N,
  then `:pressed` as `:active`. The surface root sits inside an extra wrapper
  with `container-type: inline-size; container-name: vapp`, so breakpoints
  resolve against the SURFACE width like the core (a container query cannot
  target its own element). No inline styles, so conditions can override.
- Every node gets taffy's defaults restated: `display: flex; flex-direction:
  row; box-sizing: border-box; position: relative`. `min-width` stays `auto`
  (both engines agree). Leaves are a layout box (`flex-direction: column`)
  with the real control inside it; leaf styles carry no padding.
- `overflow: hidden` is emitted as `overflow: clip`: taffy maps both to
  `Overflow::Clip`, which is not a scroll container, so its automatic minimum
  size stays content-based. CSS `hidden` would zero it.
- The card hairline is paint (inset box-shadow), not a CSS border: the core has
  no border on `card`, and a CSS border takes layout space.
- `gridTemplateAreas: ["nav main", "nav footer"]` → `'"nav main" "nav footer"'`;
  `paddingHorizontal/Vertical`, `marginHorizontal/Vertical` → physical
  left/right, top/bottom (like `style.rs`); `borderWidth` adds `border-style:
  solid`; `$palette.x` → `var(--color-x)`; `$semantic.x` → the tokens.json hex
  inlined (styles.css has no semantic variables). Unknown keys throw.

## Geometry diff (fixed fake measure, rounding off, ±1 px)

`bun scripts/vapp-spike/geometry-diff.ts` against `frames-{900,390}[-rtl].json`:

```
900: 48/48 nodes within ±1px (max |delta| 6.26e-3)
390: 48/48 nodes within ±1px (max |delta| 0.00e+0)
900-rtl: 48/48 nodes within ±1px (max |delta| 6.26e-3)
390-rtl: 48/48 nodes within ±1px (max |delta| 0.00e+0)
```

No node is beyond tolerance, so there is no delta table. The five largest
deltas (all 900 px, LTR and RTL alike) are the flex-demo row:

| node.axis | taffy | browser | delta |
|---|---|---|---|
| fx-1.w | 302.6 | 302.59 | -0.00626 |
| fx-2.x | 326.6 | 326.59 | -0.00626 |
| fx-2-t.x | 350.6 | 350.59 | -0.00626 |
| fx-3.x | 494.6 | 494.59 | -0.00626 |
| fx-3.w | 164.4 | 164.41 | +0.00626 |

Explanation: Blink stores layout in `LayoutUnit` = 1/64 px fixed point, so
302.6 snaps to 302.59375. Fractional grid tracks (nav-card 285.333) land within
the same quantum. At 390 every value is a multiple of 1/64, hence exactly 0.

### Ablations (what each alignment rule is worth)

Same diff with one CSS override injected (`ABLATE_CSS=...`):

| override | 900 | 390 | max delta |
|---|---|---|---|
| none (as shipped) | 48/48 | 48/48 | 0.006 |
| every node `box-sizing: content-box` (browser default) | 1/48 | 1/48 | 32 px |
| cards with a real 1 px CSS border | 9/48 | 7/48 | 6.9 px |
| every node `min-width: 0` (the RN/Yoga default) | 48/48 | 43/48 | 23.6 px |
| media `overflow: hidden` instead of `clip` | 48/48 | 48/48 | 0.006 (media is `width: 100%`, so the min-size difference never bites here) |
| grid pinned to `180px 2fr` (sanity: the diff is sensitive) | 8/48 | 7/48 | 278 px |

So the only rules that matter for this fixture are border-box and keeping
chrome out of layout; `min-width: auto` matters as soon as content is wider
than its track (390 px).

## RTL

The CSS surface flips via an inherited `direction: rtl` on the root; taffy gets
the root's direction on every node (it does not inherit). Both mirror the same
way, including two traps that are NOT mirrored in either engine because the
whitelist only has physical keys:
- `form-actions` `marginLeft: "auto"` pushes to the physical left: in RTL the
  actions sit right next to the select instead of at the far end.
- `media-badge` `right: 8` stays top-right in RTL.
The whitelist needs logical keys (`marginInlineStart`, `insetInlineEnd`,
`paddingInline*`) if vApps are meant to be direction-agnostic.

## Typing test

`bun scripts/vapp-spike/typing.ts` (Playwright `pressSequentially` with a 5 ms
delay = real key events through CDP, 40 chars in 290-400 ms), 3 invocations ×
3 runs: **9/9 PASS**, field and the "host:" echo line both equal
`abcdefghijklmnopqrstuvwxyz0123456789ABCD` after 400 ms. The echo is applied
only when its revision is still the latest, so stale echoes never clobber.

## Screenshots

`/tmp/vapp4-web-wide.png` (1440×960 @2x, full page) and
`/tmp/vapp4-web-narrow.png` (390×844 @3x, mobile emulation). Wide shows the
two-column grid (nav spans both rows), narrow the single column with the
footer pills wrapping and the flex-demo wrapping into 2+2.

## Where CSS and taffy disagree (or could)

1. `overflow: hidden` semantics (scroll container → min size 0 in CSS, not in
   taffy's Clip). Mapped to `clip` on web; natives have no such notion, the
   core decides.
2. Borders and any chrome: CSS borders take layout space. Card/field chrome
   must be paint or inside the measured leaf size.
3. `box-sizing`: the browser default is content-box; the core is border-box.
4. LayoutUnit quantization (1/64 px): harmless, well under a device pixel.
5. Real mode (not geometry) naturally differs: real fonts, TipTap's markdown
   block adds paragraph/list margins (visible trailing space under the
   markdown in the screenshot), Badge is 16 tall vs the contract's 20, Pill
   `sm` is 24 vs 28, ListRow is restyled to 32. Those are painter choices, not
   layout-engine disagreements.

## Whitelist keys that were awkward on web

- `@media (min-width)` must become a container query plus an extra wrapper
  element; a plain `@media` would read the window, not the surface.
- `:pressed` has no CSS equivalent; `:active` is close (pointer down) but
  differs for keyboard activation and touch-cancel.
- `gridTemplateAreas` as a string array needs quoting; `gridArea` in the core
  also sets row+column placement, CSS does the same natively.
- `paddingHorizontal/Vertical`, `marginHorizontal/Vertical` are RN names with
  no CSS property; they expand to physical sides.
- `aspectRatio` number vs `"16/9"` both work in CSS; fine.
- `overflow` values need the clip mapping above.
- Colour tokens: `$semantic.*` has no CSS variable today.
- Pressed visual: applied locally via CSS `:active`, no relayout (allowed by
  LANES.md since `:pressed` only changes opacity here).
