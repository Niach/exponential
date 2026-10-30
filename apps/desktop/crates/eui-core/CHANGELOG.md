# Changelog: `eui-core` (Rust core)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

## 0.2.1 (2026-10-30)

- Version-only bump with the SDK (eui-gpui 0.2.1: per-cluster font fallback, YO-30); unchanged from 0.2.0.

## 0.2.0 (2026-10-30)

- Renamed (EXP-1438): the crate `exponential-ui` (lib `exponential_ui`) is now `eui-core` (lib `eui_core`); `exponential-ui` 0.1.1 stays on crates.io, superseded.
- VAPP-161: package format v2 mirrored from the TS reference: `validate_package_with` (`formatVersion`, `records:<kind>` bindings, `tools`, versioned `extensions`), `host::tools` (`validate_tools`, `tool_consent`, `tool_consent_key`, `tool_call`), `host::signature` (`canonical_json` = RFC 8785, `package_file_signed_bytes`; no crypto dependency, the host verifies), `host::record_store::parse_records_source`; `HOST_CONTRACT_VERSION` 3; `fixtures/host-package.json` replayed.
- VAPP-162 (round 5): the catalog's new components and props (MessageBubble, CallSurface, Plan, QrCode, QrScanner; Segmented `orientation` + `option.count`, VoiceWave `variant: bars`, Button `hold`), `qr_code` (the QR matrix), `segmented` (the vertical rail and its badges), `hold` (the hold gesture's events), `voice_wave::voice_wave_bars`, the `type.family.serif` token.
- NEW-77: `voice_wave` (the VoiceWave geometry and the child rule).
- VAPP-122: `host::records` (`validate_record_schema`, `validate_record`, the reserved row keys), the `records` block in `validate_package`, `source_with_params` (the `bind` action's params encoding) and the contract's `package.records` / `sources.paramsEncoding`; the host fixtures replay the new `sourceParams` and `records` cases. The catalog's 33rd built-in is `bind`.

## 0.1.1 (2026-10-23)

- Version-only release: the SDK's npm packages moved from the `@exponential-at` scope to `@exponential_app` and every package shares one version. The crate is unchanged from 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-146: a Select trigger shows the option holding `""` (an unseeded value reads as `""`), and pressing the option that holds the current value only closes the list; `tests/interactions_fixture.rs` replays `select`.
- The reducer (core and basic catalogs, macros, extensions, validation) and the bind pass (bindings, functions, `$string`).
- The taffy layout engine: batched text measurement, overlay layers, windowed lists, scroll containers, sticky nodes and breakpoints.
- The embedded catalog, macro table and built-in themes. It replays the shared fixtures byte for byte.
- VAPP-106: the embedded catalog is the runtime one (no descriptions or examples); the `descriptions` feature (off) adds `catalog::descriptions()`; the default-on `builtin-themes` feature holds the non-default resolved themes, so `--no-default-features` embeds only `exponential`.
- VAPP-124: on Android and iOS the stack-growth check reads the thread's stack limit from pthread (`stack.rs`), so passes run on the host's own stack and only switch to a heap segment when it runs low (ART refused JNA upcalls from the segment with StackOverflowError).
- VAPP-132: a restyle pass memoises the node-independent style work (template items share it), `get_pointer` reads without allocating, template keys skip the duplicate walk when they are distinct, list heights hash with FxHash and the measure memo is keyed per slot (forgetting a leaf no longer scans every leaf).
