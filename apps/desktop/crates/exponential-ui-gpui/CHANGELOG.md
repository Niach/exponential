# Changelog: `exponential-ui-gpui` (desktop painter, not yet published)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- VAPP-126 (review #1187): a `display: none` subtree (laid out at zero size, clipped) is no tab stop: `focus_order` drops it, as on the web.
- `SurfaceView`: the gpui painter. It measures text in-process and paints one absolutely positioned div per node at the core's frames.
- Video and AudioPlayer hand their policed `src` to the system player on a press (gpui has no media pipeline): http(s) through `open_url`, a header-carrying or `data:` request fetched within `media.limits` into a temp file for the new `HostPlugin::open_media_file`; a denied src stays inert.
- Overlays, host-owned inputs, charts, markdown, accessibility order and the conformance runner.
- `controls::SteppedAnimationExt::with_stepped_animation` (EXP-1325): gpui's `with_animation` on a frame budget — it repaints on a timer at the animation's own step (a sprite's frame, `SMOOTH_STEP` = 30 fps for a smooth loop) instead of every frame, since gpui repaints the whole window per animation frame at the display's rate. The painter's spinner, skeleton pulse and live-dot ping, and the glass `spinner`, use it.
