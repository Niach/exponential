# Changelog: `eui-core` (Rust core)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- Select option `hint` (#EXP-1629): the row of parts gains `itemHint` before the check (`tests/select_option_hint.rs`).
- Select `button` trigger glyph (#EXP-1629): the trigger leaf carries the picked option's `valueIcon` + `iconColor` (`tests/select_button_glyph.rs`).
- Select `triggerVariant: "button"` + Text `truncate` (#EXP-1778): a `button` trigger stretches, holds the `Select.dropdown` glyph and no `label` part (the label is its accessibility `label`); its popup fits its options (at least `layout::SELECT_BUTTON_POPUP_MIN_WIDTH`), its border box `SELECT_BUTTON_POPUP_OFFSET` under the trigger and `SELECT_PILL_POPUP_INSET` outside its edge. A `truncate` Text is one line (`lines: 1`); `text_truncate::middle_truncate_width` replays `fixtures/text-truncate.json`.
- Select `row` disabled (#EXP-1629): generated themes; `tests/select_row_disabled.rs` (value + chevron 0.35, label + description unchanged).
- Menu item `detail`, `identifier`, `preview` (#EXP-1629): a row's `identifier` (a muted text part; an icon then leads as its own `itemIcon` glyph) between the icon and the label, its `detail` after the label (a submenu parent's too); `menu_preview` = which row's preview shows (a row's `hover` / `focus-visible` states and the pointer's `hover` on the preview, the waits through `OutEvent::HoverTimer` owner `<menu>.preview` and `hover_timeout`); the shown one is the menu's layer (`<menu>.preview.<row>`, Anchored right/centre of its row, mirrored, `MENU_PREVIEW_OFFSET`) holding an Embed leaf `<menu>.preview.<row>.surface` (no part: painters mount it as an Embed; its `action` fires the Menu's); the preview root is `hover_styled` so painters track the pointer on it; closing the menu drops the state. Layout constants `MENU_PREVIEW_OFFSET`, `MENU_PREVIEW_OPEN_MS`, `MENU_PREVIEW_CLOSE_MS`.
- Select option `iconColor` (#EXP-1629): an option's tint becomes its `itemIcon`'s (and, with `triggerIcon`, the `valueIcon`'s) authored `color`, a token resolved per mode or the host's bound hex (`tests/select_icon_color.rs`).
- Composer matches the app: generated catalog (`Composer.send` = `ui-submit`) + themes (the flat field's `paddingTop`: neutral `$spacing.sm`, exponential 4.25); `interactions_fixture` replays composerSlots `field` against the recipes.
- Menu item `iconColor` (#EXP-1629): an item with an `icon` passes its `iconColor` to the `itemLabel` leaf's props (`tests/menu_icon_color.rs`); a literal is validated as a colour token, a bound one is the host's.
- Skeleton `rows` are borderless (#EXP-1629, `fixtures/skeleton-rows.json`): no `borderTopWidth` hairline between rows, so rows after the first centre their dot and bars in the full row height again (+0.5 px off before). `tests/skeleton_rows.rs` replays the fixture.
- Select `multiple` (#EXP-1629): a picked item's part query carries `joinTop` / `joinBottom` when the SHOWN item above / below is picked too, so the recipe draws a run of adjacent picks as one block; the theme validator knows the three new corner keys. `tests/select_multi_join.rs`.
- Popover `matchAnchorWidth`: the layer (`LayerSpec::exact_anchor_width`) is laid out at EXACTLY its anchor's frame width every pass (min = max = the anchor's), so it follows the anchor when it resizes; `overlay-geometry.json`'s `matchAnchorWidth` cases replay through the layer manager, + a resize test.
- A Select `pill`'s popup is placed in the IDE Popover's order (#EXP-1629): the inset first, then the flip and the margin clamp, then the content's offset from the pill rounded to whole points, half away from zero (`overlay::place_inset_overlay`, gpui's `Positioner`). Pushed back from the right edge it used to land on the clamp minus the inset (a pill at x = 551.5 in a 760 wide view: 490 against the IDE's 490.5). `overlay-geometry.json` `inset` locks it.
- Composer slot `above` (#EXP-1762): a slotted Composer with one gets an `above` column FIRST (`c.above`, the content width, at the content top; the column `space-between` keeps the bar at the bottom); the measured leaf grows by its height at the content width + the Composer's gap (`above_extra`); the flat slot reserve reads the LAST child as the bar.
- Composer `layout: "flat"` (#EXP-1762): a flat, slotted Composer's measure gets the content width minus its slot wrappers (each + the bar's gap), the slots' width added back to the answer, so its field wraps where it is painted; `COMPOSER_ATTACH_W` (32) for the painters.
- `ColorPicker` (#EXP-1775): the macro's 28 px cells, the `inline` root (`grid`; its swatches pick without a popover to close) and the `customTrigger` alternative (a child); generated catalog + themes.
- Colour token `strokeActive` (`derive_colors`: a chain that sets none takes `ring`) and the generated themes' focused box Input stroke (`Input.field` `variant: box` + `focus`).
- Select `triggerIcon` + the own-colour marks (#EXP-1764): a `row` Select with `triggerIcon: true` leads its value with the chosen option's `icon` as the `valueIcon` part (an Icon leaf in the `button`, before the `value`; one chosen value only, off by default, never on a field trigger). `layout_tree::icon_fill(name)` = an icon's OWN colour from the generated `ICON_FILL_NAMES` / `ICON_FILL_COLORS` (`brand-claude` = `#D97757`); `brand-claude` / `brand-codex` join `ICON_NAMES`.
- `IconPicker` + `ColorPicker` (#EXP-1760): a `press` on a picker's cell, swatch or No icon row closes its root Popover after the pick (`picker_root`); a macro template's `$each` may be a literal array (the default icon set and palette).
- Select `triggerVariant: "pill"`, `loading`, option `mono`, `searchPlaceholder` (#EXP-1765): a `pill` trigger is the field trigger content-sized (`alignSelf: flex-start`) with the `Select.dropdown` glyph and no `label` part (the label is its accessibility `label`); its popup is `layout::SELECT_ROW_POPUP_WIDTH` wide by default, its border box `SELECT_PILL_POPUP_OFFSET` (3) under the pill and `SELECT_PILL_POPUP_INSET` (1) outside its aligned edge (`LayerSpec::inset`, still bounded by the margin). A `loading` Select's list holds `SELECT_LOADING_ROWS` Skeleton rows (node `<id>.skeleton`, the Skeleton's own recipes) and stays live (`is_inert` exempts Select from `loading`). A `mono` option's item gets `fontFamily: $type.family.mono` and keeps its `trailing` as its own prop (not appended). `searchPlaceholder` replaces the search's built-in placeholder. Recipe keys gain `paddingTop`, `paddingBottom` and `textAlign`.
- Select (#EXP-1629): an option with an `icon` or a `description` lays out as an `item` container: `itemIcon`, the `itemText` column (`itemLabel`, `itemDescription`), the chosen one's `check`. `popupMinWidth` sets the popup's `minWidth` (content-fit) in place of a row's fixed width, and the popup no longer matches its anchor's width.
- Icon `size: xl` (#EXP-1629): the `iconSize` enum and the `iconXl` control token (28) in the generated catalog and themes.
- Input `variant: "box"` + `size` (#EXP-1629): no `label` part (the field's accessibility `label` carries it); the catalog's `inputSize`.
- An autosize Textarea's `field` and the Composer are measured at their LAID-OUT width (#EXP-1629, `measure::follows_width`, `MeasureMemo::width_dependent`). Their intrinsic width is fixed (min = max content), so the memo used to answer every width from the max-content height and never asked the host at the real width. A long line now grows the field as it wraps at that width (a 12k-char URL or hash grows rows 10 to maxRows 40). No catalog, schema or FFI change: every renderer on the core layout (eui-gpui, SwiftUI, Compose) gets it.
- Select option `trailing` + event `open` (#EXP-1629): a popup `item`'s `text` is `"{label} {trailing}"` when the option carries `trailing` (the search still matches the label); `set_open` fires the Select's `open` on the closed → open transition only, and `event(<a Select node>, "open")` fires it for a host's platform menu that opens outside the core's layers.
- Input `bare` / `code` and Textarea `row` / `code` (#EXP-1629): no `label` part (the field's accessibility `label` carries it).
- Template items build cheaper (#LOGS-7): an item instantiates its template by reference (no copy per item) and `with_suffix` copies each node once instead of its whole subtree at every depth. A long chat's List re-windows in about half the time; the built trees are the same.
- Select `description` (#EXP-1629): a `field` Select shows it under the trigger (the form description); a `row` trigger with one is the `labels` column (the `label`, then the `description` at most `layout::SELECT_ROW_DESCRIPTION_MAX_WIDTH` 460, wrapping; it shrinks) and the trailing `control` column (the `value` + `chevron`, end-aligned; its recipe `minWidth` holds it). The row trigger's recipes take `minHeight` + `paddingVertical` instead of `height`, so the row grows with the hint.

- Composer slots (#EDE-65 R): a Composer with `leading` / `trailing` stays one measured leaf and gets a `bar` row at the bottom of its content box (the send disc's height): `leading`, a spacer, `trailing`, then `sendSpace` (the disc's `$control.buttonIcon` square, unpainted). A measured leaf with children now lays them out as a flex box at its measured size inside its padding + border (`Engine::set_inset`); they never size it. `interactions-round1.json` `composerSlots` + `composerPaste` replayed.
- `color.borderStrong` + recipe keys `borderTopLeftRadius`, `maxWidth` (#EDE-65): `load_theme` derives `borderStrong` from `border` when a chain sets none (as `link` from `primary`); the validator takes `maxWidth` as a number, a `$control`/`$spacing` token or a percentage `"N%"` (0–100). The built-in `exponential` theme's MessageBubble is the IDE's chat bubble.
- Select (#EXP-1629, the IDE's multi picker): a `row` trigger's `value` (else `placeholder`) and `chevron` sit in a `button` part (row, centred, shrinking), the box the IDE's ghost value button fills; the popup still hangs off the value.

## 0.21.0 (2026-11-09)

- Select `triggerVariant` / `align` / `popupWidth` (#EXP-1629): a `row` Select builds no `label` part above its trigger; the trigger is a container of parts: `label`, a flexible `spacer`, `value` (else `placeholder`, at most `layout::SELECT_ROW_VALUE_MAX_WIDTH` 240) and the `Select.chevron` (`ui-chevron-right`) `chevron`; it keeps `text` / `placeholder` / `label` props for its accessible value. The popup aligns by `align` (unset: `start`) and is `popupWidth` wide (unset: a row = `layout::SELECT_ROW_POPUP_WIDTH` 262, a field at least its trigger); a row's popup hangs off its VALUE (`SELECT_ROW_POPUP_OFFSET` 1 under it, `SELECT_ROW_POPUP_MARGIN` 7 from the view's edges; `LayerSpec.offset` / `margin`). Every Select that opens starts its list at the TOP. The list sits in a relative `body` with a bottom `fade` (`SEARCH_LIST_FADE`, its colour as SearchList's gradient) while options sit below its visible end (`LocalState.select_fade`, refitted after layout). `Surface::set_popup_bounds` fits anchored popups into the host's window instead of the surface. Recipes: the recipe-only `backgroundClip` (`Visual::background_clip`, `padding-box` only) and the theme shadow `popover`.
- Embed (#EDE-65 lane M): `embed::{embed_card, embed_mount, embed_payload, embed_surface_id, embed_error_root, EMBED_EVENT}`, the Rust twin of `src/embed.ts`: which resolved `surface` value is a card (a sound `root`, `data` absent / null / an object, `version` absent or 1.0), the error box a bad one mounts instead (an error Alert titled `$string.embedInvalid`), the `action` payload `{name, context}` an inner event becomes and the nested surface id `<host>/<painted id>`. `surface` is a DATA prop: a top-level binding resolves against the host, nothing inside it does. `tests/embed_fixture.rs` replays `fixtures/embed.json` (parse, and render / events on core surfaces).
- Composer `menuKeys` + `key` (#EDE-65): the generated catalog carries the prop and the event; a painter's `key` {key} on a Composer reaches the host unchanged (`interactions-round1.json` `composerKeys`).
- Input `variant` (#EXP-1629): a `row` Input builds no `label` part above its field; the `field` leaf carries the `label` (painted inside it, leading).
- KeyRecorder guards (#EXP-1629): on a KeyRecorder `field` `Surface::event` takes `press` only while idle and `record` / `blur` only while it records (its resolved `recording` recipe prop); a `record` payload merges into the fired `key` action's context. Locked by `fixtures/interactions-round1.json` `keyRecorder`.

## 0.20.0 (2026-11-08)

- RecordTable columns, search, filter and totals (#FEED-202): a `columns` entry may be a Table column object (`{key, label, type, currency, decimals, align, width, format, truncate, sortable}` over the field's defaults; a currency column defaults to 96 px), `search: true` (an Input over the shown text columns), `filter: "<enum field>"` (a Select, `$string.all` first) and `totals: true`; the host fill narrows the rows, the count and the phone rows through the core `filter`. Table gains `totals` (a last row: each number and currency column's sum over its rows, the first column `$string.total`; parts `row` / `cell` with the painter's `total: true`, the neutral theme's medium weight) and the column `format` (a date / dateTime TR35 pattern), on React, Solid, gpui, SwiftUI and Compose. Strings `all`, `total`. `fixtures/record-macros.json`; the styleguide's Record table and Table entries show the Autotax Expenses ledger. Prompt budget 9,811 → 9,895 tokens.
- Tabs `swipeable` (#FEED-200): `swipe::{swipe_lock, swipe_step, swipe_offset, swipe_settle, swipe_progress, swipe_indicator, swipe_blocks}` + the `SWIPE_*` constants (`fixtures/swipe.json`); `Surface::event(index, "swipe", {phase: start|move|end|cancel, offset, velocity})` on a swipeable Tabs (its root or `content`) draws the panels and the `thumb` part at the drawn offset (paint-only `transform`, no transition under the finger) and settles on release (`value` written through, `change`). `LocalState.tab_swipe` / `tab_pager` (`TabPager`, measured after each pass).
- Chart `valueFormat` + rows (#FEED-205): `chart::format_chart_value` / `ValueFormat` fill `props.chart.tickLabels` and `valueLabels` in the chart's format; `Formatter::compact` (default: `format::compose_compact` over `number`/`currency`); `chart::chart_data` resolves `rows` + `categoryField` + series `field` into `categories`/`series`.
- Pixel snapping (EXP-1629): `Surface::set_pixel_snap(Some(scale))` lays out on the device-pixel grid the way gpui's own layout does: absolute lengths round to it before layout (a non-zero border to at least one device pixel), measured sizes round up to it, and every box's outer edges round in absolute coordinates after layout, half a pixel toward zero. Off by default.
- A streamed 0.9 surface costs linear again: the upgrade bridge advances its id maps in place and keeps their inverse, instead of copying both per message (`upgrade_message09_in`, #EXP-1742).

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- An app's own file as a src (#VAPP-271): `host::app_file_id(src)` (the id of an `app-file:<fileId>` src, else `None`), `APP_FILE_SCHEME` / `APP_FILE_ID_PATTERN` (`catalog/host.json` `media.appFile`); no URL or media scheme, a host with app files resolves it before the media policy. Locked by `fixtures/host-policy.json` `appFiles`.
- A call's answer (#VAPP-271): a package action's `call` step may name its answer, `result: <name>` (`ACTION_RESULT_PATTERN`, `catalog/host.json` `package.actions.resultPattern`), read by the later steps as `results.<name>`; `run_action` now answers `Result<ActionRun, String>` (`ActionRun::Plan` | `ActionRun::Pending(PendingCall {step, name, args, result})`), `ActionContext.results` carries the answers so far; a list of dyn items fits any list field at install; every Engine capability name (`camera.` … `share.`) is a host function; a package test's action takes `results` (`step <i>: <call> has no result in the test`).

## 0.18.0 (2026-11-06)

- Tree (#VAPP-285): `tree::tree_rows` / `tree_key`, `{treeRows(props)}`, per-item `$each` conditions, interpolated `$on`/`$set` event names; `Surface::event(index, "key")` on a treeitem answers a `Focus` event. `tree_rows` takes the rail Sidebar's `rail` (every row a tile, #VAPP-277).
- A foldable Row (#VAPP-284): macro `alternatives` + `{childCount}`, `$on` routes with their own context (`MacroRoute`), `fold::fold_key` and `Surface::event(index, "key", {key})` firing `toggle` on a node that folds.
- `SurfaceSettings::hosted_popups` (VAPP-281): anchored Popover / Menu layers unclipped, no flip / shift (`overlay::anchor_position`), `Layer.hosted` + `Layer.align`.

## 0.17.0 (2026-11-06)

- Sidebar `rail` (#VAPP-277): the macro table's `$pass` (`MacroDef.pass`) runs before a macro's slots expand, and a template child may be a conditional slot splice (`TemplateChild::Slot`); a Tooltip `side` of `start`/`end` is logical (mirrored under RTL); a pressable node's `focus-visible` opens the tooltips it hosts (no tab stop of their own). Locked by `tests/painter_asks.rs` `sidebar_rail_*`.
- **Package format 5** (#VAPP-260, breaking: a host reads exactly 5): `capabilities: [{id, why, origins?}]`, the capability manifest the Engine's broker and the bundler read (`catalog/host.json` `package.capabilities`, `validatePackageCapabilities` / Rust `host::capabilities`, `docs/package-format.md` "Capabilities"); `fixtures/host-router.json` gains `acme.capabilities` and four invalid packages.

## 0.16.3 (2026-11-06)

- DatePicker `inline: true` lays the month grid out in flow (no trigger, no layer), days carry `mark`, `today` and the `isToday` / `outside` recipe props, the steppers fire `month {month}`; the calendar takes the web reference's geometry (28 steppers, narrow 24-tall weekday heads, centred day labels); `EEEEE` formats the narrow weekday (VAPP-282).
- Tabs `variant: strip` (#VAPP-279): the strip build, the fit into `+N` after each pass (`Surface` refits, at most two extra rounds), the `drag` / `key` reorder with a live preview, `more`; `tabs_strip` (the pure rules), `TAB_STRIP_MAX_WIDTH`, `TAB_DRAG_THRESHOLD`, `TAB_LONG_PRESS_MS`.
- A Meter with bound `segments` lays out each slice (the macro's track template), not an empty track (VAPP-278).
- `brand-exponential` (#VAPP-283): the Exponential logo as an icon concept (icons.json `custom`, a filled 24-unit glyph in currentColor), in the Icon vocabulary of every renderer; the Icon specimen's non-Lucide case shows it.
- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception).
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception). A Switch with `label: ""` lays out the web's empty `body` column (it grows; the track ends the row) instead of an empty `label` part.
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` (VAPP-276): `OverlayAlign::parse`, `PlaceOptions::placement`; an `anchor`ed overlay's node becomes its trigger (`trigger_for`, pressable) and its open layer is placed against that node's frame (linked after the whole build; a subtree rebuild over an anchor rebuilds fully).

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
