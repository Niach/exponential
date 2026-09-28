# FINDINGS · desktop (gpui, shared frames)

Screen: `apps/desktop/crates/ui/src/vapp_spike.rs`, `Screen::VappKitchenSink`, `EXP_DEV_SCREEN=kitchen-sink`, bench via `EXP_DEV_VAPP_BENCH=200`. Release build in `target-vapp4`. `cargo test -p ui vapp` (5) and `cargo test -p ui navigation` (21, incl. the parse test) pass.

## Decision: shared frames, not gpui styles

gpui's `Styled` cannot express `grid-template-areas` or mixed track lists (its taffy module is private and its converter drops them), so the screen calls `vapp_spike::layout` with a gpui text-system `Measure` and paints one absolute `div` per `PlacedNode` in pre-order. Geometry parity with the natives is therefore by construction; gpui only rasterises.

## Numbers (release build, M-series MacBook Pro)

| tree | pass | measure calls | wall |
|---|---|---|---|
| kitchen sink, 48 nodes | cold (first frame, shaping cache empty) | 407 | 1.4 to 3.1 ms |
| kitchen sink | first pass at a new width (1148 / 508) | 268 / 274 | 245 / 237 µs |
| kitchen sink | steady (nothing changed) | 0 | 5 to 6 µs |
| bench, 203 nodes | cold | 1942 | 3.0 ms |
| bench | first pass at a new width (1148 / 508) | 1778 / 1734 | 1.1 ms |
| bench | steady | 0 | 19 µs |

Press-down on a button = 43 measure calls, 324 µs; release 48 µs. A relayout per press is fine here. taffy asks about 12 measure calls per leaf on a cold pass; gpui's line-layout cache absorbs the repeats, which is why the first pass at a width costs a quarter of the cold pass. A native measure WITHOUT a cache would pay the cold price every time.

## Live check

Signed in with a demo token against `http://localhost:5173` (the self-signed `https://localhost:3000` fails gpui's TLS validation). 1440-wide window = 1148 surface: two-column named-area grid, absolute LIVE pill, 16:9 media, wrap row. The app's 800 px minimum window size overrides `EXP_WINDOW_SIZE=700x…`; at 800 the surface is 508 and the grid stacks (breakpoint honoured). Screenshots: `/tmp/vapp4-desktop.png`, `/tmp/vapp4-desktop-narrow.png`, `/tmp/vapp4-desktop-bench-{1440,700}.png`, `/tmp/vapp4-desktop-typed.png`.

## Typing test

Pass, both ways. Unit test: 40 characters in, the field ends with all 40, 1 echo applied and 39 dropped as stale. The test cannot use gpui synthetic keystrokes (a focused gpui-component `Input` panics in a headless test window, it asks for a native NSView), so each character goes through the input handler `replace_text_in_range`, the call real keyboard input makes. Live: `cliclick` typing gives the same `latest: 40, applied: 1, dropped: 39`.

## Divergences and caveats

- gpui breaks lines at `/`, so "r/selfhosted" wraps mid-word in the bench tree. CSS does not; line breaks differ from the web.
- `WrappedLineLayout::width()` returns min(wrap width, unwrapped width); `shape_text` is only used for heights, widths come from `shape_line`.
- Controls follow the lane contract, not the app's exact sizes: button side padding 12 (app: 16), pills 28 tall (app: 24/32/36), listrow gets an 8 px gap between title and meta.
- The markdown height estimate was 7 px short until a gap per block was counted (the last bullet touched the progress bar).
- `boxShadow` is not painted. RTL not exercised on desktop (the core test covers geometry).
- AccessKit roles and labels compile on every leaf; no screen-reader run.
- Caption text on the yellow demo box is hard to read: the fixture gives no text colour (a whitelist/authoring note, not a layout one).
- Harness note: grab the window by the launched process's window id, not by process name; the developer's own IDE answers first under System Events.
