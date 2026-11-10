# Changelog: `eui-gpui` (desktop painter, not yet published)

All notable changes to this package. The SDK packages share one version and
release together from a `eui-v<version>` tag (`packages/exponential-ui/release/README.md`).

## Unreleased

- Popover `matchAnchorWidth` (the core's exact layer width): the overlay suite paints the fixture's `matchAnchorWidth` cases and checks the panel's width against the anchor's.
- Composer `layout: "flat"` (#EXP-1762): the field sits between the `leading` slot and the attach control / `trailing` / send, centred on the disc while one line, the row's full height once it grows; the measure wraps it at that width. A one-line draft's TEXT is centred on the disc: the gpui-component Textarea sits its own editor inset (`Size::Medium` `input_py`) above the line box (it drew the text 8 px low; the IDE composer bench, #EDE-65).
- `ColorPicker` (#EXP-1775): the 28 px cells, `inline` and the child trigger painted from the core's natives; the IDE's Board detail picker moves from the 24 px to the 28 px cell.
- Text wrap (#EXP-1773): a word fits by gpui's `LineWrapper` rule. The spaces after it count, and there is no slack, so a word ending within a space-width of the edge moves down, as in the IDE's own text. A line's last word (nothing after it) still fits on its ink, with 0.5 px of float slack. Change email's address step (a `text_sm` description in a 712 px column) wraps "…your old | address stops…" like the gpui dialog (its text 0 px, was 18,372 px / 151 levels). Every benched `settings_bench` state is unchanged (0 px). The painter still draws the measured lines verbatim (`white-space: nowrap`, no gpui re-wrap), so drawn and measured lines cannot disagree. A line can overhang its box only by that ≤ 0.5 px slack.

- Select `triggerIcon` + the own-colour marks (#EXP-1764): an Icon leaf whose name has its own fill (`eui_core::layout_tree::icon_fill`, `brand-claude`) paints in it, never in the ink; the `row` trigger's `valueIcon` comes from the core. Bench `settings_account_row_*`: the IDE Account row (the Medium picker trigger: a 14 px mark, 7 to the email, 7 to the chevron) vs the genUI row, 0 px; the IDE's agent marks are the `brand-claude` / `brand-codex` concepts and `theme::CLAUDE_BRAND` is exactly `#D97757`.
- `IconPicker` + `ColorPicker` (#EXP-1760): against the IDE's `board_form::icon_picker` / `color_picker` in `ui/src/picker_bench.rs` (closed and open).
- Textarea autosize + Composer (#EXP-1768): a bound value written after the first paint (a body that loads, a host write) grows the field. The pass measured the field's live text BEFORE `sync_fields` adopted the write, and gpui-base's `set_value` emits no Change, so the field kept its old (empty) height and scrolled; the adopted write now re-measures it. Settings → Skills' 30k-char body grows to its 40 rows (0 px vs the gpui pane in `settings_bench`).
- Feature `vendored-spinner` (off by default, #EXP-1629): `view::init` registers the Spinner arc as gpui-component's loading glyph (`gpui_component::spinner::set_spinner_glyph`) only with it on. That hook exists only in the desktop workspace's vendored gpui-component, so the painter builds again against crates.io `gpui-component = "=0.9.0"` (the gpui guide check). The workspace crates turn it on next to `overlay-anchor`.
- Select `pill` (#EXP-1765): the trigger's glyph takes the `chevron` part's colour (opacity 1 unless its recipe fades it; `field` keeps 0.6) and its `value`/`placeholder` resolve with the trigger's `disabled`; a mono option paints its `trailing` as a muted sans run before the check; a Select search's glyph takes the `searchIcon` part's width; a `pill` search shows a clear button while it holds text (`Select.clear`, 0.75× the search glyph in a 1.25× box, "Clear search") that empties the field and keeps the focus; a pointer-opened `pill` popup shows no cursor until the pointer moves or a key is pressed. Bench: the IDE Repositories branch picker vs a genUI pill Select (`settings_bench` `settings_repo_branch_*`): trigger 0 px, disabled 8 px (1 level), ready 7,049 px / empty 6,857 px (≤ 5 levels, the popup's shadow ring).
- Select option rows (#EXP-1629): painted from the core's parts (no painter change). 0 px against the IDE's device picker (`picker::device_picker`) closed and open, a long device name included, apart from the popup's edge/shadow ring (at most 8 levels).
- `controls::spinner` draws THE Spinner's arc (#EXP-1629): `paint::natives::spinner_glyph` (Lucide `loader-circle`, one linear turn per 0.8 s, stepped), the glyph genUI `Spinner` paints, so the IDE's spinners match web, iOS and Android (0 px vs genUI `Spinner` headless). `ChromeIcons.refresh` is gone (it only fed the spinner).
- Icon `size: xl` (#EXP-1629): 28 px from the recipe (`$control.iconXl`).
- Input `variant: "box"` + `size` (#EXP-1629): from the recipe alone. Against the IDE's `glass_input(..).web_input_sm()` / `.web_input()` (`settings_bench::box_fields`): 160 px, all ±1 on the rounded corners' antialiasing (the IDE fill is `input` × 0.3 = white 7.8/255, a theme hex says 8/255; the IDE drawn with `#ffffff08` is 0 px).
- Textarea autosize + Composer: the measurer counts the rows gpui-component's Textarea actually draws (`text::field_breaks` / `field_rows`, a port of gpui-base 0.9's `measured_wrap`): a word wider than the row (a long URL, a hash) breaks at the last grapheme that fits instead of measuring as one overflowing line, continuation rows keep the leading-space indent, and every `\n`-line (a trailing one too) is a row. A 12k-char unbroken line in an autosize Textarea (rows 10 → maxRows 40) now grows the field to 40 rows instead of scrolling at 10. The Composer's typed text uses the same rule (its placeholder still measures as text).
- Textarea autosize (#EXP-1629): an intrinsic measure pass (no width yet) counts the value's hard lines only, instead of wrapping it at a guessed 160 px, which left an enabled one-line `row` Textarea a row taller than its rows (the Skills page's description, 3 rows for 2).
- Textarea `row` / `code` (#EXP-1629): the gpui-component Textarea is placed its own editor inset (`Size::Medium` input_px × input_py) outside the field's content box, so the recipe padding is the whole inset, as the IDE's `Textarea.appearance(false).px_4().py_3()` draws it. 0 px against the IDE's agent instructions, skill description, SKILL.md / prompt body and the disabled team prompt; Input `bare` / `code` against `glass_row_input` and the skill name row.
- Composer slots + paste (#EDE-65 R): the attach control moves before the core-laid `trailing` slot; a paste into the field (`Textarea::on_paste`) with images (copied image files, a raw clipboard image written to a temporary `image.<ext>`) fires `attach` {files} and `on_upload` (`name` = the Composer's id), files without an image insert nothing. `SurfaceView::composer_paste`, `clipboard_images`, `clipboard_has_files`.
- Select (#EXP-1629): the row's `button` lends its hover to the value, placeholder and chevron inside it (theirs alone is never reported); a `multiple` popup opens with the cursor on its first chosen option. Against `Picker::multi` in a `glass_picker_row` at the real 14 px rem: 0 px at rest, hovered and after a pick (both close); open, only the popup's shadow ring (≤ 8 levels, the single picker's too) differs.

## 0.21.0 (2026-11-09)

- Embed (#EDE-65 lane M, `view/embed.rs`): a nested `SurfaceView` painted in place, as wide as its slot and as tall as its content (the measurer lays the child out at the leaf's width; a child that grows lays the host out again), built from `eui_core::embed::embed_mount` of the resolved `surface` (the error box for a bad one) and cached per painted id + value (a new value rebuilds it, a gone node drops it). It inherits the host's theme, mode and settings (its popups stay in-surface) under the surface id `<host>/<painted id>`; its host plugin forwards the icon, media, URL policy, `open_url`, text-link and markdown hooks, re-fires every inner `event` action on the host's Embed as `action` `{name, context}` (merged over `on.action`'s context; the host's `on_action` hears it) and DENIES inner host function calls (never forwarded, their event does not follow). `SurfaceView::embedded(id)`. `tests/embed.rs` replays `fixtures/embed.json`.
- Composer `menuKeys` (#EDE-65): a keystroke interceptor, alive while a Composer has menuKeys on, hands Up, Down, Enter, Tab and Escape without a modifier on the focused Composer to `key` {key} ahead of the textarea's bindings (never while the IME has marked text); `SurfaceView::composer_key` for automation and tests.
- `measure::prewarm_fonts(text_system, default_family)` (#LOGS-6): resolves the faces a surface paints with (the sans and the first loadable mono face, each with its fallback chain, in every weight and style) and returns them, so a host can load the families and build their fallback cascades on a background thread before the first surface paints.
- Input `row` (#EXP-1629): the field paints `glass_input_row` (the `Input/label` recipe's type leading, never shrinking; the bare field end-aligned in a `flex_1` column at 70% of the field's ink; the placeholder and caret are gpui-component's), measured as label + gap + field. 0 px against `glass_input_row` in the exponential theme.
- Glyph layer (#LOGS-4, `controls/glyph_layer.rs`): `stepped_glyph(id, size, animation, step, paint)` is a long-lived animated glyph that, inside a `glyph_scope` around a cached `GlyphRegion`, does not paint in place: it records its bounds, clip and text style, and the window's `GlyphLayer` view (rendered after the content) repaints only the glyphs on the shared step grid, so a working mark's frame no longer re-renders the cached region around it (gpui re-renders every ancestor of a notified view). A region that re-renders without the glyph, or a scope that leaves the tree, drops it; outside a scope it paints in place. The painter's loader `spinner_glyph` is one. Every stepped animation and glyph shows its static face and schedules nothing while its window is unfocused or under Reduce motion (`animation_paused`); gpui refreshes a window when its focus changes.
- Select (#EXP-1629): the trigger paints its parts from their recipes (`natives::select_trigger`: the `value` or `placeholder` text with its colour and opacity, the `chevron` at the recipe's size and opacity, the trigger's `gap`); a `row` trigger leads with its `label` (the `Select/label` recipe's weight) and ends the value. An option's `hover` is the popup's ONE cursor: the keyboard focus (opening on the first chosen option) and the pointer move it, keyboard moves scroll it into view, and an option draws no focus ring. The `check` takes its recipe's size and colour.
- KeyRecorder (#EXP-1629, `view/key_recorder.rs`): the macro's `field` presses like any Box (a click, Enter/Space while idle); while its `recording` recipe prop is true it takes the focus and an app-wide keystroke interceptor sends EVERY key down to `record {type: down, key, ctrl, alt, shift, meta}` ahead of the window's keybindings (no Tab move, no Escape, no shortcut), key ups as `{type: up}`, modifier changes as `{type: modifiers}` and mouse buttons 3-5 over the field as `{type: mouse, button}` (1 and 2 send nothing). `key` is the canonical name (a-z, 0-9, f1-f24, the named keys, US punctuation; gpui's shifted character maps back to its physical key with shift); anything else and key repeats send nothing; meta = gpui `platform`. Losing the focus while recording fires `blur`; a host write of recording false drops the capture.
- One animation grid (#LOGS-3): every `with_stepped_animation` and `repaint_on_next_step` repaint lands on a tick of ONE 33.3 ms grid the app shares, and a loop's start moves back onto its step's line, so loops at different steps (a 33 ms spinner, a 90 ms sprite, a 100 ms label) share frames instead of adding up: a window redraws at most 30 times a second whatever animates. New `PULSE_STEP` (every second tick, 15 fps) for a long-lived opacity pulse. A looping keyframe `animation` (infinite iterations: a `spin`, a `pulse`) repaints on the grid instead of requesting every frame; a finite one keeps every frame. Each stepped element opens an `anim.step` trace span labelled with its id while tracing listens.
- gpui revisions: zed `cc053a4a` (git) → `gpui-pre` 0.3.11 (crates.io, zed `f816e1f`); gpui-component `da4f9369` (git) → 0.9.0 (crates.io, longbridge/gpui-kit); rust 1.96.0 → 1.98.1. A breaking change for consumers: take gpui as `gpui = { package = "gpui-pre", version = "=0.3.11" }` and `gpui-component = "=0.9.0"`, drop the ztracing `[patch]` (zed relicensed it Apache-2.0) and the `cargo update --precise` step (README "gpui revisions"). Ships as the SDK's next minor.
- `net`: reqwest 0.12 → 0.13 (`rustls-no-provider`) and tokio-tungstenite 0.24 → 0.30. The first http(s) transport or media fetch installs rustls's ring provider as the process default unless the host installed one already, and certificates verify against the OS trust store (rustls-platform-verifier) instead of the bundled webpki roots.

## 0.20.0 (2026-11-08)

- Scrollbars (#EXP-1629): a scroll container's overlay scrollbar is gpui-component's own `Scrollbar` over the core's offset (`view/scrollbar.rs`), so an eUI surface in the IDE shows exactly the IDE's scrollbars: the theme's thumb, width and inset, shown when the offset changes (wheel, `scrollToIndex`, a drag), fading out after gpui-component's idle delay, wider under the pointer, draggable. It replaces the EXP-1555 bars (6 px at 2 px, shown on hover and for 1 s after a scroll). No catalog or prop change.
- Tabs `swipeable` (#FEED-200, `view/swipe.rs`): a two-finger trackpad swipe pages the Tabs like a finger. gpui's phased `ScrollWheelEvent`s (`ScrollDelta::Pixels`, `TouchPhase` Started/Moved/Ended) over the content send `swipe {phase: start|move|end, offset, velocity}` once `swipe_lock` calls the travel horizontal (vertical = the page's); a gesture starting over a node `swipe_blocks` names (Table, Chart, Slider, a horizontal scroller…) stays that node's; the fling's momentum never pages again; a mouse wheel (`Lines`) never swipes; a gesture silent for 500 ms settles. The panels and the thumb paint the core's `transform` / `transition` as any node does.
- Chart ticks, value labels and the tooltip show the core's labels in the chart's `valueFormat` (#FEED-205).
- Pixel snapping (EXP-1629): `SurfaceViewOptions::pixel_snap` (on by default) snaps a surface to whole device pixels at the window's scale factor like gpui's layout, so an eUI document paints the same pixels as the gpui view it replaces. A view under a measure override never snaps; the conformance dump turns it off (it compares the core's fractional frames with the web). While snapping, a text's measured width rounds up to a whole pixel, as gpui's text element does.
- A `native: true` style no longer applies to a Switch track: genUI's Switch is the IDE's since #EXP-1745, so the borrowed `controls::web_switch` path is gone and a Switch always paints the eUI look (#EXP-1629).
- `binding` (#EXP-1629): the native host binding, a host's own screen drawn as a document. `Binding<S>` owns the screen's state and writes its projection at one data-model path (re-projected on `observe`d entities, an unchanged projection writes nothing); a named action or a committed text edit maps to a `Mutation` (applied at once, committed off the main thread, reverted and toasted through the host's `Toasts` on failure; a `key`ed failure never reverts a newer edit; a commit older than the field's last is dropped). `BindingHost` routes the document's actions and inputs to it and everything else to the host's own plugin.

## 0.19.1 (2026-11-07)

- Released with the SDK at 0.19.1: the Swift and Compose hosts check an app's calls against its upgraded package, so a format-5 app's `records_upsert` is no longer refused silently (VAPP-272).

## 0.19.0 (2026-11-07)

- Round 9 parity: Markdown's inline code paints the `Markdown/code` recipe in its own type over a placeholder (a span wider than its line stays a breaking mono run), a disc at every list depth, an ordered marker one space before the text (`fixtures/markdown-inline.json` `documentPaint`).
- Canvas (round 8, `docs/round-8-contract.md` §1): a Canvas leaf (`canvas::CanvasPlayer`, `view/canvas_leaf.rs`) compiles `{size, shapes}` in the engine (recompiled on a `size` / `shapes` change, resampled with the theme's slots on a theme or mode change, no playback), rasters the shapes with tiny-skia only when the box size, the fit or the slots change, and sets every `text` op natively ABOVE the image: the anchor through fit × transform, the font px scaled by it, `$type.family.sans` / `.mono` at the CSS weight, physical align, the top / middle / alphabetic baseline from gpui's font ascent and descent, clipped by the box only. An img named by `label` (decorative without one), never a tab stop or a button. With `pointer: true`: `press {x, y}`, `move {x, y}` at most once per frame step (the last held move fires at the step) and `leave`, in drawing units (`canvas::to_drawing`). `SurfaceView::canvas_error` / `canvas_list` / `canvas_texts` / `canvas_rasters` for tests and automation.
- The Motion painter takes display list v2: `text` ops (none from a Motion today) are set natively over the raster by the same code (`MotionPlayer::texts`).
- Checkbox `indeterminate` (round 8 §2): a mixed box paints the checked look with the `Checkbox.check.indeterminate` dash (`ui-minus`; a `native` box falls back to it, gpui-component has no mixed state) and reads `Toggled::Mixed` (mixed wins over checked; `SurfaceView::toggled_state`); the press semantics are the core's.
- Progress `valueText` (round 8 §3): the `value` part is shaped with tabular figures (`tnum`) in the measurer and the painter (`measure::tabular_text`, keyed on the macro's `<id>.header.value`).
- Stepper `collapse` / `@container` (round 8 §4): through the core's layout pass, no painter change.

- An app's own file as a src (#VAPP-271): `HostPlugin::app_files() -> Option<Arc<dyn AppFiles>>` (default `None`; `AppFiles::read(file_id) -> Option<Vec<u8>>`, any `Fn(&str) -> Option<Vec<u8>> + Send + Sync`); an `app-file:<fileId>` Image / Avatar / poster / Markdown image src reads it off the main thread under the image limits and the asset cache (`media::image_key`, `MediaKey::app_file`); no app files, a bad id or a missing file = the missing-image fallback.
- A call's answer (#VAPP-271): `run_package_action` gates and runs a `call` step with a `result` when the plan reaches it (before the commit; a refusal or a failure is the action's failure, nothing lands) and runs the action again over the same snapshot with the answer as `results.<name>`.

## 0.18.0 (2026-11-06)

- Tree (#VAPP-285): the arrows, Home and End on a treeitem go to the core's tree keys.
- A foldable Row (#VAPP-284): ArrowRight / ArrowLeft on a node with `expanded` go to the core's fold keys.

## 0.17.0 (2026-11-06)

- #VAPP-258: two gpui host extras for an app's own Markdown: `HostPlugin::chip_icon` paints an inline chip's icon (a status glyph in its tint), `tasks_pressable` / `on_task_toggle` make task boxes pressable (the host rewrites its text); `markdown_hooks_version` memoizes a leaf's laid-out blocks (`paint::markdown_memo`), `markdown_visible_only` builds only the blocks near the host's viewport, `markdown_window_selection` joins the text units to the host's window-level selection (gpui-base, `paint::markdown_selection`).
- An inline DatePicker paints the core's in-flow month grid; a marked day draws its event dot (VAPP-282).
- Tabs `variant: strip` (#VAPP-279): a strip tab press becomes a reorder drag past `tabDragThreshold` (the press is spent); Ctrl/Cmd+Shift+Arrow moves the focused tab.
- #VAPP-258 step 5: `HostPlugin::markdown_options` (`MarkdownOptions`, also `HostPolicy::markdown_options`): hard breaks, the chat rhythm (`density: chat` recipes), selection off, streamed text (`parse_with`, `parse_streaming`, a per-text memo). A document of 24+ blocks lays out and paints only the blocks crossing the clip (`LazyMdBlock`), the measured height unchanged. A copy across a chip gives its label; a chip box spans its whole placeholder (no stray gap after it).
- #VAPP-258: Markdown host hooks: inline chips on `text_links` segments (`TextLink.chip`, measured as painted), `HostPlugin::image_size` / `image_pressable` / `on_image_press` for block images and `HostPlugin::attachment` for one-link video, audio and file blocks (also on `HostPolicy`).
- `SettingsRow` (#VAPP-280, round 7): one line of a settings page as a catalog macro: the label (+ a muted description) on the start column, the control (its children) on the end column, 16 px in, 12 px top and bottom, at least 44 px tall; below `stackBelow` (default `sm`; `never` keeps it inline) the control goes under the label and stretches. A group named by the label. Every renderer paints it through the core's expansion; prompt budget 9,292 → 9,359 tokens (a named exception).

## 0.16.3 (2026-11-06)

- #VAPP-258: two gpui host extras for an app's own Markdown: `HostPlugin::chip_icon` paints an inline chip's icon (a status glyph in its tint), `tasks_pressable` / `on_task_toggle` make task boxes pressable (the host rewrites its text); `markdown_hooks_version` memoizes a leaf's laid-out blocks (`paint::markdown_memo`), `markdown_visible_only` builds only the blocks near the host's viewport, `markdown_window_selection` joins the text units to the host's window-level selection (gpui-base, `paint::markdown_selection`).
- An inline DatePicker paints the core's in-flow month grid; a marked day draws its event dot (VAPP-282).
- Markdown `imageMaxHeight` is a host render option (`markdown.options`, a number or `null` = fit the width at the image's aspect, capped at the viewport; default 160), and Compose Markdown text follows the surface's font scale and folds at whole lines (`Markdown.foldHeight`).
- Slider `icon`, `endLabel`, `showValue` (#VAPP-286): a leading glyph, short muted text after the track (bindable) and `showValue: false` (no value above the track); the core lays the volume row out as a `row` part (gap sm: icon iconSm, the growing track, the end label at least buttonIcon wide). New parts `row`, `icon`, `end`; prompt budget 9,216 -> 9,237 (a named exception). gpui paints the core's parts (no painter change).
- `HostPlugin::on_hosted_popups`, `SurfaceView::hosted_popup_element` / `dismiss_hosted_popup` (VAPP-281): hosted layers paint in the host's popup, never in the surface.
- Popover `align` + `anchor`, Menu `side` + `align` + `anchor` (VAPP-276): painted from the core's layers; the overlay suite replays the aligned and anchor-by-id cases.
- #VAPP-258 step 5: `HostPlugin::markdown_options` (`MarkdownOptions`, also `HostPolicy::markdown_options`): hard breaks, the chat rhythm (`density: chat` recipes), selection off, streamed text (`parse_with`, `parse_streaming`, a per-text memo). A document of 24+ blocks lays out and paints only the blocks crossing the clip (`LazyMdBlock`), the measured height unchanged. A copy across a chip gives its label; a chip box spans its whole placeholder (no stray gap after it).
- #VAPP-258: Markdown host hooks: inline chips on `text_links` segments (`TextLink.chip`, measured as painted), `HostPlugin::image_size` / `image_pressable` / `on_image_press` for block images and `HostPlugin::attachment` for one-link video, audio and file blocks (also on `HostPolicy`).
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
