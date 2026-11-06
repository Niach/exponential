# Changelog: `eui-gpui` (desktop painter, not yet published)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- #VAPP-258 step 5: `HostPlugin::markdown_options` (`MarkdownOptions`, also `HostPolicy::markdown_options`): hard breaks, the chat rhythm (`density: chat` recipes), selection off, streamed text (`parse_with`, `parse_streaming`, a per-text memo). A document of 24+ blocks lays out and paints only the blocks crossing the clip (`LazyMdBlock`), the measured height unchanged. A copy across a chip gives its label; a chip box spans its whole placeholder (no stray gap after it).
- #VAPP-258: Markdown host hooks: inline chips on `text_links` segments (`TextLink.chip`, measured as painted), `HostPlugin::image_size` / `image_pressable` / `on_image_press` for block images and `HostPlugin::attachment` for one-link video, audio and file blocks (also on `HostPolicy`).

## 0.16.3 (2026-11-06)

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
