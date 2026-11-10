# Changelog: `at.exponential:eui-compose` + `eui-compose-primitives`

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- Select `triggerIcon` + the own-colour marks (#EXP-1764): the `valueIcon` part comes from the core; `IconView` paints a mark with its own fill (`CatalogConstants.iconFills`, generated `iconFillNames` / `iconFillColors`: `brand-claude`) in it, never in the ink.
- `IconPicker` + `ColorPicker` (#EXP-1760): macros the core expands and closes on a pick; painted from the Popover, Button, Box and Icon the painter has.
- Select `pill` + `loading` + option `mono` + `searchPlaceholder` (#EXP-1765): a `triggerVariant: pill` trigger paints the field trigger's parts content-sized (the core's), no visible label (the `label` is its accessible name), the `Select.dropdown` glyph at the `chevron` part's size and colour (opacity 1 unless the recipe fades it), its `value` / `placeholder` parts resolved with the trigger's `disabled`. Our own popup for a pill: `popupWidth` else layout `selectRowPopupWidth`, its border box `selectPillPopupOffset` under the pill and `selectPillPopupInset` outside its aligned edge (`selectPopupX`), `selectRowPopupMargin` from the window. Opened by the POINTER a pill's popup (core layer and our own) shows no cursor until the pointer moves over an option or a key goes (↓ lands on the first enabled option, `selectWakeCursor`); from the keyboard it keeps it. `loading`: the core's `<id>.skeleton` rows in the core layer, `selectLoadingRows` Skeleton rows (the `Skeleton/row|dot|bar` recipes, `$control.row` high) in our own popup, never the options or `empty`. A `mono` option: the label in the mono type, its `trailing` its own muted sans run `selectTrailingGap` after it (the measurer counts it). The search field: `searchPlaceholder`, the `searchIcon` part's glyph size, the recipe's `paddingTop` / `paddingBottom`. A pill's search ends, only while it holds text, in the clear button (`Select.clear` 0.75 × the `searchIcon` size in `mutedForeground`, centred in a 1.25 × square box, "Clear search", a button): it empties the field, the core hears the empty `change` at once and the focus stays in the field (`selectSearchClear`). The popup's `empty` honours the recipe's `textAlign` (and its `paddingHorizontal` / `paddingVertical`). A `loading` Select still opens. `InteractionTest.selectPill` / `selectPillPopupRules` / `selectPillSearchClear` / `selectLoadingOpens`.
- Select option `description` + `popupMinWidth` (#EXP-1629): the popup's option rows take the `itemIcon` size and draw the muted `itemDescription` under the label; an item is at least its `minHeight` and grows; `popupMinWidth` gives the popup a minimum width instead of a row's fixed one.
- Icon `size: xl` (#EXP-1629): 28 dp from the recipe (`$control.iconXl`). Snapshot `components.json` gains `Icon/size=xl`.
- Input `variant: "box"` + `size` (#EXP-1629): painted from the core's parts (no label part; the field's accessibility label is the owner's `label`). Snapshots re-recorded.
- An autosize Textarea and the Composer are measured at their laid-out width (#EXP-1629, from eui-core) instead of a guessed 160 px / 320 px; Compose's text layout wraps a word wider than the row by character, so a 12k-char URL or hash grows rows 10 to maxRows 40.
- Select option `trailing` + event `open` (#EXP-1629): the core's popup item carries the text; the native popup's option reads `label trailing` (filtered by the label); opening the native popup fires the core's `open` (`fire(trigger, "open")`). `InteractionTest.selectTrailingAndOpen`.
- Textarea `variant` + Input `bare` / `code` (#EXP-1629): painted from the core's parts (no label part; the field's accessibility label is the owner's `label`). Snapshots re-recorded.
- Select `description` (#EXP-1629): painted from the core's parts (the row's `labels` and `control` columns, the field's hint). Anchored popups now fit into the WINDOW around the surface (the surface's window position + the root's size, `SurfaceModel.setPopupWindow` → the core's `set_popup_bounds`), as web and the IDE do.
- Composer slots + paste (#EDE-65 R): a slotted Composer paints its leaf and then the core-laid slot row over it; the attach control ends one `xs` gap before `trailing`. A hardware Ctrl/⌘+V or the selection toolbar's Paste reads the clipboard's content URIs (`composerClipboard`) and `SurfaceModel.composerPaste` fires `attach` {files} for the images and calls the new `HostPlugin.onUpload(UploadEvent)` (`name` = the Composer's id, `uris` = the content URIs); files without an image insert nothing. `InteractionTest`.
- Select (#EXP-1629, the IDE's multi picker): a row's value button (the core's `button` part) lends its pointer hover to the value, placeholder and chevron inside it; a `multiple` popup (core layer and native popup) opens with the cursor on its first chosen option; the native popup's options paint their recipe border.

## 0.21.0 (2026-11-09)

- `eui-compose-primitives` `Kbd`: fills with `PrimitiveTokens.accent` (the catalog's `Kbd` recipe; was `muted`).
- Input `variant: "row"`: a Group row (label inside, the value end-aligned at 70%); the exponential theme's row is the IDE's (14 px rem).
- The `Embed` native (#EDE-65, `fixtures/embed.json`): a nested surface painted in place by `ExponentialSurface`, SANDBOXED: each painted Embed gets its OWN `SurfaceModel` (`embed(index)`; surface id `<host>/<embed id>`, `embedSurfaceId`) holding the card's data model (an inner `set` writes it only; inner bindings never read the host's), under the host's theme, mode, settings and environment, with the host's icons, fonts, url and media policy. Every inner `event` action fires the Embed's `action` with `{name, context}` (`embedPayload`, merged over the author's context by the core; no `on.action` = nothing); an inner host function is denied (never forwarded, its event does not fire); inner input edits stay inner. The measurer sizes the Embed by its nested surface at the width it gets (a card fills it; the host measures it again when the card's height moves). A bad `surface` (`embedCard`: not an object, no sound `root`, a non-object `data`, a `version` other than 1.0) or one the core refuses paints the error Alert (`EMBED_ERROR_ROOT`, `$string.embedInvalid`); a changed `surface` value is a fresh mount. The fixed measure (`fixedMeasure`, the snapshot suites) lays each card out at its Embed's width and runs the pass again with that size, so a card is never 0 tall there. No role of its own (the nested nodes join the host's order). `EmbedTest` (parse, render, events), `EmbedPaintTest`.
- Composer `menuKeys` (#EDE-65, `fixtures/interactions-round1.json` `composerKeys`): while it is true (read live), Up, Down, Enter, Tab and Escape with no modifier held in the focused Composer fire `key` {key} and nothing else (the surface's keys take them before the field: no edit, caret move, submit, Tab move, Escape dismiss or `stop`; the soft keyboard's Send / Return too); a held modifier (Shift+Enter's newline) and an IME composition pass by. A send now submits the trimmed text and writes the emptied draft through, so a bound value reads "" after it. `composerKey` / `composerMenuKey` / `composerMenuKeysOn`, `FieldState.composing`. `InteractionTest`.
- Toolchain: Compose 1.12 (BOM 2026.09.00), core-ktx 1.19, activity 1.13, built with AGP 9.5 (built-in Kotlin) on Gradle 9.6. **Consumers now need AGP 9.1+ and `compileSdk = 37`** (Compose 1.12's own floor); minSdk stays 26.
- Select `row` + the IDE popup (#EXP-1629): a `triggerVariant: row` trigger is the core's container of parts (`label`, a spacer, `value` at most 240 wide and truncating, the `Select.chevron` `chevron`), painted generically, ONE TalkBack dropdown node (label + shown value) whose tap opens the popup; a field trigger paints its `value` and `chevron` parts. Under native overlays the Select opens our own popup ([SelectPopup], no more M3 `DropdownMenu`): a row's hangs off its VALUE (layout `selectRowPopupOffset` under it, `selectRowPopupMargin` from the window's edges), a field's off the trigger, aligned by `align` (default `start`), `popupWidth` wide (row: layout `selectRowPopupWidth` 262; field: at least the trigger), painted from the `content` recipe's shadows (`$shadow.popover`), fill and border (`backgroundClip: padding-box` keeps the fill inside the border), `search` (its `gap`, the glyph in `mutedForeground`) / `item` / `check` / `empty` / `fade` recipes. Both popups open at the top with the cursor on the first enabled option and focus in the search field (else the list), and show SearchList's 24 dp bottom `fade` while options sit below; the cursor (arrows or the pointer) is the item's one `hover`, `selected` = chosen with the `check` part. Recipes' `backgroundClip: padding-box` paints every box's fill inside its border (`PaintStyle.clipToPadding`); the Avatar picture sits on its `Avatar/image` fill, the whole round box under the ring (`AvatarView(pictureFill)`). `InteractionTest`, `components.json`.
- The `KeyRecorder` macro (#EXP-1629, `fixtures/interactions-round1.json` `keyRecorder`): its `field` presses on a tap (or Enter / Space while idle), takes the focus when `recording` turns true (focusable in touch mode), and while recording the surface's keys hand EVERY hardware key to the core as `record` (no Tab move, no Escape dismiss, no repeat; the navigation bar's Back stays the system's) with the canonical key name (`keyRecorderKeyName`: the physical key, f1-f24, US punctuation) and the held flags, a modifier key alone as `modifiers`, mouse buttons 3-5 over it as `mouse`; losing the focus is `blur`. `keyRecorderPress` / `keyRecorderRecord` / `keyRecorderMouse` / `keyRecorderBlur`, `keyRecorderKeyPayload`.
- Markdown: an image directly in a top-level paragraph's text splits the paragraph into the text before it, a block image and the text after it (edges trimmed, an empty side dropped); in a heading, list item, cell, quote or inside a span it stays its alt text (#FEED-208, `fixtures/markdown-render.json` `image_split_*`).

## 0.20.0 (2026-11-08)

- Tabs `swipeable` (#FEED-200): a horizontal TOUCH swipe on the `content` pages the panels (a mouse is unchanged): the axis locks through the core's `swipeLock`, a touch starting inside a Table, Chart, Carousel, ZoomableImage, Slider, Resizable, a horizontal List / ScrollArea or an `overflowX: auto|scroll` box is left to it (`swipeBlocks`, `SurfaceModel.swipeBlockedAt`), and `swipe {phase, offset, velocity}` goes to the core (`SurfaceModel.tabSwipe`), which draws and settles. A pure-translation `transform` now animates over the node's `transition` (snaps under reduced motion), so the panels and the `thumb` settle smoothly. `fixtures/swipe.json` lock / offset / settle / blocks replay through the facade.
- Chart value labels and the tooltip show the core's labels in the chart's `valueFormat` (#FEED-205).
- While an anchored overlay is up, the events the core raises during a pass are dispatched (#EXP-1739: an overlay whose anchor moved or went closes).
- The `Window` macro (VAPP-290): a framed window (a 34 dp chrome strip with three dots and a centred title, hidden from TalkBack, over a body holding its one child; the root a group named by the title) through the Box / Text paths, nothing of its own. `WindowTest`.

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- Round 9 parity: Markdown's inline code paints the `Markdown/code` recipe (its type, side padding reserved in the line, corners, in quotes too), a bullet is 0.36 em at 0.46 em into the indent, an ordered marker is in the text colour and ends one space before the text (`fixtures/markdown-inline.json` `documentPaint`). `MarkdownChipStyle.code`.
- Round 9 (`docs/round-9-contract.md`): check severities (§6.6): a field's one message in the `error` part's `warning` / `info` state, polite and never the field's semantic error; a Button's `checks`. `accessibility.live` wins over `Text.live`, `accessibility.hidden` leaves the tree (§5.1). Markdown `profile: inline` (§9.1): `MarkdownPainter.inlineRecipes`, `MarkdownStyles.paragraph`, `MarkdownColors.paragraphInk` / `headingInk`; an inline Markdown skips `HostPlugin.markdown`.
- `Canvas` (round 8, `docs/round-8-contract.md` §1): a static vector drawing on the core's `CanvasPlayer` (`CanvasLeaf`, `CanvasSource`), recompiled on a `size` / `shapes` change and resampled on a theme or mode change. Shapes paint through `MotionPainter`; the `text` ops follow natively (one line in the theme's sans / mono at the CSS weight, physical align, the baseline from `Paint.FontMetrics`). It boxes like a Motion, and it is an image named by `label`. With `pointer: true` it sends `press` / `move` / `leave` with `canvasToDrawing` coordinates (at most one `move` per frame); it is never a focus stop.
- Motion's packed frames are VERSION 2: `MotionList` decodes the `text` op (`Op.Text`), and `MotionPainter.paint` takes a `Typefaces` for it.
- Checkbox `indeterminate`: the dash glyph (`Glyph.Minus`, builtin `Checkbox.check.indeterminate`), `ToggleableState.Indeterminate` (mixed wins over checked), and a press on a mixed box checks it (`SurfaceModel.mixed`; replays `interactions-round1.json` `checkbox`).
- Progress `valueText` + `detail`: the `value` caption sets tabular figures (`ResolvedTextStyle.forPart`). Stepper `collapse` and the `@container` condition come through the core. Progress (with a label), Checkbox and Stepper snapshots re-recorded.

- An app's own file as an image src (#VAPP-271, `catalog/host.json` `media.appFile`): `app-file:<fileId>` loads through the new optional `HostPlugin.appFile` hook (`AppFileLoader = suspend (String) -> ByteArray?`; also `HostPolicy(appFile =)`, `ClosureHost(appFile =)`) under the image limits and cache, resolved before the media policy; no hook = nothing loads (the missing-image fallback). `appFileId(src)`, `LeafImages.appFileBytes` (replays `fixtures/host-policy.json` `appFiles`).
- A call's answer (#VAPP-271): `ExponentialHost` gates and runs an action's result call (`call` + `result: <name>`) before the commit and runs the action again over the same snapshot with the answer as `results.<name>`; a refusal or the function's error fails the action. `Actions.run(…, results =)`, `Actions.Outcome.Waiting(Actions.Pending)`.

## 0.18.0 (2026-11-06)

- A focused text field adopts an action's `set` of its bound value (#VAPP-288): `SurfaceModel.assignData` (the host's package-action sets), the write epoch in `echoFields`; echoes keep the typing rule.
- Tree (#VAPP-285): the arrows, Home and End on a treeitem go to the core (`nodeKey`), which moves the focus or folds.
- A foldable Row (#VAPP-284): ArrowRight / ArrowLeft on a node with `expanded` go to the core (`nodeKey`, its fold keys).
- Hosted popups (VAPP-281): `SurfaceSettings.hostedPopups`, `LocalPopupHost`, `HostedPopup`, `LayerInfo.hosted` / `align`.
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` (VAPP-276): painted from the core's layers; `placeOverlayAligned` in the bindings.

## 0.17.0 (2026-11-06)

- **Package format 5** (#VAPP-260, breaking: a host reads exactly 5): `capabilities: [{id, why, origins?}]`, the capability manifest the Engine's broker and the bundler read (`catalog/host.json` `package.capabilities`, `validatePackageCapabilities` / Rust `host::capabilities`, `docs/package-format.md` "Capabilities"); `fixtures/host-router.json` gains `acme.capabilities` and four invalid packages.

## 0.16.3 (2026-11-06)

- Markdown `imageMaxHeight` is a host render option (`markdown.options`, a number or `null` = fit the width at the image's aspect, capped at the viewport; default 160), and Compose Markdown text follows the surface's font scale and folds at whole lines (`Markdown.foldHeight`).
- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). Compose paints the core's parts (no painter change); the example maps media-volume-up.
- `HostPlugin.markdownOptions` / `HostPolicy.markdownOptions` (`MarkdownOptions`): hard breaks, the chat rhythm, selection (`SelectionContainer` / `DisableSelection`) and streamed text, measured as painted; a document of 24+ blocks composes only the blocks in its visible window.
- #VAPP-258: Markdown host hooks (`fixtures/markdown-host.json`): `TextLinkSegment.chip` paints an inline Chip, `HostPlugin.imageSize` sizes block images, `onImagePress` makes them pressable, `attachment` paints a one-link paragraph as a Video, AudioPlayer or file row. All optional; also on `HostPolicy`. `MarkdownView` takes them as parameters.
- An inline DatePicker paints the core's in-flow month grid; a marked day draws its event dot (VAPP-282).
- Tabs `variant: strip` (#VAPP-279): the core lays the strip out and folds it; a long-press drag on a strip tab reorders (`SurfaceModel.tabDrag`, `isStripTab`).
- `HostPlugin.markdownOptions` / `HostPolicy.markdownOptions` (`MarkdownOptions`): hard breaks, the chat rhythm, selection (`SelectionContainer` / `DisableSelection`) and streamed text, measured as painted; a document of 24+ blocks composes only the blocks in its visible window.
- #VAPP-258: Markdown host hooks (`fixtures/markdown-host.json`): `TextLinkSegment.chip` paints an inline Chip, `HostPlugin.imageSize` sizes block images, `onImagePress` makes them pressable, `attachment` paints a one-link paragraph as a Video, AudioPlayer or file row. All optional; also on `HostPolicy`. `MarkdownView` takes them as parameters.
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).

## 0.16.3 (2026-11-06)

- Markdown `imageMaxHeight` is a host render option (`markdown.options`, a number or `null` = fit the width at the image's aspect, capped at the viewport; default 160), and Compose Markdown text follows the surface's font scale and folds at whole lines (`Markdown.foldHeight`).
- `brand-exponential` (#VAPP-283): the Exponential logo as an icon concept (icons.json `custom`, a filled 24-unit glyph in currentColor), in the Icon vocabulary of every renderer; the Icon specimen's non-Lucide case shows it. The example host maps it in SinkIcons.
- `HostPlugin.markdownOptions` / `HostPolicy.markdownOptions` (`MarkdownOptions`): hard breaks, the chat rhythm, selection (`SelectionContainer` / `DisableSelection`) and streamed text, measured as painted; a document of 24+ blocks composes only the blocks in its visible window.
- #VAPP-258: Markdown host hooks (`fixtures/markdown-host.json`): `TextLinkSegment.chip` paints an inline Chip, `HostPlugin.imageSize` sizes block images, `onImagePress` makes them pressable, `attachment` paints a one-link paragraph as a Video, AudioPlayer or file row. All optional; also on `HostPolicy`. `MarkdownView` takes them as parameters.
- Markdown parses the same on all five renderers (`markdown-render.json`, every golden case passes; VAPP-258), and a number field seeded with a value selects it on focus so Save runs once (VAPP-208).

## 0.16.2 (2026-11-06)

- The icon list follows the app: `power` and the eDE icons (`ede-window`, `ede-lock`, `ede-power`, `ede-suspend`, …) are new.

## 0.16.1 (2026-11-05)

- #VAPP-253: a Drawer's bottom sheet spans the window (the core's layer viewport, no `sheetMaxWidth`) instead of keeping the surface's width.

## 0.16.0 (2026-11-05)

- VAPP-212: `RecordsStore` + `RecordsOverlay`: one app's rows stay in Rust, `sync` pushes only what changed, and `RecordsQuery.value(store, …)` carries no rows across the FFI.
- **Breaking** VAPP-212: `RecordsQuery.value(rows, …)` is gone; use `RecordsStore`.

## 0.15.4 (2026-11-05)

- The open libraries are the Exponential SDK: genUI, Motion and Apps name the skills and docs (`exponential-ui` skill = `exponential-genui`; docs under /docs/sdk/); package names and tags stay. The amber warning colour is `semantic.yellow` (EXP-1695).

## 0.15.3 (2026-11-05)

- The icon list follows the app: `thread-stories` is gone, `thread-results` and `nav-needs-you` are new (FEED-191).

## 0.15.2 (2026-11-04)

- VAPP-250: a scroll container clips at its padding box, so what scrolls never shows through its border or beside a node pinned at the padding edge.

## 0.15.1 (2026-11-04)

- No changes of its own; released with the SDK (EXP-1691's fixes landed in the core).

## 0.15.0 (2026-11-04)

- **Breaking** (over the core) #EXP-1674: `installPackage` refuses a package without `formatVersion: 4`. `Screens.nav`, `Screens.crossAppTarget`, `UrlPolicy.linkBase`.

## 0.14.0 (2026-11-04)

- The U2 record macros and U3 Calendar/Timeline render through the core; `Row.href` / `Button.href` open the URL (VAPP-217, VAPP-232, EXP-1674).

## 0.13.0 (2026-11-03)

- Switch gains `size` (`default` | `compact`): the compact switch is a 28×16 track under a 12px thumb, as on the other renderers (#1853).

## 0.12.0 (2026-11-03)

- VAPP-233: `RecordMigrations.migrate(pkg, version, kind, data)` / `.refusal(before, after)`; `RecordMigrationsTest` replays `record-migrations.json`.

## 0.11.0 (2026-11-03)

- VAPP-222: `RecordTransitions.refusal/from/validate` and `RecordsQuery.value(…, transitions, role)` + `RecordsQuery.transitions(records, kind)`; `RecordTransitionsTest` replays `record-transitions.json`.

## 0.10.0 (2026-11-03)

- No changes of its own; released with the SDK (refs + expand, history and files landed in the core).

## 0.9.0 (2026-11-03)

- No API of its own; carries the core's computed fields (VAPP-214) and sorted install issues (VAPP-201).

## 0.8.0 (2026-11-03)

- VAPP-200: `SurfaceSettings(readOnly = true)`: every writing control paints disabled (the core's read-only build). Conformance suite `read-only`.

## 0.7.1 (2026-11-03)

- No changes of its own; released with the SDK (#1742's specimen look parity and #1744's neutral example names).

## 0.7.0 (2026-11-03)

- VAPP-160: VoiceWave hardening. The rings are computed per frame in pure Kotlin (`VoiceWaveGeometry` + `VoiceWaveStyle` in the primitives: per-edge cos/sin tables, the points rewritten in place; no JSON, FFI call or parse per frame); same points (`VoiceWaveGeometryTest` holds the constants to catalog/style.json `voiceWave` and replays every `fixtures/voice-wave.json` case). The frame loop (rings and `variant: bars`) pauses while the wave is off screen (empty bounds in the window) or the host lifecycle is below STARTED, and resumes where it stopped.
- **Breaking** (over the core): `installPackage` refuses a package without `formatVersion: 3` and validates each kind's record `policy`.

## 0.6.0 (2026-11-02)

- EXP-1589: parity with the web renderer. A cartesian Chart maps its nice-tick range, so the top tick label no longer rides into the title. A node's opacity dims it as one group like CSS, so a disabled Button inside a disabled Form keeps a legible label; nodes with shadows still modulate. A disabled NumberField or ChipInput dims its whole `field` (the core's `disabled` state) instead of only the value. Markdown bullets are a filled disc in the text colour, 0.34 em across, centred 1 em before the text (`MarkdownView` in the primitives too). The CodeBlock copy glyph is 12. A ZoomableImage whose picture has not loaded draws no pill.
- **Breaking**: eApps keeps no backwards compatibility (over the core): `installPackage` refuses a package without `formatVersion: 2` (version 1 is no longer read); `RecordsQuery.parse(params, kind)` takes the kind only from `kind` (the source's name, `records:<kind>`): a `kind` param is a field filter, and Exponential's `exp:records?kind=…` is no records source; `authorId` is a reserved record key; `Actions.effect(name)` no longer knows `harness.records.upsert` / `.delete` (records are written only through `records.upsert` / `.delete` / `.batch`).
- VAPP-194: typed effects (eApps E1). `Actions.run(pkg, name, params, data, me, nowMs, zone, maxSteps)` (the plan `{writes, sets, calls}` or the one error), `Actions.applyBatch(rows, writes, records, me, now, newIds)` (all or nothing) and `Actions.effect(name)` over the core. `ExponentialHost.callFunction` runs `actions.<name>` as the surface's package action: every planned host call's gate and consent first, then the writes as ONE `records.batch {writes}`, then the data sets, then the calls; a failure lands nothing (ACTION_FAILED + `HostOptions.onActionFailed`). New `HostOptions.me` / `now` / `zone` / `onActionFailed`; `ActionsTest` replays `fixtures/actions.json`.
- VAPP-196: package tests (eApps T1) over the core: `validatePackageJson` reports a package's malformed or failing `tests` (`/tests/<i>`), `runPackageTestsJson` runs them; `PackageTestsTest` replays `fixtures/package-tests.json`.

## 0.5.0 (2026-11-02)

- VAPP-191: `Expression.evaluate(…)` / `Expression.check(…)` over the core's expression evaluator, `TimeZoneOffset` (a `java.util.TimeZone` as `HostZone`); `ExpressionsTest` replays `fixtures/expressions.json`. Bind-time `{call: "mul" …}` / `{call: "expr" …}` resolve through the core.
- VAPP-190: `RecordsQuery.parseJson(json, kind)` → `Parsed(query, issues)` (the JSON form: group, aggregate, running, before, in / between / or) and `RecordsQuery.dateBucket(date, bucket)`; `value` answers `groups?`, `totals?`, `before?`. `RecordsQueryTest` replays `jsonCases`, `bucketCases` and the JSON `valueCases`.
- VAPP-189: `RecordsQuery` (`parse(params, kind)`, `parse(source)`, `value(rows, me, now, names)`) and `RecordRow` in `at.exponential.ui.host`: the core's one records evaluator over the FFI; `RecordsQueryTest` replays `fixtures/records-query.json`.

## 0.4.0 (2026-11-02)

- EXP-1605: `Motion` interactivity (eui-motion README › Interactivity): a `MotionDriver` runs the core's dotLottie state machine (`stateMachine`, else a `.lottie`'s first via `motionReadDotlottieMachine`); `inputs` (literal or bound) write on change; a press (tap, keyboard, TalkBack) posts `click` then fires `on.press`, the clickable's press / release post `pointerDown` / `pointerUp`, a machine without `on.press` takes taps itself; `hoverable` posts `pointerEnter` / `pointerExit` + `on.hover`; `on.visible` once; `on.state {state}` per entered state; `complete` posted once per finished play + `on.complete` (also on a final state); every state change restarts the clock. With `on.press` a Motion is a button named by `label`. The tests replay `fixtures/motion-state.json` over the FFI and click a Motion on a composed surface.
- EXP-1604: the `Motion` native: a `Canvas` paints the core's packed display list (`MotionPlayer`, `MotionList` + `MotionPainter` on the Android canvas): `saveLayer` groups with alpha and `BlendMode` (dst-in/out, xor, luma mattes through a luminance-to-alpha `ColorMatrix`), colour-matrix / blur / shadow filters, `DashPathEffect` strokes, linear, radial and two-point conical gradients (interpolated unpremultiplied like the raster), data-URL images; `layers` compile through `motionCompile`, `.lottie` through `motionReadDotlottie`; theme `$color.*` slots + `$mode`; one shared-clock frame per display frame, `progress` scrubs, `play: false` freezes; the poster under reduced motion, `autoplay: false`, while not resumed and past 8 animating per surface; an image named by `label`, hidden without. The tests replay `fixtures/motion-playback.json` and match the raster's parity PNGs.
- EXP-1586: the `ZoomableImage` native: the picture zooms and pans on the core's maths (`zoomJson`, replayed against `fixtures/zoom.json`): pinch about the fingers with the rubber band, a one-finger pan only along an axis with room (a swipe without x room stays the host's), the core's momentum, double tap, ctrl/⌘-wheel, the keys `+ = - 0 1`; THE pill (− / percent with the preset Menu / + / fit) from the `ZoomableImage/*` and `Menu/*` recipes; `zoom {scale, atFit}` (the core writes a bound `scale` back) and `tap`; a set `scale` zooms there. The primitives gain `ZoomableImageView` (any content laid out at the natural size, or a `Painter` / `ImageBitmap`), `ZoomableImageState` and the `ZoomMath` interface (`CoreZoomMath` in `eui-compose`).
- EXP-1493: the Carousel `counter` pill paints at the track's top-END corner (the core's frame, mirrored under RTL); the current page dot takes the `indicator` recipe's `selected` state (neutral: primary), the others its base.

## 0.3.0 (2026-10-31)

- EXP-1495 (round 6): the Carousel `counter` pill (tabular figures, hidden from TalkBack) and `thumbnails` strip (a thumbnail press pages through the Carousel, the current thumbnail kept in view inside the strip) painted from the core's frames; the Outline and Mosaic macros through the Box / Text / Image paths. `ResolvedTextStyle.fontFeatures` (OpenType settings, painter-side).
- VAPP-180 (with the SDK): a package's `exposes` and `upstreams`, validated by the core.

## 0.2.1 (2026-10-30)

- YO-30: the tests replay `fixtures/text-clusters.json` (ICU grapheme clusters). No painter change.

## 0.2.0 (2026-10-30)

- Renamed (EXP-1438): the Maven artifacts `at.exponential:ui-compose` / `ui-compose-primitives` are now `at.exponential:eui-compose` / `eui-compose-primitives` (Kotlin packages unchanged).
- VAPP-162 (round-5 contract): `QrCode` paints the core's matrix (`qrMatrixJson`, `qr-code.json` bit for bit) on its plate, module edges snapped to device pixels, dark on light in every mode; `QrScanner` paints the viewfinder chrome (corners, the 2 s scan line, the `Camera unavailable` placeholder) around the host's camera through the new `LocalQrScanner` (`QrScannerHost.Feed(formats, report, modifier)`; `scanned {value}` once per distinct value); `VoiceWave variant: bars` from `voiceWaveBarsJson`; the vertical Segmented and THE rail (72 × items × 56) with `option.count` badges (`99+`, read after the label); Button `hold` through `Surface.hold` (pointer claimed on down, the core's 24 px slop, Space/Enter/Escape, focus loss cancels; TalkBack's click = one whole hold); `ui-serif` = `FontFamily.Serif`. `Round5Test` replays `qr-code.json`, `voice-wave.json bars` and `interactions-round1.json buttonHold`.
- NEW-77: the `VoiceWave` native paints: three closed waves from the core's `voiceWaveJson` (filled / stroked in the `VoiceWave/outer|mid|inner` recipe colours, the root recipe's box and opacity), animated per display frame while listening, thinking or speaking (still under reduced motion), its one child (THE CHILD RULE) at the core's centred frame at its own size, drawn scaled about the box centre by the core's `child.scale` (min(1, childSize / its longer edge), never up) and kept but unpainted at `sm` (no input, no semantics), in the `VoiceWave/child` colour; TalkBack reads one image `<label>, <activity>` (hidden without a label). `ui-compose-primitives` gains `VoiceWaveView` (with a child overload) / `DrawScope.drawVoiceWave` / `Modifier.voiceWaveChild`; `fixtures/voice-wave.json` replays in `Round2InteractionTest` within 0.5 px (the child box too) and `ControlPaintTest` measures the drawn child against its `child` cases within 0.5 px.
- VAPP-122: `bind` is an action the host runs like `openUrl`: `{path, source, params?}` re-follows a path (`sourceWithParams` over the core's `sourceWithParamsJson`), subscriptions are keyed by path so the earlier one at that path is cancelled; the conformance suite replays the new `sourceParams` and `records` cases over `validateRecordJson`.

## 0.1.1 (2026-10-23)

- Version-only release: the SDK's npm packages moved to the `@exponential_app` scope and every package shares one version. Coordinates unchanged (`at.exponential:eui-compose`, `at.exponential:eui-compose-primitives`); same contents as 0.1.0.

## 0.1.0 (2026-10-23)

First release.

- VAPP-146: `selectPick` of the option that holds the current value (`""` when unseeded) fires nothing; `fixtures/interactions-round1.json` `select` replays in `InteractionTest`.
- VAPP-126 (review #1187): a `display: none` box arrives as `visibilityHidden` from the facade: its zero-framed subtree is inert and hidden from accessibility, as on the web.
- `:ui-compose`: `SurfaceModel` + `ExponentialSurface` paint an A2UI surface at the frames the Rust core computes on Android 8+. The AAR bundles the UniFFI facade (arm64-v8a, armeabi-v7a, x86_64) and its Kotlin binding.
- `:ui-compose-primitives`: generic Compose primitives themed by `PrimitiveTokens`.
- Host-owned text fields set the Input `type`'s keyboard, autocorrection and autofill hints (email, password, phone).
- Material sliders keep the author's range. `step` 0 is continuous; a step that divides the range is passed as M3 `steps`.
- An extension the core refuses fails `SurfaceModel` creation; it is no longer skipped silently.
- Video and AudioPlayer play their `src` through the media policy with Media3 ExoPlayer (`androidx.media3:media3-exoplayer` + `media3-ui` 1.11.1): poster and duration until the press, `autoplay` muted, a header-carrying or `data:` request fetched within `media.limits` into a cache file; a denied src loads nothing.
- Media redirects: each hop is re-policed (a hop off a rule's prefix loses its headers) and an https → http hop is refused.
- Video / AudioPlayer: http(s) streams with its headers after a policed redirect probe, pinned to the resolved origin (no byte cap, so authed media over 20 MB plays); only `data:` goes to a file, uncapped.
- Video / AudioPlayer: one open per player; a second press waits (spinner, control disabled), another src cancels it, a replaced player is released.
- `data:` media files are refcounted and deleted by the last player, at most 8 kept, the cache directory emptied on the first use per process.
- An `Image` without `alt` paints only the glyph (no English "image" label).
- `ChoiceTiles` (EXP-1280): the core expands it into tiles; a tile reads as a radio (single) or a checkbox (multiple) with its picked state, arrows rove the tiles (a single choice follows focus), and an unexpanded one measures as its grid. `:ui-compose-primitives` adds the standalone `ChoiceTiles` primitive (picture over label + muted description, 2 dp accent ring on a pick, radius 10, gap 8, picture 1.6 : 1).
- Markdown inline parsing is linear on hostile input (unclosed destinations, bracket and emphasis floods); link labels nest at most 32 deep.
- Paint failures are keyed by component: a failed node paints empty, unreported, until ITS props change; it drops when it leaves the surface. The host dedupes per surface + component and forgets one when `updateComponents` names it.
- Markdown block images align to the top of their box.
- `SurfaceModel.snap` reaches `max` when the step does not divide the range.
- The example's `brand` theme loads with `ThemeHandle.loadOrDefault`; a unit test loads every shipped sample theme strictly.
- Text `shimmer` paints a moving gradient (`textShimmerJson` colours, plain under reduced motion); `Trace` / `TraceItem` and CodeBlock `variant` / `size` / ANSI tokens paint from the core; CodeBlock copy copies the `copy` part's `text` (ANSI escapes stripped).
- Faster passes (VAPP-134 to 138): nodes are patched from the layout's delta (`nodesAt`), visuals and text styles are re-read only for the changed nodes and the leaves under them, theme scalars and wrapped line counts are memoised, and ICU formatters load on first use. A 10,000-item template paints in half the time; a Table scroll step costs 0.5×; the facade moves 57 % less data over a stream.
- `SurfaceModel` reads each step's paint with ONE batched `Surface.paintAt` (the touched slots' visuals and the text styles of the live leaves under them) instead of a `visual(i)` / `textStyle(i)` call per slot. Facade calls per list scroll step 143 → 82; on the emulator a streamed message applies 2.1× faster (p95 29 → 9.4 ms), a mode switch 3.9×, a 10,000-item rebind 1.3×. Budget exception: the streaming scenario's `ffiBytesTotal` rises 2.4 % (5,212,191 → 5,336,991 B, each re-read leaf carries its visual in the batched read) for 43 % fewer facade calls per step; no timing budget moved.
- Robolectric unit, conformance and snapshot suites, plus instrumented tests (`src/androidTest`) that run on an emulator in CI.
