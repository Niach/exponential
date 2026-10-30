# Changelog: `eui-gpui` (desktop painter, not yet published)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

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
