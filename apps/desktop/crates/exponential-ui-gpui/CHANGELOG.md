# Changelog: `exponential-ui-gpui` (desktop painter, not yet published)

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- `SurfaceView`: the gpui painter. It measures text in-process and paints one absolutely positioned div per node at the core's frames.
- Video and AudioPlayer hand their policed `src` to the system player on a press (gpui has no media pipeline): http(s) through `open_url`, a header-carrying or `data:` request fetched within `media.limits` into a temp file for the new `HostPlugin::open_media_file`; a denied src stays inert.
- Overlays, host-owned inputs, charts, markdown, accessibility order and the conformance runner.
