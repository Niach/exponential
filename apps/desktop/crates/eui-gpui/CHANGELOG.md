# Changelog: `eui-gpui` (desktop painter, not yet published)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- gpui revisions: zed `cc053a4a` (git) → `gpui-pre` 0.3.11 (crates.io, zed `f816e1f`); gpui-component `da4f9369` (git) → 0.9.0 (crates.io, longbridge/gpui-kit); rust 1.96.0 → 1.98.1. A breaking change for consumers: take gpui as `gpui = { package = "gpui-pre", version = "=0.3.11" }` and `gpui-component = "=0.9.0"`, drop the ztracing `[patch]` (zed relicensed it Apache-2.0) and the `cargo update --precise` step (README "gpui revisions"). Ships as the SDK's next minor.
- One animation grid (#LOGS-3): every `with_stepped_animation` and `repaint_on_next_step` repaint lands on a tick of ONE 33.3 ms grid the app shares, and a loop's start moves back onto its step's line, so loops at different steps (a 33 ms spinner, a 90 ms sprite, a 100 ms label) share frames instead of adding up: a window redraws at most 30 times a second whatever animates. New `PULSE_STEP` (every second tick, 15 fps) for a long-lived opacity pulse. A looping keyframe `animation` (infinite iterations: a `spin`, a `pulse`) repaints on the grid instead of requesting every frame; a finite one keeps every frame. Each stepped element opens an `anim.step` trace span labelled with its id while tracing listens.

## 0.20.0 (2026-11-08)

- Scrollbars (#EXP-1629): a scroll container's overlay scrollbar is gpui-component's own `Scrollbar` over the core's offset (`view/scrollbar.rs`), so an eUI surface in the IDE shows exactly the IDE's scrollbars: the theme's thumb, width and inset, shown when the offset changes (wheel, `scrollToIndex`, a drag), fading out after gpui-component's idle delay, wider under the pointer, draggable. It replaces the EXP-1555 bars (6 px at 2 px, shown on hover and for 1 s after a scroll). No catalog or prop change.
- Tabs `swipeable` (#FEED-200, `view/swipe.rs`): a two-finger trackpad swipe pages the Tabs like a finger. gpui's phased `ScrollWheelEvent`s (`ScrollDelta::Pixels`, `TouchPhase` Started/Moved/Ended) over the content send `swipe {phase: start|move|end, offset, velocity}` once `swipe_lock` calls the travel horizontal (vertical = the page's); a gesture starting over a node `swipe_blocks` names (Table, Chart, Slider, a horizontal scroller…) stays that node's; the fling's momentum never pages again; a mouse wheel (`Lines`) never swipes; a gesture silent for 500 ms settles. The panels and the thumb paint the core's `transform` / `transition` as any node does.
- Chart ticks, value labels and the tooltip show the core's labels in the chart's `valueFormat` (#FEED-205).
- Pixel snapping (EXP-1629): `SurfaceViewOptions::pixel_snap` (on by default) snaps a surface to whole device pixels at the window's scale factor like gpui's layout, so an eUI document paints the same pixels as the gpui view it replaces. A view under a measure override never snaps; the conformance dump turns it off (it compares the core's fractional frames with the web). While snapping, a text's measured width rounds up to a whole pixel, as gpui's text element does.
- A `native: true` style no longer applies to a Switch track: genUI's Switch is the IDE's since #EXP-1745, so the borrowed `controls::web_switch` path is gone and a Switch always paints the eUI look (#EXP-1629).
- `binding` (#EXP-1629): the native host binding, a host's own screen drawn as a document. `Binding<S>` owns the screen's state and writes its projection at one data-model path (re-projected on `observe`d entities, an unchanged projection writes nothing); a named action or a committed text edit maps to a `Mutation` (applied at once, committed off the main thread, reverted and toasted through the host's `Toasts` on failure; a `key`ed failure never reverts a newer edit; a commit older than the field's last is dropped). `BindingHost` routes the document's actions and inputs to it and everything else to the host's own plugin.

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- Round 9 parity: Markdown's inline code paints the `Markdown/code` recipe in its own type over a placeholder (a span wider than its line stays a breaking mono run), a disc at every list depth, an ordered marker one space before the text (`fixtures/markdown-inline.json` `documentPaint`).
- Canvas (round 8, `docs/round-8-contract.md` §1): a Canvas leaf (`canvas::CanvasPlayer`, `view/canvas_leaf.rs`) compiles `{size, shapes}` in the engine (recompiled on a `size` / `shapes` change, resampled with the theme's slots on a theme or mode change, no playback), rasters the shapes with tiny-skia only when the box size, the fit or the slots change, and sets every `text` op natively ABOVE the image: the anchor through fit × transform, the font px scaled by it, `$type.family.sans` / `.mono` at the CSS weight, physical align, the top / middle / alphabetic baseline from gpui's font ascent and descent, clipped by the box only. An img named by `label` (decorative without one), never a tab stop or a button. With `pointer: true`: `press {x, y}`, `move {x, y}` at most once per frame step (the last held move fires at the step) and `leave`, in drawing units (`canvas::to_drawing`). `SurfaceView::canvas_error` / `canvas_list` / `canvas_texts` / `canvas_rasters` for tests and automation.
- The Motion painter takes display list v2: `text` ops (none from a Motion today) are set natively over the raster by the same code (`MotionPlayer::texts`).
- Checkbox `indeterminate` (round 8 §2): a mixed box paints the checked look with the `Checkbox.check.indeterminate` dash (`ui-minus`; a `native` box falls back to it, gpui-component has no mixed state) and reads `Toggled::Mixed` (mixed wins over checked; `SurfaceView::toggled_state`); the press semantics are the core's.
- Progress `valueText` (round 8 §3): the `value` part is shaped with tabular figures (`tnum`) in the measurer and the painter (`measure::tabular_text`, keyed on the macro's `<id>.header.value`).
- Stepper `collapse` / `@container` (round 8 §4): through the core's layout pass, no painter change.

- An app's own file as a src (#VAPP-271): `HostPlugin::app_files() -> Option<Arc<dyn AppFiles>>` (default `None`; `AppFiles::read(file_id) -> Option<Vec<u8>>`, any `Fn(&str) -> Option<Vec<u8>> + Send + Sync`); an `app-file:<fileId>` Image / Avatar / poster / Markdown image src reads it off the main thread under the image limits and the asset cache (`media::image_key`, `MediaKey::app_file`); no app files, a bad id or a missing file = the missing-image fallback.
- A call's answer (#VAPP-271): `run_package_action` gates and runs a `call` step with a `result` when the plan reaches it (before the commit; a refusal or a failure is the action's failure, nothing lands) and runs the action again over the same snapshot with the answer as `results.<name>`.

## 0.18.0 (2026-11-06)

- Tree (#VAPP-285): the arrows, Home and End on a treeitem go to the core's tree keys.
- A foldable Row (#VAPP-284): ArrowRight / ArrowLeft on a node with `expanded` go to the core's fold keys.

## 0.17.0 (2026-11-06)

- #VAPP-258: two gpui host extras for an app's own Markdown: `HostPlugin::chip_icon` paints an inline chip's icon (a status glyph in its tint), `tasks_pressable` / `on_task_toggle` make task boxes pressable (the host rewrites its text); `markdown_hooks_version` memoizes a leaf's laid-out blocks (`paint::markdown_memo`), `markdown_visible_only` builds only the blocks near the host's viewport, `markdown_window_selection` joins the text units to the host's window-level selection (gpui-base, `paint::markdown_selection`).
- An inline DatePicker paints the core's in-flow month grid; a marked day draws its event dot (VAPP-282).
- Tabs `variant: strip` (#VAPP-279): a strip tab press becomes a reorder drag past `tabDragThreshold` (the press is spent); Ctrl/Cmd+Shift+Arrow moves the focused tab.
- #VAPP-258 step 5: `HostPlugin::markdown_options` (`MarkdownOptions`, also `HostPolicy::markdown_options`): hard breaks, the chat rhythm (`density: chat` recipes), selection off, streamed text (`parse_with`, `parse_streaming`, a per-text memo). A document of 24+ blocks lays out and paints only the blocks crossing the clip (`LazyMdBlock`), the measured height unchanged. A copy across a chip gives its label; a chip box spans its whole placeholder (no stray gap after it).
- #VAPP-258: Markdown host hooks: inline chips on `text_links` segments (`TextLink.chip`, measured as painted), `HostPlugin::image_size` / `image_pressable` / `on_image_press` for block images and `HostPlugin::attachment` for one-link video, audio and file blocks (also on `HostPolicy`).
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).

## 0.16.3 (2026-11-06)

- #VAPP-258: two gpui host extras for an app's own Markdown: `HostPlugin::chip_icon` paints an inline chip's icon (a status glyph in its tint), `tasks_pressable` / `on_task_toggle` make task boxes pressable (the host rewrites its text); `markdown_hooks_version` memoizes a leaf's laid-out blocks (`paint::markdown_memo`), `markdown_visible_only` builds only the blocks near the host's viewport, `markdown_window_selection` joins the text units to the host's window-level selection (gpui-base, `paint::markdown_selection`).
- An inline DatePicker paints the core's in-flow month grid; a marked day draws its event dot (VAPP-282).
- Markdown `imageMaxHeight` is a host render option (`markdown.options`, a number or `null` = fit the width at the image's aspect, capped at the viewport; default 160), and Compose Markdown text follows the surface's font scale and folds at whole lines (`Markdown.foldHeight`).
- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). gpui paints the core's parts (no painter change).
- `HostPlugin::on_hosted_popups`, `SurfaceView::hosted_popup_element` / `dismiss_hosted_popup` (VAPP-281): hosted layers paint in the host's popup, never in the surface.
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` (VAPP-276): painted from the core's layers; the overlay suite replays the aligned and anchor-by-id cases.
- #VAPP-258 step 5: `HostPlugin::markdown_options` (`MarkdownOptions`, also `HostPolicy::markdown_options`): hard breaks, the chat rhythm (`density: chat` recipes), selection off, streamed text (`parse_with`, `parse_streaming`, a per-text memo). A document of 24+ blocks lays out and paints only the blocks crossing the clip (`LazyMdBlock`), the measured height unchanged. A copy across a chip gives its label; a chip box spans its whole placeholder (no stray gap after it).
- #VAPP-258: Markdown host hooks: inline chips on `text_links` segments (`TextLink.chip`, measured as painted), `HostPlugin::image_size` / `image_pressable` / `on_image_press` for block images and `HostPlugin::attachment` for one-link video, audio and file blocks (also on `HostPolicy`).
- Markdown parses the same on all five renderers (`markdown-render.json`, every golden case passes; VAPP-258), and a number field seeded with a value selects it on focus so Save runs once (VAPP-208).

## 0.16.2 (2026-11-06)

- The icon list follows the app: `power` and the eDE icons (`ede-window`, `ede-lock`, `ede-power`, `ede-suspend`, …) are new.

## 0.16.1 (2026-11-05)

- #VAPP-253: under `lift_layers` a top or bottom Drawer spans the window (`Surface::set_layer_viewport`) instead of keeping the surface's width.

## 0.16.0 (2026-11-05)

- Released with the SDK at 0.16.0: the records store moves behind a `RecordStore` handle (VAPP-212; **breaking** for the Swift and Compose `RecordsQuery.value(rows…)` and the FFI `records_value_json`), RecordForm date-time pickers and an owner's Author field (VAPP-245), and text that binds in Dialog, Alert and Chip (VAPP-206).

## 0.15.4 (2026-11-05)

- The open libraries are the Exponential SDK: genUI, Motion and Apps name the skills and docs (`exponential-ui` skill = `exponential-genui`; docs under /docs/sdk/); package names and tags stay. The amber warning colour is `semantic.yellow` (EXP-1695).

## 0.15.3 (2026-11-05)

- The icon list follows the app: `thread-stories` is gone, `thread-results` and `nav-needs-you` are new (FEED-191).

## 0.15.2 (2026-11-04)

- #VAPP-252: `SurfaceViewOptions::lift_layers` / `HostOptions::lift_layers` (default off): a modal layer hangs from the window, its scrim over all of it; `view::init` keeps Tab in the modal's focus trap under a host's window-wide Tab binding.

## 0.15.1 (2026-11-04)

- No changes of its own; released with the SDK (EXP-1691's fixes landed in the core).

## 0.15.0 (2026-11-04)

- No changes of its own; released with the SDK (format 4 and in-app links landed in the core).

## 0.14.0 (2026-11-04)

- Repeating animations share one 30 fps redraw grid instead of each scheduling its own frames (#1874).
- EXP-1674: `Row.href` / `Button.href` presses open the URL.

## 0.13.0 (2026-11-03)

- Switch gains `size` (`default` | `compact`): the compact switch is a 28×16 track under a 12px thumb, as on the other renderers (#1853).

## 0.12.0 (2026-11-03)

- No changes of its own; released with the SDK (U1 screens, R8 client migrations and R7 components() landed in the core).

## 0.11.0 (2026-11-03)

- No changes of its own; released with the SDK (R6 transitions and R8 record migrations landed in the core).

## 0.10.0 (2026-11-03)

- No changes of its own; released with the SDK (refs + expand, history and files landed in the core).

## 0.9.0 (2026-11-03)

- No changes of its own; released with the SDK (computed fields and the sorted install issues landed in the core).

## 0.8.0 (2026-11-03)

- VAPP-200: `HostOptions.read_only` (every host surface read-only) and `SurfaceViewOptions.settings` with `read_only`: every writing control paints disabled. Conformance suite `read-only`.

## 0.7.1 (2026-11-03)

- EXP-1644: a transparent container hands its opacity down to its children instead of fading its element, so a dimmed Button inside a dimmed Form flattens to the total dim like web.

## 0.7.0 (2026-11-03)

- VAPP-160: a moving `VoiceWave` asks for the next frame from a paint-time ticker over its box, only while that box is inside the visible content (offscreen or clipped away: no frame requests); the view prunes animation clocks of removed, rebuilt or hidden nodes on every pass (`SurfaceView::animation_clocks`, a test aid). No visual change.
- VAPP-209: pressing a package action no longer panics (`cannot read SurfaceView while it is already being updated`): the host function call is deferred out of the surface view's update; `SurfaceView::host_plugin` (a regression test drives the press path).

## 0.6.0 (2026-11-02)

- VAPP-194: `ExponentialHost::call_function` runs `actions.<name>` as the surface's package action (`run_package_action`): the plan over one snapshot, every host call's gate and consent first, the writes as ONE `records.batch`, then the sets, then the calls; a failure sends `ACTION_FAILED` and calls `HostOptions::on_action_failed` (`ActionFailure`). New `HostOptions` fields `me`, `now`, `zone` (`ZoneFn`), `on_action_failed`.
- EXP-1589 / EXP-1624: parity with the web renderer. A cartesian Chart maps its nice-tick range (the top tick label no longer rides into the title); a disabled Button, Toggle or Composer send button dims as one layer (its label never shows its fill through); a disabled text field dims once and is inert; the FileUpload dropzone glyph fills its part; a ZoomableImage that has not loaded draws a placeholder (the image glyph over the alt, on a muted tint) and no pill instead of a broken-image glyph.

## 0.5.0 (2026-11-02)

- No changes of its own; released with the SDK (the records query and expressions landed in the core).

## 0.4.0 (2026-11-02)

- EXP-1604: the Motion native (`motion::MotionPlayer` + the view's Motion leaves): the source compiles in `eui-motion` (dialect `layers`, inline `lottie`, a `src` .json/.lottie through the media loader), theme `$color` slots + `$mode` (reloaded on theme/mode change), painted by the `raster` (tiny-skia) into one reused pixmap at device size → `RenderImage` only when the frame changes (old images leave the atlas); the shared playback clock, timer-stepped at ≤ 60 fps; poster under reduced motion, `autoplay: false`, off-screen and in a background window (`set_pause_motion_in_background`); `play: false` freezes; ≤ 8 play per surface; `fit`; an img named by `label`. Pixel parity against the raster references (`tests/motion.rs`).
- EXP-1586: the ZoomableImage native (`controls::ZoomableImage`, also a standalone gpui view for the IDE's lightbox): ⌘/ctrl-wheel and trackpad pinch zoom about the pointer, a plain wheel pans with room, drag with rubber band, spring-back and momentum, double click, `+ = − 0 1`, delayed single-click `tap`, THE zoom pill with its percent Menu, `controls` auto/always/none, `zoom {scale, atFit}` writing the bound `scale`; all maths through `eui_core::zoom`. The `ui-fullscreen` glyph.

## 0.3.0 (2026-10-31)

- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- YO-30: font fallback per grapheme cluster (`fallback::cluster_runs`, applied wherever the painter and the measurer shape): a cluster the run's face cannot shape whole (Nunito lacks U+0329 in `/ˈʃpɛtsl̩/`) shapes whole in the first fallback face that covers it, so combining marks attach to their base instead of painting as a free-standing glyph (macOS). Markdown spans and code tokens get the sans fallback chain too. `letterSpacing` follows clusters (a mark adds none), as CSS does. `fixtures/text-clusters.json` replays it.

## 0.2.0 (2026-10-30)

- Renamed (EXP-1438): `exponential-ui-gpui` (lib `exponential_ui_gpui`) is now `eui-gpui` (lib `eui_gpui`), on the `eui-v<version>` tag.
- VAPP-162 (round 5): QrCode (the core's matrix, dark modules on the lighter plate), QrScanner (viewfinder chrome, the new `HostPlugin::qr_scanner_feed` camera hook + `SurfaceView::qr_scanned`), VoiceWave `variant: bars`, the vertical Segmented and the rail with count badges, Button `hold` (pointer, keys, blur through `Surface::hold`), `ui-serif` mapped to a system serif; `SurfaceView::node_font` inspection aid.
- EXP-1429: the gpui revision policy for consumers (README "gpui revisions": the zed + gpui-component pins, the ztracing patch to copy, the bump and announcement rules, why there is no crates.io release); `test.yml` gates the pins against the workspace and the gpui guide.
- VAPP-122: `ExponentialHost::bind` runs the `bind` action (`{path, source, params?}` through the router like a server's bindDataModel); subscriptions are keyed by path, so a re-bind cancels the earlier one at that path. The conformance suite replays the new host cases.

## 0.1.1 (2026-10-23)

- Version-only bump with the SDK (the npm packages moved to the `@exponential_app` scope); unchanged from 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-126 (review #1187): a `display: none` subtree (laid out at zero size, clipped) is no tab stop: `focus_order` drops it, as on the web.
- `SurfaceView`: the gpui painter. It measures text in-process and paints one absolutely positioned div per node at the core's frames.
- Video and AudioPlayer hand their policed `src` to the system player on a press (gpui has no media pipeline): http(s) through `open_url`, a header-carrying or `data:` request fetched within `media.limits` into a temp file for the new `HostPlugin::open_media_file`; a denied src stays inert.
- Overlays, host-owned inputs, charts, markdown, accessibility order and the conformance runner.
- `controls::SteppedAnimationExt::with_stepped_animation` (EXP-1325): gpui's `with_animation` on a frame budget — it repaints on a timer at the animation's own step (a sprite's frame, `SMOOTH_STEP` = 30 fps for a smooth loop) instead of every frame, since gpui repaints the whole window per animation frame at the display's rate. The painter's spinner, skeleton pulse and live-dot ping, and the glass `spinner`, use it.
