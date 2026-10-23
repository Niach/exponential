# Changelog: `@exponential-at/ui-react`

All notable changes to this package. The SDK packages share one version and
release together from a `ui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## 0.1.0 (unreleased)

First release.

- VAPP-152: the node sheet is painted in `<style>` batches (`nodeSheetBatches`): a streamed `updateComponents` appends one with its new nodes' rules and leaves the others' elements and text alone, so Chromium restyles the new card instead of the whole surface. `share` comes from `@exponential-at/ui/web` (one copy for React and Solid).

- VAPP-141, VAPP-130, VAPP-137, VAPP-139, VAPP-140: a change re-renders only the nodes it reaches. The surface context keeps its identity when only the data, the breakpoint, `data-xq` or the mode change (`SurfaceContextValue.data`/`breakpoint`/`xq`/`mode` are live getters, `subscribe` announces a change, `deriveContext` derives one); each node re-renders when a value its bindings read is new. A re-reduced tree shares its unchanged nodes, the two stylesheets are rewritten only when their text changes, a template whose keys all change mounts as one block, a windowed list reads its extents once per measurement. On the benchmark Mac: a streamed message 13.6 → 2.4 ms, a mode switch 32 → 7.5 ms, a resize 28 → 1.2 ms, `scrollToIndex` on 100,000 rows 28 → 3.8 ms, 10,000 template items 328 / 406 → 148 / 133 ms.
- VAPP-146: a Select option whose value is `""` is an ordinary option (it threw); `fixtures/interactions-round1.json` `select`.
- An Avatar without a picture paints its initials with the first frame.

- The primitives' class strings come from `@exponential-at/ui/web/primitives` (FEED-131), the one source the Solid twins share; the exports keep their names.
- New primitives: `table`, `command` (no cmdk) and `toast` (no sonner, on the toast stack contract).

- `ExponentialSurface` + `useSurface`: the reference renderer, painting with real CSS on shadcn/Radix at the frames the Rust core computes (geometry-locked to taffy).
- Runtime themes, light/dark/system mode, locale and time zone, built-in string overrides, and density and contrast settings.
- Host-owned inputs, overlays on Radix portals, extension registration and the host plugin API.
- Runs the full conformance suite in headless Chromium.
- Trace lines, the Text shimmer (reduced motion: plain) and CodeBlock's destructive/sm looks; an `ansi` block copies without its escapes.
- The stylesheet, bind pass and pure helpers come from `@exponential-at/ui/web` (re-exported under the same names), shared with every web renderer.
