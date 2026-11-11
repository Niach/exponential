# Changelog: `exponential-ui-ffi` (UniFFI facade, not published)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- `FfiTextStyle.font_variant_numeric` (`tabular-nums` = shape with `tnum`; `None` = the font's own figures). Bindings regenerated.
- #EXP-1629: `set_popup_bounds(x, y, width, height)`: the host's window around the surface, in surface coordinates, that anchored popups fit into (width or height <= 0 = unset, the surface).

## 0.21.0 (2026-11-09)

- #EXP-1629: `FfiVisual.background_clip` (`padding-box` = the background paints inside the border; `None` = under it).

## 0.20.0 (2026-11-08)

- FEED-200: `swipe_lock(dx, dy)`, `swipe_offset(index, count, offset, width, rtl)`, `swipe_settle(index, count, offset, velocity, width, rtl)`, `swipe_blocks(component, direction?, overflow_x?)`: the pager's pure rules for the Swift and Kotlin painters (and FEED-201's app-screen pager).

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- A call's answer (#VAPP-271): `run_package_action_json` takes a trailing `results_json` (`{<name>: answer}`, defaulted to none) and may answer `{"pending": {step, name, args, result}}`. Bindings regenerated.

## 0.18.0 (2026-11-06)

- #VAPP-288: `Surface.assign_data(path, value_json)` (a programmatic write: a package action's `sets`) and `Surface.write_epoch()`; an action's `set` bumps the epoch too, and a painter's focused field adopts a changed bound value when the epoch moved (the form that clears itself after Save). Bindings regenerated.
- Tree (#VAPP-285): `event(index, "key", {key})` on a treeitem moves the focus (no new API).
- A foldable Row (#VAPP-284): `event(index, "key", {key})` takes the fold keys on a node with `expanded` (no new API).
- `FfiSettings.hosted_popups`, `FfiLayer.hosted` / `align` (VAPP-281, defaulted).
- `place_overlay_aligned(..., side, align)` (VAPP-276): the placement rule with `start | center | end` along the side.

## 0.17.0 (2026-11-06)

- **Package format 5** (#VAPP-260, breaking: a host reads exactly 5): `capabilities: [{id, why, origins?}]`, the capability manifest the Engine's broker and the bundler read (`catalog/host.json` `package.capabilities`, `validatePackageCapabilities` / Rust `host::capabilities`, `docs/package-format.md` "Capabilities"); `fixtures/host-router.json` gains `acme.capabilities` and four invalid packages.
- The inline DatePicker month grid and its `month` event through the facade (VAPP-282).
- Tabs `variant: strip` (#VAPP-279) through the core (no new API: `drag` / `key` events on the tab).
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).

## 0.16.3 (2026-11-06)

- The inline DatePicker month grid and its `month` event through the facade (VAPP-282).
- Markdown parses the same on all five renderers (`markdown-render.json`, every golden case passes; VAPP-258), and a number field seeded with a value selects it on focus so Save runs once (VAPP-208).

## 0.16.2 (2026-11-06)

- The icon list follows the app: `power` and the eDE icons (`ede-window`, `ede-lock`, `ede-power`, `ede-suspend`, …) are new.

## 0.16.1 (2026-11-05)

- #VAPP-253: `Surface.set_layer_viewport(width, height)`, the core's layer viewport; bindings regenerated.

## 0.16.0 (2026-11-05)

- VAPP-212: `RecordStore`: one app's rows behind a handle (`applyJson`, `valueJson`, `count`), so a query no longer has to carry the rows across the FFI.
- **Breaking** VAPP-212: `records_value_json` is gone; use `RecordStore`.

## 0.15.4 (2026-11-05)

- The open libraries are the Exponential SDK: genUI, Motion and Apps name the skills and docs (`exponential-ui` skill = `exponential-genui`; docs under /docs/sdk/); package names and tags stay. The amber warning colour is `semantic.yellow` (EXP-1695).

## 0.15.3 (2026-11-05)

- The icon list follows the app: `thread-stories` is gone, `thread-results` and `nav-needs-you` are new (FEED-191).

## 0.15.2 (2026-11-04)

- No API of its own; carries the core's `$string` checks, Markdown text links and narrow-phone layouts.

## 0.15.1 (2026-11-04)

- No changes of its own; released with the SDK (EXP-1691's fixes landed in the core).

## 0.15.0 (2026-11-04)

- **Breaking** (through `eui-core`) #EXP-1674: `validate_package_json` requires `formatVersion: 4`; `decide_url_json` reads `linkBase`.

## 0.14.0 (2026-11-04)

- No API of its own; carries the core's U2/U3 macros and the record fill (VAPP-217, VAPP-232).

## 0.13.0 (2026-11-03)

- No API of its own; carries the core's calendar views (VAPP-232).

## 0.12.0 (2026-11-03)

- VAPP-218 (eApps U1): screens (`screens` in the package, the host paints the navigation), the `app.navigate` action and the `@media (form-factor: phone|wide)` condition (`SurfaceSettings.form_factor`).
- VAPP-233: `migrate_record_json`, `migration_refusal_json`.

## 0.11.0 (2026-11-03)

- VAPP-222: `transition_refusal_json`, `transitions_from_json`, `validate_record_transitions_json`; `records_value_json`'s context takes `transitions` + `role`.

## 0.10.0 (2026-11-03)

- The committed UniFFI bindings match the core again: 0.9.0's Swift package reported `apiChecksumMismatch` at init (`records_value_json` after Q3). Use 0.10.0, not 0.9.0.

## 0.9.0 (2026-11-03)

- No API of its own; carries the core's computed fields (VAPP-214) and sorted install issues (VAPP-201).

## 0.8.0 (2026-11-03)

- No changes of its own; released with the SDK (read-only surfaces, install-time validation and the stricter checker landed in the core).

## 0.7.1 (2026-11-03)

- No changes of its own; released with the SDK (#1742's specimen look parity and #1744's neutral example names).

## 0.7.0 (2026-11-03)

- **Breaking** (through `eui-core`): `validate_package_json` requires `formatVersion: 3` and validates each kind's record `policy`; `dateAdd` out of range is an error (no overflow). No API change of its own.

## 0.6.0 (2026-11-02)

- **Breaking** (through `eui-core`): `parse_records_query_json(params_json, kind)` takes the kind only from `kind` (a `kind` param is a field filter); `validate_package_json` requires `formatVersion: 2`; `function_effect("harness.records.upsert")` is `host`, no longer `write`.
- VAPP-194: `run_package_action_json(package_json, name, params_json, data_json, me?, now_ms?, zone: HostZone?, max_steps?)` → `{plan}|{error}` (named so because `run_action_json` is the bind-time press), `apply_record_batch_json(rows_json, writes_json, records_json, me?, now, new_ids_json)` → `{rows, ids}|{error}` (ids past `new_ids_json` = `r<n>`), `function_effect(name)` → `pure|write|host`; `validate_package_json` reports effect and action issues. Bindings regenerated.
- VAPP-196: `run_package_tests_json(package_json)` → `[{name, pass, got}]` (`[]` without tests or with a malformed list); `validate_package_json` reports failing package tests as `/tests/<i>` issues. Bindings regenerated.

## 0.5.0 (2026-11-02)

- VAPP-191: `evaluate_expression_json(expr, vars_json, now_ms, zone: HostZone?, max_steps?)` → `{value}|{error}` and `check_expression_json(expr, types_json)` → `{type}|{error}`; `validate_package_json` reports expression issues. Bindings regenerated.
- VAPP-190: `parse_records_json(query_json, kind)` → `{query, issues}` and `date_bucket(date, bucket)`; `records_value_json` evaluates the new query keys (`fixtures/records-query.json` `jsonCases`, `bucketCases`).
- VAPP-189: `parse_records_query_json(params_json, kind)`, `parse_records_source_json(source)`, `records_value_json(rows_json, query_json, context_json)`: the records evaluator (`fixtures/records-query.json`).

## 0.4.0 (2026-11-02)

- EXP-1605: `motion_read_dotlottie_machine(data)`: a `.lottie`'s first state machine (manifest order) as JSON, "" without one.
## 0.3.0 (2026-10-31)

- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- Version-only bump with the SDK (eui-gpui 0.2.1: per-cluster font fallback, YO-30); unchanged from 0.2.0.

## 0.2.0 (2026-10-30)

- VAPP-162 / NEW-77: `voice_wave_json`, `voice_wave_bars_json`, `qr_matrix_json`, `segmented_badge_text`, `Surface::hold` + `is_held` for the Swift and Kotlin painters; bindings regenerated.
- VAPP-122: `source_with_params_json(source, params_json)` and `validate_record_json(schema_json, data_json)` for the Swift and Kotlin hosts; bindings regenerated.

## 0.1.1 (2026-10-23)

- Version-only bump with the SDK (the npm packages moved to the `@exponential_app` scope); unchanged from 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-126 (review #1187): `visibility_hidden` is also set for a `display: none` box (the core lays it out at zero size with its subtree): SwiftUI and Compose paint nothing of it, take no taps and keep it out of the a11y tree; a zero frame alone let an icon button's glyph draw past it on SwiftUI.
- A coarse `Surface` object: messages go in; frames, visuals, layers, lists and scrolls come out, with batched `Measurer` upcalls.
- Swift and Kotlin bindings, committed and drift-gated.
- `build-ios.sh` builds the xcframework: device, simulator and macOS slices, fat x86_64 + arm64, profile `mobile`, without the `cli` feature.
- `build-android.sh` builds the `.so` per ABI.
- VAPP-106: both scripts build the facade as ONE crate type (`cargo rustc --crate-type staticlib|cdylib`) so the `mobile` profile's LTO applies (cargo skipped it next to the rlib), `build-ios.sh` strips a copy of each archive before `lipo` (a device slice went from 44.5 MB to ~5 MB), and `EXPONENTIAL_UI_FEATURES` selects the core's feature gates. The profile keeps `panic = "unwind"` for the `catch_unwind` boundary.
- VAPP-131 / VAPP-138: `Surface.paint_at(indices)` returns each slot's visual plus, for a leaf, its text style in one call (a step's added and changed slots), instead of `visual(i)` + `text_style(i)` per slot.
- VAPP-134: the Kotlin binding skips uniffi's per-function API checksum calls at load (`omit_checksums`: ≈280 JNA calls); Swift keeps them (direct calls, microseconds) as the guard against a stale xcframework, and the contract version check stays on both.
- VAPP-131: `patch-swift-binding.pl` (run by `generate-bindings.sh` and `build-ios.sh`) makes the generated Swift readers load integers, byte runs and strings in place instead of copying them out of the buffer.
- VAPP-151: `run-binding-tests.sh kotlin` compiles again and runs in CI (the `compose` job; `swift` runs in the `swift` job): kotlin and JNA are pinned to the Compose package's version catalog and the compiler's classpath to its POM, each jar from the gradle cache at that exact version or fetched once from Maven Central (`sort | tail -1` had picked jna 5.6.0 over 5.17.0); `FacadeTest.kt` escapes `${'$'}schema` in its raw string.
