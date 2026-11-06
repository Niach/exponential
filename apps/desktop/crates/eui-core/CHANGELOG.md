# Changelog: `eui-core` (Rust core)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- A Meter with bound `segments` lays out each slice (the macro's track template), not an empty track (VAPP-278).
- `brand-exponential` (#VAPP-283): the Exponential logo as an icon concept (icons.json `custom`, a filled 24-unit glyph in currentColor), in the Icon vocabulary of every renderer; the Icon specimen's non-Lucide case shows it.

## 0.16.3 (2026-11-06)

- Markdown parses the same on all five renderers (`markdown-render.json`, every golden case passes; VAPP-258), and a number field seeded with a value selects it on focus so Save runs once (VAPP-208).

## 0.16.2 (2026-11-06)

- The icon list follows the app: `power` and the eDE icons (`ede-window`, `ede-lock`, `ede-power`, `ede-suspend`, …) are new.

## 0.16.1 (2026-11-05)

- #VAPP-253: `Surface::set_layer_viewport(width, height)`: a top or bottom Drawer lays out at the window's size, a popup above it lands in it. Width <= 0 = unset (the default), today's behaviour.

## 0.16.0 (2026-11-05)

- VAPP-212: `host::MemoryRecordStore` + `RecordsDelta`: one app's rows, sync deltas, the records value with an optional per-query overlay.

## 0.15.4 (2026-11-05)

- The open libraries are the Exponential SDK: genUI, Motion and Apps name the skills and docs (`exponential-ui` skill = `exponential-genui`; docs under /docs/sdk/); package names and tags stay. The amber warning colour is `semantic.yellow` (EXP-1695).

## 0.15.3 (2026-11-05)

- The icon list follows the app: `thread-stories` is gone, `thread-results` and `nav-needs-you` are new (FEED-191).

## 0.15.2 (2026-11-04)

- EXP-1691: a built-in `$string.<id>` message on a `checks` entry resolves through the surface's strings; a Markdown's text asks the host's `textLinks` hook (`#IDENT` links). VAPP-250: the narrow-phone Timeline and 3-column Table layouts.

## 0.15.1 (2026-11-04)

- EXP-1691: a dismissed bound overlay (Dialog, Drawer, Sheet) stays closed instead of reopening; a side Drawer panel sizes to its content, not the viewport; `bind` actions reach the host (a RecordTable row tap opens the record); `pluralize` reads `{count}`; RecordTable's Updated column is 136 wide.

## 0.15.0 (2026-11-04)

- **Breaking** (hence the minor): eApps packages are `formatVersion: 4` (a screen's `nav: false` and `exposed`, so apps can expose screens to each other); `harness.openIssue` and `harness.openDevice` are removed (links open in the app through `openUrl`). A version-3 package is migrated to 4 on the server.
- **Breaking** #EXP-1674: `PACKAGE_FORMAT_VERSION` 4: a screen's `nav: false` and `exposed`; `host::nav_screens`, `host::cross_app_target`; `UrlPolicy::link_base` (`decide_url` expands `~/x` against it).

## 0.14.0 (2026-11-04)

- VAPP-217 (eApps U2): the `RecordForm` and `RecordTable` macros expand in the core, with the host's record fill (kind schema, row values, the form's submit as a records batch).
- VAPP-232 (eApps U3): the `Calendar` and `Timeline` macros over the records query's month view (new reserved query params, month-view fields).
- EXP-1674: `Row.href` / `Button.href` open through the host's `openUrl`.

## 0.13.0 (2026-11-03)

- VAPP-232 (eApps U3, 1/3): calendar views in the records query (`host::calendar`): month weeks + lanes for spanning rows, week and timeline.

## 0.12.0 (2026-11-03)

- VAPP-218 (eApps U1): screens (`screens` in the package, the host paints the navigation), the `app.navigate` action and the `@media (form-factor: phone|wide)` condition (`SurfaceSettings.form_factor`).
- VAPP-221: `Surface::components()`, a read-only getter of the flat components (hosts find a FileUpload's bound `files` path).

## 0.11.0 (2026-11-03)

- VAPP-233 (eApps R8): `host::record_migrations` (`migrate_record`, `range_holds`, `validate_migrations`, `kind_changes`, `migration_refusal`).
- VAPP-222 (eApps R6): `host::record_transitions` (`transition_refusal`, `transitions_from`, `validate_record_transitions`, `kind_transitions`); `RecordsContext.transitions` + `role` (rows carry `transitions`).

## 0.10.0 (2026-11-03)

- VAPP-219 (eApps R5): record refs + expand: a kind's `refs` (`records.<kind>.refs`) name the kind a field points to, and a query's `expand` resolves them into the rows.
- VAPP-221 (eApps R7): files on records: `format: "attachment"` on a string field or a list's string `items`, validated by `validate_record`; `record_attachment_ids` / `record_attachment_fields` / `record_attachment_id`.

## 0.9.0 (2026-11-03)

- VAPP-214 (eApps Q3): computed fields: a record kind's `computed` (`records.<kind>.computed`, a CEL expression per field) is evaluated on read and can be filtered, sorted and grouped on like a stored field; `RecordsQuery.computed` carries them into the query.
- VAPP-201: install-time template issues come out in sorted path order, the same on every renderer.

## 0.8.0 (2026-11-03)

- VAPP-200: `SurfaceSettings::read_only` + `Surface::set_read_only`: the build disables every writing control (`read_only::disables`, `read_only_tree`, `disabled_ids`); conformance suite `read-only` (`fixtures/read-only.json`). The FFI's `FfiSettings` gains `read_only` (default false).

## 0.7.1 (2026-11-03)

- No changes of its own; released with the SDK (#1742's specimen look parity and #1744's neutral example names).

## 0.7.0 (2026-11-03)

- **Breaking** (hence the minor): eApps packages are `formatVersion: 3`, and a record kind may carry a per-kind `policy` (`read`/`create`/`update`/`delete`: `member | author | owners`); a version-2 package is migrated to 3 on the server, and hosts refuse newer formats.
- **Breaking** eApps R3: `PACKAGE_FORMAT_VERSION` 3 and per-kind record policies: `host::record_policy` (`record_policy`, `record_refusal`, `record_allowed`, `readable_rows`, `validate_record_policy`, `RecordOp`, `RecordViewer`), `apply_record_batch_as` (judged writes; created rows carry `authorId`), package tests take `role`. Fixture `record-policy.json`.
- `cel`: `dateAdd` with a huge `n` and any day outside 0000..9999 is `date out of range` (it overflowed); the CEL differential fuzz corpus `fixtures/cel-fuzz.json` is replayed in `tests/cel_fuzz.rs`.
- VAPP-209: an `expr` call's var bound to nothing (a row's missing optional field) reads `null` in `data::resolve_value` instead of failing as an unknown name and falling to the `default` (the TS twin agrees). New example `records_query_25k` (local query bench).

## 0.6.0 (2026-11-02)

- **Breaking** (hence the minor): eApps packages need `formatVersion: 2` and read records only through `records:<kind>`; an expression is pure (writes and host calls only in `actions`); every `{call: expr}` binding needs a `default`, and an unguarded optional (`T?`) value is an install issue.
- **Breaking** (eApps keeps no backwards compatibility): records are read only through `records:<kind>?…`. `host::parse_records_query(params, kind)` takes the kind only from its argument (no `kind` param fallback); `kind` left `RECORDS_RESERVED_PARAMS` (it is a plain field filter now); `expand_query_source` keeps no `kind` param; `validate_package`'s `query=` check and the expressions' records-binding typing accept only the `records` scheme (no `exp:records?kind=`). `cel::WRITE_FUNCTIONS` lost `harness.records.upsert` / `harness.records.delete`. `validate_package` requires `formatVersion` = `PACKAGE_FORMAT_VERSION` (2): missing = `formatVersion is required (2)`, older = `format version 1 is no longer read: write 2`. `RECORD_RESERVED_KEYS` gains `authorId` (host.json `package.records.reserved`).
- VAPP-194 (E1, typed effects): `cel::function_effect` / `FunctionEffect` (`WRITE_FUNCTIONS`, `HOST_FUNCTIONS`, `HOST_FUNCTION_PREFIXES`, `effect_words`); the parser refuses a write or host function in an expression (`at N: <name> writes|is a host function: an expression is pure, use an action step`, a dotted name at its first segment); `cel::evaluate_counted` (the result + steps used). `host::actions`, the twin of `src/host/actions.ts`: `validate_package_effects` + `validate_package_actions` (called by `validate_package` after the expressions), `run_action(pkg, name, params, &ActionContext)` → the plan `{writes, sets, calls}` or the error, `apply_record_batch` (all or nothing), the limits and names. `RECORDS_FUNCTIONS` gains `records.batch`; `HOST_ERROR_CODES` gains `ACTION_FAILED`; `host_contract()` gains `package.actions`. Replays `fixtures/actions.json` (`tests/actions.rs`).
- VAPP-196 (T1, package tests): `host::tests`, the twin of `src/host/tests.ts`: `validate_package_tests` (the shape of `tests: [{name, records?, now?, as?, expr | query | action, expect}]`), `run_package_tests` → `[PackageTestResult {name, pass, got}]` (each test on its own seeded store, at `now` in UTC, as `as`), `test_matches`, `test_failure`, `package_test_issues`, the limits and defaults. `validate_package` ends with the tests' shape issues and, on an otherwise valid package, one `/tests/<i>` issue per failing test (`<name>: expected <canonical JSON>, got <canonical JSON>`). `host_contract()` gains `package.tests`. Replays `fixtures/package-tests.json` (`tests/package_tests.rs`).
- **Breaking** VAPP-195 (E2, explicit missing values): a type that may be missing is `T?`, spelled `{"?": T}` (`cel::is_optional`, `optional_type`, `required_type`; `type_display` prints `T?` and now returns `String`). The checker refuses an unguarded use of a `T?` (`<path> may be missing: guard it with has(), a null check or orElse()`, `cel::path_text`); `has(a.b)` (`Node::Has`), `p != null` / `p == null` through `&&` / `||` / `!` narrow; `==` / `!=` take optionals; a null-branch ternary is `T?`. New function `orElse(value, default)` (`FUNCTION_NAMES`, `CALL_NAMES`). `call_expression_function` and `data::resolve_value` fall back to the call's `default` when a function fails or is unknown (absent = no value). `host::expressions`: `row_of(properties, required)` (non-required fields are `T?`, the row keys never), an open binding path or a call var is `dyn?`, every `{call: expr}` needs `default`. `host::actions`: `params` via `row_of`, `data` `dyn?`, `me` `string?`; a `T?` fits only an optional field (an upsert field is optional unless required). Replays the new `fixtures/expressions.json` and `fixtures/actions.json` cases.
- EXP-1563: a horizontal List's divider is a vertical hairline (`width` + `height: auto`, like web); a disabled NumberField or ChipInput puts the `disabled` state on its whole `field`; a disabled FileUpload's Browse button no longer carries its own `disabled` (the dropzone dims it once).

## 0.5.0 (2026-11-02)

- **Breaking** (hence the minor): `RecordsQuery` / `RecordsFilter` (re-exported from `host::record_store`) are the structured query now: `kind`/`order`/`filters` became `from`, `sort: Vec<RecordsSortKey>` and `where_`; `RecordsFilter.op` is `RecordsFilterOp`, `.value` is `RecordsFilterValue`.
- VAPP-191 (Q2): `cel`, the twin of `src/cel.ts` (`parse_expression`, `evaluate_expression`, `check_expression`, `call_expression_function`, `ExpressionEnv {now, offset, max_steps}`, `MAX_LENGTH/MAX_DEPTH/MAX_STEPS`); `data::call_function` runs the 19 expression functions and `expr` on the surface clock in the formatter's zone (`Formatter::offset_minutes`, default UTC, `ZonedEnglishFormatter` answers its offset). `host::expressions` (`validate_package_expressions`, called by `validate_package`), `dates::day_number` / `day_from_number`. Replays `fixtures/expressions.json` (`tests/expressions.rs`) and the new `bind-time.json` case.
- VAPP-190: `dates` (calendar-day maths shared with the coming expression functions: `date_bucket`, `DateBucket`, `parse_day`, `iso_week`, `days_in_month`, `weekday_of`…) and the JSON query form in `host::query`: `parse_records_json` → `(RecordsQuery, Vec<RecordsQueryIssue>)`, `fill_query_templates`, `expand_query_source`; `RecordsQuery` gains `or`, `group`, `aggregate`, `running`, `before` (serde-defaulted), `RecordsFilter.value` is `RecordsFilterValue` (one or many); `records_value` answers `groups?`, `totals?`, `before?` and running values. `validate_package` checks `queries` (`is_query_name`) and the router expands `?query=<name>` binds. Replays `records-query.json` `jsonCases` / `bucketCases` / `expandCases`.
- VAPP-189: `host::query`, the Rust twin of the TS records evaluator: `parse_records_query`, `parse_records_source`, `records_value` (`RecordRowRef`, `RecordsContext`) and `records_value_owned` (`RecordRow`), replaying `fixtures/records-query.json`; `host::record_store` keeps `RECORDS_FUNCTIONS` and re-exports the query.

## 0.4.0 (2026-11-02)

- EXP-1604: THE Motion box (`layout_tree::motion_size`): unstyled, the composition's px (`width` + `maxWidth: 100%` + `aspectRatio`), shrinking with its aspect; a style width/height wins; a `src` animation gets none. The fixed measure's Motion intrinsic.
- EXP-1586: THE ZoomableImage box (`width: 100%` + `layout_tree::zoomable_aspect_ratio`: natural ratio, a tall picture capped at a square, else 16:9; a style height wins) and its `zoom` event writing the bound `scale` before it fires.
- EXP-1493: the Carousel `counter` frame sits at the track's top-END corner (`insetInlineEnd: $spacing.sm`, mirrored under RTL), off the page's own heading.

## 0.3.0 (2026-10-31)

- VAPP-180: `validate_package` checks a package's `exposes` (MCP tools a host's broker runs) and `upstreams`, mirroring `src/host/exposes.ts` line by line (`host/exposes.rs`: `expose_issues` with the eMCP error codes, `validate_package_exposes`, `validate_upstreams`, the `package.exposes` contract constants); `host-router.json` replays it.

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
