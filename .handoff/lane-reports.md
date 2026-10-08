

## contract (Contract)

## Done

**New authoring features (contract only; the renderers implement them)**
- **`Resizable`** is a new native component. Props: `direction` (responsive), bindable `sizes`, `panels` limits, `handle` and `style`. It also has a `panel` def, recipe parts, an a11y entry, the `$string.resize` label and neutral recipes. The panel arithmetic is in `src/resizable.ts`: normalize, drag from start sizes, keys (rtl aware), extents and px→% conversion.
- **Style:**
  - `position: sticky`.
  - `backdropBlur`, which only takes `$blur.*` tokens. The new `blur` token group is defined in neutral and the other themes inherit it.
  - `animation` (8 keyframe sets in `style.json` `animations`) and `animationDuration`. Timing comes from motion tokens, easing is per segment, reduced motion shows the rest frame (`src/animation.ts`, which also emits the CSS keyframes).
  - `direction` is now allowed on any node and is inherited. Text alignment resolves start/end against the node's own direction (`src/direction.ts`).
- **Formatter** (`src/format.ts`):
  - A `Formatter` interface; TS gets `intlFormatter(locale, timeZone)`.
  - An `englishFormatter` fallback whose data lives in `locale.json` `englishFallback`. A test checks it gives the same output as Intl en-US on every fixture call.
  - Two new core functions, `formatPercent` and `formatRelativeTime`.
  - The six format functions are in the bind table and run through `ResolveOptions.formatter`.
  - `SurfaceSettings.timeZone` is new.
  - `displayString` defines what a text prop shows for a bound number or boolean.
- **Strings:** 8 new ids for the labels renderers still hard-code: `invalidValue`, `message`, `codeBlock`, `dialog`, `table`, `carousel`, `slide`, `resize`. The hard-coded spots in React, gpui and Compose are listed in the doc.

**Contract decisions, each with a fixture (TS is the reference; Rust work is in the checklist)**
- **Template nodes are lifted out of the tree** into a new `ReduceResult.templates`, on both the nested and the flat path, including nested templates. An unknown template id is reported as an issue.
- **Keys:** missing, empty or duplicate list/Table keys become `#<index>`, and instance suffixes accumulate through nested templates (`src/list.ts`).
- **Per-node direction and text alignment**, covered by `fixtures/text-direction.json`.
- **Switch:** label first, switch at the end.
- `bindRowSlotJson`, `bindSectionHeaderJson`, layout reading props through `resolve_node_props`, and `display_string` replacing `as_str` are specified for the Rust/FFI lane.

**Virtual lists**
- Windowing works on one axis, so horizontal lists use the same rules.
- New `List` props: `sectionBy`, `stickyHeaders` and a row-scoped `section` slot (bound with `sectionScope`).
- `scrollToIndex` is a new host command.
- Lists without a bounded height window against the nearest scroller or the host viewport.
- `fixtures/bench-list.json` defines the shared 100,000-row bench.

**Conformance**
- Every part a painter lays out is now listed in `recipes.json` (11 parts were added), with the part-id naming rule.
- The web dump now measures layout boxes (transforms and animations off) and skips inactive carousel pages.
- Real Nunito and Fira Code fonts (OFL, licence files beside them) are committed under `conformance/fonts/` and kept out of the npm package.
- The manifest is now version 2: 24 suites, 1609 cases.
- `catalog/layout.json` holds the explicit sizes that replace browser defaults: field/media intrinsic widths, 16:9 media ratio, window threshold and overscan, Resizable step/min/hit area, TreeGuides column.
- Video has a new `aspectRatio` prop. Chart `height` now means the whole chart box. Overlays take their trigger's box.

**All 53 divergence causes have a decision** in `fixtures/conformance-known.json`. Each `causes` entry says which side is right, who fixes it and the reason; a separate `rules` list holds the decisions with no current origin (Switch order, Checkbox overlap, field text inset, Accordion count, image fallback, carousel pages, rtl bidi, line breaks, part coverage). The only cause fixed on my side is the `tab-bar` icon one (the dump measured transforms).

**Specimens, prompt, docs**
- Specimens are regenerated; Resizable and the round-1 containers now get real content (Sidebar nav, AppBar action, triggers, Form fields, ScrollArea rows). The kitchen sink ends with a `panels` Resizable.
- The prompt is at 6475 of 6500 tokens (lite 5257, terse 4803).
- `docs/round-2-contract.md` covers every change with checklists for React, Rust core + FFI, gpui, SwiftUI and Compose. The package README and conformance README are updated.

Key files:
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/docs/round-2-contract.md`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/src/{format,list,resizable,direction,animation,layout}.ts`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/scripts/generate-round2.ts`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/fixtures/{format,template-items,text-direction,resizable,virtual-list,animations,bench-list,conformance-known}.json`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/catalog/layout.json`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/conformance/{dom.ts,fonts.json,fonts/}`

## Contract notes

- **React must read `ReduceResult.templates`.** Right now React finds a nested template by searching the tree, which no longer works. In my one-case web dump the kitchen-sink `list` rendered 0 items.
- **Do not rewrite the web baseline yet.** The fonts change, the lifted template, `panels` (currently "Unknown component" on React) and the dump rules all change the web dump. Until the baseline and `conformance-known.json` counts are rewritten after the React lane, the gpui ratchet will fail (worst on playful, from the font switch).
- **The gpui writer must keep `causes` and `rules`** when it rewrites `conformance-known.json`; today the ratchet only reads `cases` and would drop them.
- **Breaking changes:**
  - `direction` is no longer root-only.
  - The `blur` token group is required on every root theme.
  - List is now row-scoped (it has a `section` slot).
  - `cellType` gains `currency`, `percent`, `relativeTime`; `column` gains `currency`, `decimals`.
  - `bindFunctionNames` gains the format functions.
  - Chart `height` semantics changed (whole box).
  - The conformance manifest is version 2.

## Deferred

- **Renderer implementations, and lowering the gpui ratchet:** the per-lane checklists are in §9 of the doc. Smallest next step: React adopts `templates`, then rerun the baseline + known rewrite procedure in §8.
- **Licence row:** `docs/third-party-licences.md` is outside my paths. Proposed row: "Exponential UI conformance fonts (round 2, 2026-10-08): `packages/exponential-ui/conformance/fonts/` holds Nunito (The Nunito Project Authors, SIL OFL 1.1; seven static instances generated from Google Fonts' `Nunito[wght].ttf`/`Nunito-Italic[wght].ttf` with `fonttools varLib.instancer --update-name-table`; no Reserved Font Name) and Fira Code 6.2 (The Fira Code Project Authors, SIL OFL 1.1; unmodified release TTFs), each with its `OFL-*.txt`. Used only by the conformance harness; no app, image or npm package ships them (`release/prepare-npm.ts` skips the folder), so nothing reaches a NOTICES.txt."
- **View-catalog entries:** `packages/view-catalog` is outside my paths. Ids to add: `exponential-ui-{scroll-area, sidebar, app-bar, code-block, kbd, label, breadcrumb, tab-bar, stepper, alert-dialog, hover-card, context-menu, toast, sparkline, form, number-field, rating, chip-input, date-range-picker, time-picker, file-upload, resizable}`. Captures belong to the shot and Mac runs.
- **gpui backdrop blur:** the pinned gpui has no backdrop sampling, so the fallback is the background colour alone.
- **CardRow cause:** gpui is 30 px off on some sibling part; it can only be pinned down once React marks every CardRow part.
- **Smaller deferrals:**
  - Calendar-aware relative days. Next step: a `calendarDays` option on `relativeTime`.
  - Resizing that pushes past the neighbour panel into the next one.
  - Per-locale string tables.

## Tests run

- `bun run --filter @exponential-at/ui generate` twice: the second run left all 40 outputs unchanged.
- `bun run --filter @exponential-at/ui test`: 320 pass, 0 fail. This includes the new `src/round2.test.ts`, which replays every round-2 fixture and checks that the English fallback equals Intl en-US.
- `bun run --filter @exponential-at/ui typecheck`: exit 0.
- `CARGO_TARGET_DIR=/tmp/xui-r2-target cargo check -p exponential-ui --lib`: compiles against the regenerated mirror.
- `cargo test -p exponential-ui` (lib): 69 pass, 6 fail. All six are from the contract change and belong to the Rust lane: the a11y commands list, `direction` no longer root-only, the recipe part count, and three theme tests that need the `blur` group.
- `bun run --filter @exponential-at/ui-react test`: 21 fail, all expected and for the React lane: no Resizable painter, the lifted templates, new `stickyHeaders` snapshots, and one basic-map template case. Vitest wrote 4 snapshot lines into React's `fixtures.test.tsx.snap`; I restored that file from HEAD.
- `bun run ./conformance/run.ts --only kitchen-sink/playful/dark/390/ltr --skip-desktop --out /tmp/xui-r2-contract-conf`: the harness runs with the new dump rules and fonts and shows the expected web drift. I did not rewrite the baseline.
- `CLAUDE.md` is unchanged (39,986 chars).


## impl:docs (Implement)

## Done

The component pages are rebuilt, the site has a light scheme and a phone menu, and every main page except the builder scores 95+ on mobile Lighthouse. Tests, typecheck and build pass. The marketing site still builds. Nothing committed.

**Components index** (`src/pages/ComponentsIndex.tsx`)
- Every component is a live mini render, not a name or a shot. The renders mount near the viewport, one per idle slice, inside a fixed box so the layout never shifts.
- Groups in the requested order: Layout, Text, Media, Navigation, Overlays, Feedback, Data, Actions, Inputs, Lists.
- Search matches the start of words in name, sentence, group, A2UI basic names and prop names, so `date` finds DatePicker but not `validateOn`. `/` focuses the search box.
- Group filter chips; `?q=` and `?group=` make a filtered view linkable; a status line shows the match count.
- Four platform dots per card: filled means a native shot exists, hollow means pending.
- The whole card is the link; the render is inert, so no controls sit inside the link.
- Phones get two cards per row.

**Component page** (`Component.tsx`, `ComponentStudio.tsx`, `VariantGallery.tsx`, `embed.ts`)
- **Head:** one sentence (at most 160 chars), "Built from" chips for macros (from `macros.json`), A2UI basic alias, children.
- **Big live preview** with switches for theme (3), mode, LTR/RTL and width (Fit, 390, 768, 1280). A preset wider than the column is scaled to fit and outlined.
- **Tabs:** Preview (prop switcher), A2UI JSON, React, SwiftUI, Compose, gpui. ARIA tabs with arrow, Home and End keys.
- **Embed code** feeds the current surface through each SDK's `MemoryTransport` into its host. Checked against the real APIs: `MemoryTransport`, `HostOptions`, `HostSurface`, `builtin_theme`, `Mode`. The `$`, backtick and raw-string escapes are handled for each language and tested.
- **Platform strip:** the four shots at equal height (column widths follow each shot's aspect ratio). On phones it becomes a sideways-scrolling strip. A missing shot shows a deterministic "Shot pending" box.
- **Variants gallery:** each option of every short enum prop plus the boolean states, as small live renders.
- **Props table:** name, type, default, first sentence of the description; badges for bindable and per-breakpoint props. Phones get one block per prop.
- **Other sections:** events and slots table; accessibility (role, semantics, keys table with `<kbd>` from `a11y.json`); theming (recipe parts, what they are keyed by); app parity with long notes folded under "More"; Related chips.
- **Sidebar and order:** the sidebar is grouped by component group (optional `group` field on `DocsNavEntry` in site-shell), and prev/next follow the same order.

**Shot pending, never a broken image:** `scripts/shot-index.ts` writes a gitignored `src/generated/shot-index.ts`. It runs in `gen`, which dev, build, typecheck and test now call first. 22 components currently have no shots and show the pending state.

**Light/dark:** `src/styles/scheme.css` overrides the tokens under `html[data-scheme=light]`. A tiny inline script in `index.html` sets the scheme before first paint (stored choice, else the OS setting). The CSP in `public/serve.json` allows that exact script by hash. A sun/moon toggle sits in the top bar, and live renders and the theme gallery follow the scheme.

**Responsive and accessible:**
- A phone menu (`<details>`, no JS); before this, the top nav simply disappeared at ≤720px.
- Section tags in `DocsSection` now have `role=heading`.
- Focus rings and contrast fixes for the light scheme.

**Copy pass and stale facts:**
- Home, Guides, Themes, Playground, builder intro, Conformance and Concepts are shorter.
- Removed the "Coming with round 1" and "Arrives with round 1" callouts; the real-font harness is now described as it exists, in a table.
- Fixed Concepts calling Table a macro (it is native).
- Native count 45 → 46 and function count 29 → 31 (failing tests before this work).
- Added `blur`, `breakpoint`, `ease` and `density` labels on the Themes page.

**Other fixes:**
- **Playground** now prerenders in full, so the editor is in the HTML. This fixed a hydration error from `toLocaleString()` without a locale.
- **Theme builder:** the site's builder overrides lost to `builder.css`, which loads later, so phones showed two squeezed columns and the sticky offsets were wrong. Fixed, and the toolbar is pre-styled so mounting no longer shifts it (CLS 0.289 → 0.048).
- **Specimen lookup:** Stack's card was rendering its specimen caption; now it renders the subject.
- **Small layout shift** on component pages (0.017) is fixed.

**Tests:**
- `src/lib/docs-coverage.test.ts`: every non-hidden catalog component has a doc, a page and a specimen whose subject is that component; summary ≤160 chars; no lorem ipsum in docs, examples or specimens; macro parts and related links name real components; variants change exactly one prop; embed escaping; index search behaviour.
- `src/lib/scheme.test.ts`: the CSP hash matches the inline script, and the script uses the same storage key as `scheme.ts`.

**Screenshots:** `bun --filter @exp/ui-site screenshots` (after a build) serves `dist/` and writes to `/tmp/ui-site-shots/`:
`home.png`, `home-light.png`, `components.png` (full page), `components-light.png`, `components-filter-inputs.png`, `components-search.png`, `components-phone.png`, `component-button.png` (full), `component-button-light.png`, `component-button-phone.png` (full), `component-button-rtl-768.png`, `component-button-react-tab.png`, `component-table-1280.png`, `component-resizable.png` (full), `component-dialog.png` (full), `component-date-range-picker.png` (full), `concepts.png`, `themes.png`, `theme-builder.png`, `guides.png`, `guide-react.png`, `playground.png`, `conformance.png`.

The README is updated.

## Contract notes

- **Sidebar sentence is 205 chars**, over the 160 limit. The site cuts it at the `;` (`componentSummary`). The catalog should shorten it at the source (contract lane).
- **Resizable:** the React renderer still paints "Unknown component Resizable" on its page and index card. It goes away once the React lane adds the painter; no site change needed.
- **Builder accessibility problems found by Lighthouse, not in my paths:**
  - React renderer: `span.xui-trigger` carries `type="button"`; Ring `role=progressbar` has no name; Meter is missing a required ARIA attribute; RowList `role=list` lacks list children.
  - Builder: its recipe-sheet `<select>` has no label, its sheet headings jump straight to `h3`, and a Button label in the preview's custom theme fails contrast.
- **`hidden` components** (the `Unknown` placeholder) get no page; the coverage test skips them on purpose.

## Deferred

- **Theme builder Lighthouse** is mobile 69 (performance) / 74 (accessibility), desktop 83 / 74. Blocking time is 1.7–3s because the builder package renders its whole recipe sheet and kitchen sink in one go, plus the accessibility items above. Smallest next step: in `packages/exponential-ui/builder/mount.ts`, render the sheet sections lazily, one per idle callback, and fix the listed attributes in the React renderer.
- **Best-practices 96** on every page comes from `api.github.com` answering 403 after my repeated Lighthouse runs (rate limit), which logs a console error. It is environmental, not a site defect; caching the star count in `use-github-stars` would avoid it.
- **Playwright** is used by `scripts/screenshots.ts` but not declared in `apps/ui-site/package.json`; it resolves from the root `node_modules`. I left `bun.lock` alone because other lanes are editing it; declaring it later is a one-line devDependency.
- **Shots:** this site has no captures of its own. The 22 components without stored shots stay "Shot pending" until the shot and Mac runs add them; the next build picks them up automatically.
- **Guides:** `check:guides` was not run; no guide sources changed.

## Tests run

- `cd apps/ui-site && LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 bun run test` → 28 pass, 0 fail.
- `bun run typecheck` (ui-site) → 0 errors. `packages/site-shell`: `bun run typecheck` → clean (it has no test files).
- `bun run build` (ui-site) → prerender complete, 99 URLs. One intermediate build failed on the React lane's mid-edit `packages/exponential-ui-react/src/list.tsx`; later builds were green.
- `apps/marketing`: `bun run build` → prerender complete, 18 pages; `bun run typecheck` → clean; no tracked files changed.
- `bun run scripts/screenshots.ts` → 23 screenshots, no page errors logged.
- **Lighthouse 13** on the built `dist/`, served locally with gzip (as `npx serve` does in production), mobile preset unless noted:

| Page | Performance | Accessibility | SEO |
|---|---|---|---|
| `/` | 97 (desktop 100) | 100 | 100 |
| `/components/` | 96 (desktop 100) | 100 | 100 |
| `/components/button/` | 95 (desktop 100) | 100 | 100 |
| `/components/date-range-picker/` | 96 | 100 | 100 |
| `/concepts/` | 98 | 100 | 100 |
| `/themes/` | 95 (desktop 100) | 100 | 100 |
| `/guides/` | 99 | 100 | 100 |
| `/guides/react/` | 98 | 100 | 100 |
| `/playground/` | 95 | 100 | 100 |
| `/conformance/` | 99 | 100 | 100 |
| `/themes/builder/` | 69 | 74 | 100 |

  Best-practices is 96 on every page because of the GitHub 403 above.
- `wc -m CLAUDE.md` → 39,986; unchanged.


## impl:core (Implement)

## Done

**Catalog and theme**
- The new generated catalog loads. Counts updated: 83 components, 31 functions.
- The theme now has the `blur` token group (`ThemeTokens::blur`, `NUMBER_GROUPS`), plus two new recipe keys: `backdropBlur` (token only) and `animation` (enum).
- `direction` is valid on any node, which needed no code change because the style check reads the spec. Tests cover `sticky`, `backdropBlur` and `animationDuration`.

**New core modules**

| module | what |
|---|---|
| `src/format.rs` | `Formatter` trait, `EnglishFormatter` (built from `englishFallback`), `display_string`, `parse_date_value`, `format_pattern`, `relative_time_unit`, `call_format_function` |
| `src/resizable.rs` | `normalize_sizes`, `resize_panels`, `keyboard_resize`, `panel_extents`, `drag_delta`, `PanelLimits`, `Orientation` |
| `src/direction.rs` | `node_directions`, `physical_text_align`, `text_align_of`, `node_text_align` |
| `src/animation.rs` | `animation_timing`, `animation_timing_ms`, `animation_frame`, `frame_with_timing`, `cubic_bezier`, `keyframes_css` |
| `src/layout.rs` | the `layout.json` constants |
| `src/list.rs` (extended) | `template_item_keys`, `table_row_keys`, `template_instances`, `item_offsets`, `virtual_window`, `scroll_offset_for_index`, `list_sections`, `section_rows`, `sticky_header` |

**Reducer and bind pass**
- Templates are lifted out of the tree into `ReduceResult.templates` on both the nested and the flat path, including nested templates and the missing-id issue. The surface builds items from this table.
- Instance suffixes now accumulate through nested templates (`issue.title.ops.1`). Item keys use `template_item_keys`. Table rows use `table_row_keys`.
- Layout props resolve through `resolve_node_props`, so literal Table rows holding `{path}` stay data.
- A text prop bound to a number or boolean shows its `display_string`. `reconcile.rs` uses `display_opt`.
- `json::number_to_string` is now the full ECMAScript algorithm. The old version was wrong past 2^63.
- `ResolveContext` gained `formatter` and `now`. The six format functions run through it.

**Surface**

*Formatting*
- `set_formatter`, `set_clock` and `tick` are new, and `SurfaceSettings.time_zone` was added.
- These go through the surface formatter:
  - Table `number`, `currency`, `percent`, `date` and `relativeTime` cells (using `column.currency` and `column.decimals`)
  - NumberField text
  - chart `tickLabels`
  - DatePicker and DateRangePicker trigger text
  - calendar month titles and weekday names
- `$string` labels are added to node `accessibility` when the author set none:
  - `invalidValue`
  - `dialog`
  - `codeBlock`
  - `table`
  - `carousel` / `slide`
  - `resize`

*Visual and layout*
- `Visual` gained `direction` and a physical `text_align` (on leaves), plus `backdrop_blur`, `animation` and `sticky`.
- `position: sticky` is laid out as relative, ignoring its insets. Its pinned offsets come out in `LayoutOutput.sticky` (`StickyOutput {index, dx, dy}`).

*Resizable*
- Builds `panel`, `handle` and `grip` parts; the handles carry separator accessibility.
- `event(handle, "drag", {phase: start|move|end, delta})` always computes from the sizes at the drag start.
- `event(handle, "key", {key})` handles the arrow keys, Home, End and Enter.
- A bound `sizes` is written back and `change {sizes}` fires.

*Lists*
- Windowing works on one axis, so horizontal lists window on x (`ListOutput.horizontal`). The windowing threshold is now 50.
- A divided list puts the hairline in the middle of the gap.
- `sectionBy` with the `section` slot and `stickyHeaders` is implemented. Each header is a part `<list>.section.<i>` with heading level 3.
- A list without a bounded size windows against its nearest scrolling ancestor or the host viewport. The new `Surface::set_surface_scroll` feeds the host's offset.
- `scroll_to_index` and `SurfaceCommand::ScrollToIndex` are new. When the page scrolls the list, the surface emits the new `OutEvent::ScrollSurface {x, y}`.
- Windowed items carry `posInSet` and `setSize` in their accessibility.
- Item keys are cached per data version, so a scroll step never re-keys 100,000 rows.

**FFI**
- New foreign trait `HostFormatter` and `Surface.setFormatter`.
- New surface methods: `setClock`, `tick`, `setSurfaceScroll`, `scrollToIndex`.
- `FfiSettings.timeZone` added.
- `FfiVisual` gained `direction`, `backdropBlur`, `animationJson` and `sticky`.
- `FfiLayout.sticky` and `FfiList.horizontal` added. All new record fields have defaults.
- New functions:
  - `bindRowSlotJson`, `bindSectionHeaderJson`
  - `formatCallJson`, `displayStringJson`, `parseDateValueJson`, `formatPatternJson`, `relativeTimeUnitJson`
  - `animationFrameJson`, `resizableJson`, `listJson`
- Bindings regenerated (Swift and Kotlin under `bindings/`). Added Rust facade and Kotlin round-2 checks.

**Conformance and bench**
- The Rust runner now runs all 24 suites. The shared round-2 cases are in `tests/support/round2.rs`.
- New tests: `tests/round2_fixtures.rs` and `tests/round2_surface.rs`.
- The 100k bench was added to `examples/bench.rs`, with numbers in the README. Both READMEs have a round-2 table.

## Contract notes (for the gpui, Compose and Mac lanes)

- **Templates**: read `ReduceResult.templates`. The surface already does. Items wear the accumulated suffix.
- **Formatter over FFI**: the host implements `HostFormatter` only for localizing. The core passes a parsed instant and `dateOnly`, the relative value and unit already chosen, and the plural is decided from its category. It is called while the surface is locked, so the host must not call back into the surface from it.
- **Text**: painters shape leaf text with `Visual.direction` as the bidi paragraph direction. `Visual.text_align` is now always set on leaves (left, right, center or justify).
- **Sticky**: paint `LayoutOutput.sticky` nodes and their subtrees translated by `(dx, dy)`, inside their scroller's translation. A pinned header that is outside the window is rendered with `position: absolute`.
- **Resizable**: panels split the space after the handles exactly as `panelExtents` does. Handles are `$control.hairline` thick in layout; painters add the 8 px `resizeHandleHit` area. A Resizable defaults to `width: 100%`.
- **Unbounded lists**: a windowed list gets `overflow: scroll` only when bounded (it has `height`/`maxHeight`, `width`/`maxWidth` for horizontal, or `flexGrow`).
  - Hosts that scroll the whole surface should call `set_surface_scroll`.
  - Calling `scroll(list)` with the page offset still works as long as `set_surface_scroll` was never called.
- **Deviation from the contract**: an unmeasured item takes the *mean measured* extent once any item has been measured; `$control.row` is used only before that. This keeps the scroll extent and clamping stable. I suggest the contract adopt this rule.
- **Visible changes**:
  - Calendar weekday names come from the formatter's `EEE`, so they read "Sun", not "Su".
  - Chart props gained `tickLabels`.
  - `ListOutput`, `LayoutOutput.sticky` and the new `OutEvent`/`SurfaceCommand` variants are additive.
- **serde_json `float_roundtrip`**: now enabled on `exponential-ui`. JS `JSON.parse` precision requires it for display strings like `123456789012345680000`. It applies workspace-wide; `Cargo.lock` is unchanged.
- **Kept for gpui until its lane switches**: `fields::dates::{MONTHS, WEEKDAYS_SHORT, label}`.

## Deferred

- **gpui lane**:
  - The `suites` runner needs the 9 new manifest suites; the case lists in `exponential-ui/tests/support/round2.rs` can be reused.
  - The ratchet `gpui_matches_the_web_baseline…` fails (`position 261 > 253`, `onlyRef 4 > 1`), as the contract lane predicted, until the web baseline is rewritten.
- **Swift binding suite**: no round-2 checks added, because it cannot run here. Next step: mirror the Kotlin round-2 block in `tests/swift/main.swift` on the Mac.
- **`cargo publish --dry-run -p exponential-ui-ffi`** fails because `exponential-ui` is not on crates.io yet; this was already true before this round. The core crate's dry run is clean.
- **TimePicker trigger text** does not go through the formatter. Next step: add a time-only `DateOptions` style.
- **rtl horizontal lists** window from the left edge, not the inline start. Next step: flip the axis offset in `list_view` and `list_scroll` for rtl.
- **Scroll step at 100k rows** is about 3.2 ms because offsets are recomputed in O(n) each time new extents are measured. Next step: a Fenwick tree in `ListWindow`.

## Tests run

- `CARGO_TARGET_DIR=/tmp/xui-r2-target cargo test -p exponential-ui -p exponential-ui-ffi --no-fail-fast`: 238 passed, 0 failed. The facade suite is 20/20.
- `cargo clippy -p exponential-ui -p exponential-ui-ffi --all-targets -- -D warnings`: clean.
- `bun run --filter @exponential-at/ui conformance:check .conformance/exponential-ui.json`: **CONFORMANT**, 1609 cases in 24 suites.
- `JNA_JAR=/usr/share/java/jna-5.14.0.jar bash …/run-binding-tests.sh kotlin`: 5358 checks, 0 failures. Bindings were regenerated first.
- I regenerated the bindings with the steps of `generate-bindings.sh` run by hand, skipping its copy into `packages/exponential-ui-swift`, which another lane owns.
- `RUSTUP_TOOLCHAIN=1.96.0 cargo check -p exponential-ui-gpui --all-targets` and `cargo check -p ui`: compile. `cargo test -p exponential-ui-gpui`: only the 2 suites listed under Deferred fail.
- `cargo publish --dry-run -p exponential-ui`: OK.
- `cargo run --release -p exponential-ui --example bench`:
  - 205-node grid: 0.14 ms
  - 2,000-row list: 1.16 ms per scroll step, 0.03 ms per hover
  - 100k rows: firstPaintMs 74.6, scrollStepMs 3.19, scrollToIndexMs 5.39, renderedItems 30

Key paths:
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/apps/desktop/crates/exponential-ui/src/{format,resizable,direction,animation,layout,list,reducer,data}.rs`
- `…/exponential-ui/src/layout_tree/containers.rs`
- `…/exponential-ui/src/surface/{pass,events,restyle,mod}.rs`
- `…/exponential-ui/tests/{round2_fixtures,round2_surface}.rs`, `…/tests/support/round2.rs`
- `…/exponential-ui-ffi/src/lib.rs`, `…/exponential-ui-ffi/bindings/`


## impl:react (Implement)

**React lane (round 2): done. Every jsdom, Chromium and conformance suite passes, and I rewrote the web conformance baseline at the end.** The gpui ratchet has not been re-run against the new baseline yet; that is for the gpui lane.

## Done
All paths below are under `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-react/`.

- **Templates (§4):**
  - `useSurface` now exposes the reducer's lifted `templates`, and `<ExponentialSurface>` takes a new `templates` prop. The old in-tree lookup and `templateNodeFrom` are removed.
  - Item keys come from the core's `templateItemKeys`. Table row keys come from `tableRowKeys` (`#<index>` rule).
  - `HostSurface` falls back to re-reducing the store's components, because `SurfaceStore` has no `templates` (see Contract notes).
- **Resizable (§1):** new `src/natives/resizable.tsx`.
  - Panels use flex-grow factors over a 0 basis, so widths equal `panelExtents`.
  - Handles are a hairline thick with an 8 px hit area and an optional grip.
  - Pointer drags start from the sizes at drag start; arrow keys use `keyboardResize`.
  - Bound `sizes` are written, and `change {sizes}` fires at drag end or on a key.
  - Each handle is a `separator` named by `$string.resize`.
- **Style (§2):**
  - `backdropBlur` maps to `backdrop-filter`; the theme sheet now defines `--xui-blur-*`.
  - `animation` and `animationDuration` become CSS longhands timed by the motion tokens, with the factor kept. The 8 keyframe sets are in the base sheet (they replace the old spin/pulse/fade-in).
  - Shimmer is an `::after` band driven by a registered `--xui-band`. Reduced motion shows the rest frame.
  - `position: sticky` is plain CSS.
  - Any node can set `direction`: it gets a `dir` attribute, and its natives read the new direction. Glyphs in `rtlMirroredIcons` flip via `:dir(rtl)`.
- **Bug fix:** a recipe with only per-side border widths was getting the browser's default 3 px on the other sides. That was the extra 9 px on Table.
- **Formatter (§3):**
  - One `intlFormatter(locale, timeZone)` per surface; `timeZone` is a new prop.
  - It drives the six format functions, the new Table cell types (`currency`, `percent`, `relativeTime`, with `column.currency` and `column.decimals`), NumberField, the Slider value, chart ticks and the `$string.chartSummary` label, picker triggers, and calendar month/weekday names.
  - A `formatRelativeTime` call without `now` re-binds once a minute.
  - Text and Markdown show non-string values via `displayString`.
- **Strings:** `invalidValue`, `message`, `codeBlock`, `dialog`, `table`, `carousel`, `slide` and `resize` replace the hard-coded labels.
- **Lists (§5):** `src/list.tsx` rewritten.
  - Windowing works on one axis, so horizontal lists window too. Gaps and dividers count in the offsets.
  - `sectionBy` and the `section` slot are bound to `{value, count, index}`. Headers are level-3 headings, and `stickyHeaders` pins them.
  - Unbounded lists window against the nearest scroller or the viewport.
  - `scrollToIndex` is a host command on List and Table; it re-aims once revealed rows are measured.
  - Every item has `aria-setsize` and `aria-posinset` for the whole list.
- **Parts (§6):**
  - Every laid-out part carries `data-xui-id="<id>.<part>[.<i|key>]"` (`useParts`, `part.at`).
  - Gpui-only ids per kitchen-sink case went from 110 to 3. The 3 left are the old in-place template nodes, which gpui still has to drop.
  - Inactive carousel pages are `inert` and carry `data-xui-inactive`.
- **§7 boxes:**
  - Overlays take their trigger's box (`data-xui-overlay-root`), and the menu trigger has no chevron.
  - Chart `height` is the whole box. AudioPlayer is a track plus a controls row.
  - Video and Image default to 16:9 and get an intrinsic width of 320.
  - Image fallback fills the box with an `iconLg` glyph.
  - CodeBlock and Table sizes come from recipes only. FileUpload shows a Browse button.
  - TreeGuides use 16 px columns. Switch puts the label first. Checkbox has a gap. Radio uses the root gap. Accordion count is its own part.
- **Tests and harness:**
  - New `src/round2.test.tsx` (38 tests; it replays `resizable.json` keys through the painter) and `browser/round2.test.ts` (6).
  - `browser/suites.test.ts` now covers all 24 suites of manifest v2.
  - New harness views `round2` and `bench`.
- **Bench (100,000 rows):** `scripts/bench-list.ts` (`bun run bench:list`) and `src/bench.test.tsx` (`XUI_BENCH=1`).
  - Chromium: first paint 147 ms, scroll step 4.9 ms, jump to row 50,000 38 ms, 30 items alive.
  - jsdom: 549 / 14.9 / 70 ms, 30 items.
  - Numbers are in the README (i9-13900K). Getting there needed memoised extents and template keys: a scroll step was 17 ms before.
- **README:** Lists, Round 2, bench and commands sections updated.
- **Web baseline rewritten** (`packages/exponential-ui/fixtures/conformance-baseline.json`, 48 cases). Intended changes: lifted template, the Resizable section, layout-box dump, Nunito/Fira Code, the part ids, and the §7 fixes above.

## Contract notes
- `packages/exponential-ui/builder/preview.tsx` still passes only `root`, so the builder's kitchen-sink list now renders 0 items. It needs `templates={…}`.
- `SurfaceStore` (`src/host/runtime.ts`) should expose `get templates()`; React re-reduces the components until it does.
- `conformance-known.json` was not touched. The gpui ratchet needs re-running against the new baseline, keeping `causes` and `rules`.
- Some differences have no decision in the contract:
  - CodeBlock `lineNumber`: web applies the `CodeBlock/gutter` padding; gpui ignores it (16 px).
  - Carousel indicator: the web has a dot row plus prev/next buttons (carousel 218 px tall); gpui has one 8 px dot (188 px).
  - Table column widths: web aligns columns; gpui sizes each row separately.
  - A Link trigger stretches to its wrapped line's height (78 px at 600).
- The Formatter has no time-of-day format, so TimePicker text stays on Intl in the surface locale. Calendar day aria-labels also stay on Intl, because `intlFormatter` builds a new Intl object on every call; caching inside `intlFormatter` would help.
- The `List/divider` recipe only sets `height`, so horizontal lists cannot get a vertical divider from it.

## Deferred
- Web captures for the 22 specimen ids and the view-catalog entries: outside my paths (shot lane). All 83 specimens paint in the harness with no `Unknown` and no page errors.
- Licence row for the conformance fonts: `docs/third-party-licences.md` is outside my paths.

## Tests run
- `LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 npx vitest run` (react): 1196 passed, 1 skipped (the bench), after `-u` updated 209 DOM snapshots for the new part ids, list wrappers and §7 markup. The calendar test that walks about 900 months now has a 30 s timeout; it also hit the 5 s limit on the unmodified code.
- `npx vitest run --config vitest.browser.config.ts`: 8 files, 130 passed, 1 skipped. This includes conformance against the new baseline and `suites.test.ts`, which reports conformant on manifest v2.
- `XUI_BENCH=1 npx vitest run src/bench.test.tsx` and `bun scripts/bench-list.ts`, 3 runs each: numbers above.
- `npx tsc --noEmit` (react): clean. `npx tsc --noEmit` in `apps/web`: clean.
- `bun run build:css`: 74.7 KB. `packages/ui` vitest: 475 passed.
- `bun test ./src ./conformance` (`packages/exponential-ui`): 320 passed.
- `apps/ui-site` `bun run test`: 28 passed.
- `bun run --filter @exponential-at/ui conformance -- --write-baseline --skip-desktop`: 48 cases written; the re-run matches with 0 divergences.


## impl:gpui (Implement)

## Done

The gpui renderer now matches the web baseline the React lane rewrote at 22:58. The 24 responsive cases have zero divergences. Each of the 24 kitchen-sink cases has one origin left, the Carousel. Its 28 px web-only controls row also accounts for the rest of each kitchen-sink case's count: 4 size and 183–193 position divergences.

**gpui crate (`apps/desktop/crates/exponential-ui-gpui`)**
- **Resizable**:
  - Handles are 1 px hairlines with an 8 px hit area (`resizeHandleHit`) and a resize cursor.
  - A drag sends `drag {start|move|end, delta}` measured from the pointer's start position.
  - Arrows, Home, End and Enter send `key {ArrowLeft…}`.
  - Each handle is a `Splitter` a11y node with value, min/max and orientation.
- **Sticky**: nodes paint at `LayoutOutput.sticky` offsets and on top of their siblings.
- **Animations**: frames come from the core's `frame_with_timing`, starting when the node enters the tree, and the view requests frames while one runs.
  - Applied: opacity, translate, a leaf's scale, an Icon's rotation, and the shimmer band.
  - Reduced motion paints the rest frame.
- **backdropBlur**: paints the translucent background alone; the README documents this.
- **Formatter**:
  - New: `SurfaceViewOptions::formatter`, `HostOptions::formatter`, `set_formatter`, `set_clock`. The view re-binds once a minute so relative times move.
  - Removed the English `MONTH_NAMES`/`WEEKDAYS` and the `date_label`/`select_label` helpers.
  - Table/FileUpload stand-ins now use `$string.table`/`dropFiles`; Unknown uses `unknownComponent`.
- **Direction**:
  - Leaves use their own `Visual.direction`.
  - New `text::with_paragraph_direction` adds an RLM/LRM mark when the first strong character disagrees with the node, so `Hello!` reads `!Hello` in an rtl box.
- **Lists**:
  - The host scroll now goes through `set_surface_scroll`.
  - Horizontal windowing, `section` headers (a11y headings) and sticky headers work.
  - New `command(...)` and `scroll_to_index(...)` on the view; `HostPlugin::scroll_surface` is called when the host's scroller has to move.
- **§7 measures**:
  - Picker triggers have no 160 px floor.
  - Media: max-content 320 px, min-content 0, height from the aspect ratio.
  - An empty text lays out 0 lines; markdown widths are measured with their bold/code runs (fixes `main-md`).
  - Text-like props show via `display_string`.
  - Others: ToggleGroup item gap, Accordion count after the title, AudioPlayer `$control.row`, Carousel dots row, CodeBlock line-number digits, Tab count without a pill.
- **Paint fixes**:
  - Single-line inputs have no gpui-component padding of their own.
  - Image fallback per contract: `Image/fallback` fill with the `iconLg` glyph.
  - Icon glyphs use their `size` token (the FileUpload remove X was 36 px).
  - A one-line text gets 1 px of trailing slack (no false ellipsis on "Online", "Archived", "88 posts").
  - Table cells centre their text; expanded natives no longer paint a stand-in "Table" label.
- **Runtime**: the runtime's plugin adapter now forwards `announce`, `pick_files`, `on_upload` and `scroll_surface` (the first three were silently dropped before).
- **Conformance runner**:
  - `tests/suites.rs` now runs all 24 suites through the core's shared `round2.rs` cases.
  - The ratchet writer keeps `causes`/`rules` and writes a new `survivors` key.
  - The ratchet now fails on any origin that has no entry in `SURVIVORS`.
  - The dump counts the lines a Text actually paints (`painted_lines`).
  - Budgets rewritten.
- **New tests and bench**:
  - `tests/round2.rs`: 9 tests.
  - `examples/bench_list.rs`: the 100k-row bench, headless.
  - README updated.
- **IDE screen** (`exponential_ui_screen.rs`): `scroll_surface` scrolls the pane; the windowing threshold comment now says 50.

**Core changes I made (listed as the rules require; locked by the new core test file `exponential-ui/tests/round2_parity.rs`, 7 tests)**
1. `restyle`: a `lines: 1` leaf clips on x in taffy only. This works around a taffy 0.12 bug that made a "3" Badge twice its padding wide.
2. `engine`: an aspect-ratio leaf with a known width gets height = width / ratio. `media_ratio` defaults Image/Video to 16:9.
3. Builder defaults (`style_default`, tracked in a new `LNode.default_keys`) now sit **under** the native's root recipe, and the author's style still wins. This fixes the Form and Radio gaps.
4. Type inherits like CSS: size, weight, family, style, case, tracking, and the px line height. Roots inherit `$type.size.sm` / `$type.lineHeight.sm`, like the web's `.xui-surface`.
5. Smaller fixes:
   - A sticky direct child of the scroller is bounded by the scroll content, so it stays pinned all the way.
   - Table text-cell flex basis floors at its padding.
   - TreeGuides: width = depth × 16, stretched to its row.
   - Overlays with a non-control trigger fill; Link triggers stretch.
   - Slider track margins centre the rail in a thumb-tall row.
   - Chart `minWidth: 0`; ToggleGroup `alignSelf: flex-start`.
   - NumberField gap `xxs`; CodeBlock gutter margins plus a `digits` prop; code lines stretch.
   - File sizes use the formatter (`47.1 kB`).
   - Carousel dots row without the per-dot recipe; FileUpload title medium weight.
   - `Iterations::count` is now public.

## Contract notes
- **CardRow cause was misattributed**: the 30 px came from TreeGuides (web 2 px wide). It is gone with the React fix.
- **Compose and SwiftUI must re-run** against items 1–4 above. They change shared layout: type inheritance, recipe precedence, one-line clipping, aspect ratio.
- **`digits` on line numbers**: Compose and Swift should size every line number like the widest one. The web's `(digits+1)ch` minimum never takes effect, so its code column shifts at line 10.
- **ToggleGroup icon–label gap**: it now comes from the `ToggleGroup/item` recipe, which defines none, so it is 0 like the web.
- **File-size unit names** are English until there are per-locale string tables.

## Deferred
- **The one survivor, `carousel`**: the web paints a controls row (28 px previous/next buttons, CSS literals) that has no parts in the contract. Next step: add `Carousel/controls|previous|next` to `recipes.json` and `layout.json`, then both renderers lay them out.
- **Parts painted but not laid out as nodes** (6–8 web-only nodes per case): `accordion.count`, `Image/fallback`, `Ring/label`, `Video/controls`, `AudioPlayer/track|controls`. Next step: the core builds them as part nodes.
- **`panels.grip.0`** shows up only in gpui. Next step: React marks the grip.
- **Textarea** keeps gpui-component's editor padding; that API has no override.
- **Animations**: containers can translate and fade but not scale or rotate (gpui limit); `spin` turns Icons only.
- **backdropBlur**: background only.
- **Unknown label** uses the default string, not host overrides; the measurer has no strings table.

## Tests run
- `cargo test -p exponential-ui -p exponential-ui-gpui --no-fail-fast` (toolchain 1.96.0, `/tmp/xui-r2-target`): all pass. That covers the gpui lib (53), render (9), interaction (29), host (6), round2 (9), suites (1), conformance (2), and every core suite including the new `round2_parity` (7).
- `cargo clippy -p exponential-ui -p exponential-ui-gpui --all-targets -D warnings`: clean.
- `cargo test -p exponential-ui-ffi`: facade 20/20. Kotlin binding suite: 5358 checks, 0 failures.
- `cargo check -p ui`: compiles (the warnings are in other crates and were there before).
- `conformance:check .conformance/exponential-ui-gpui.json`: CONFORMANT, 1609 cases in 24 suites.
- `EXP_UI_WRITE_FIXTURES=1 cargo test --test conformance` wrote the budgets; a plain rerun passes.
- `cargo run --release --example bench_list` (i9-13900K): firstPaintMs 87.2, scrollStepMs 6.52, scrollToIndexMs 9.25, renderedItems 30.
- Looked at the kitchen sink in an Xvfb window (`--window`), ltr and rtl. That caught the remove-icon, ellipsis, table-label and cell-alignment problems above, all fixed.
- `CLAUDE.md` is unchanged at 39,986 characters.

Key paths:
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/apps/desktop/crates/exponential-ui-gpui/`
- `…/apps/desktop/crates/ui/src/exponential_ui_screen.rs`
- `…/apps/desktop/crates/exponential-ui/src/{surface/restyle.rs,surface/pass.rs,engine.rs,layout_tree/*.rs,animation.rs}`
- `…/apps/desktop/crates/exponential-ui/tests/round2_parity.rs`
- `…/packages/exponential-ui/fixtures/conformance-known.json`


## impl:compose (Implement)

## Done

The Compose painter now implements round 1 and round 2. Most of the work rides on the core: templates, keys, sections, layers, windowing and binding all come from the facade.

**Settings and formatting**
- `SurfaceOptions.settings` / `SurfaceSettings` covers locale, time zone, string overrides, system mode, density, contrast, font scale, reduced motion, hover, safe-area insets, `today`, `hoverCloseMs` and the formatter.
- `ExponentialSurface` reads what the device says: locale, zone, dark mode, font scale, animator scale, mouse, safe-drawing insets and high-contrast.
- `IcuFormatter` implements `HostFormatter` over `android.icu`. It falls back to the core's English formatter where ICU is unavailable, e.g. plain-JVM tests.
- The hard-coded labels now come from built-in strings (`loading`, `message`, `pageOf`, `search`, `confirm`/`cancel`, `unknownComponent`).

**Painting**
- `PaintStyle` reads every visual key: gradients, per-side and dashed borders, per-corner radii, transform, visibility, pointer events, transitions, letter spacing, decoration, case, italic, direction, `backdropBlur`, `animation` and `sticky`.
- Text leaves are dispatched by their owner component: tabs (with the web's count spacing), Accordion (the count as its own muted text), menu item labels, Select options with check, Table header cells with sort arrow, `boolean`/`badge` cells and CodeBlock token lines.
- New Chart painter for every kind (bar, stackedBar, line, area, pie, donut, sparkline), using the core's ticks, formatted labels, palette and donut hole, with a legend and a tooltip.
- Directional glyphs mirror under RTL.

**Measurer**
- Per-side padding and baselines.
- Text chrome per owner component.
- Round 2 §7 sizes: no 160 floor on picker triggers, 160 field width, media aspect ratio, Chart `height` as the whole box, AudioPlayer as title + control row, empty text as 0 lines.
- Measures the live text of inline fields, autosize Textareas and the Composer.
- Widths are ceiled to a whole device pixel.

**Behaviour**
- Scroll containers come from the core's `scrolls`, on both axes, and offsets sync both ways (`scrollTo`).
- Sticky pins, unbounded lists windowed against the host's scroll, `scrollToIndex`, and commands (`model.command`).
- The core's events now work: focus, announce, copy, file picking, hover timers and `scrollSurface`.
- Toast timers (a press or hover pauses them). Viewport-placed painted layers follow the host scroll.
- Resizable handles drag from the start sizes and take keys.
- Keyframe animations play from the core's frames; reduced motion shows the rest frame. Transitions animate colour and opacity.
- Enter submits single-line fields; leaving a field fires `blur`.
- Long press opens a ContextMenu.

**Keyboard** (`Keyboard.kt`)
- Tab walks controls in pre-order and sets `:focus-visible`; buttons without a handler are focusable too.
- Arrows rove and activate per `a11y.json`; also slider, carousel, calendar, NumberField and ChipInput keys, Shift+F10, Escape.
- Tab leaves multi-line fields.

**TalkBack** reads each node's `accessibility`: roles, heading, pressed/checked/selected/current, expand/collapse, progress values, live regions, set positions, field errors and dialog pane titles. Pressable containers now expose a click action.

**Conformance**
- All 9 new manifest-v2 suites run. `format` goes through the ICU formatter itself; animation timings and leaf direction are read from a live surface. Result: **CONFORMANT, 1609 cases in 24 suites**.
- New real-font dump (`RealFontConformanceTest`): every case of `conformance-cases.json` is laid out with only the conformance fonts (Robolectric native text, density 3) and written in the `xui-frame-dump/1` format to `build/conformance/compose.json`.
  - It is compared with the web baseline by a Kotlin port of `compare.ts`.
  - The ratchet budget is `ui-compose/src/test/conformance-known.json`; all origins go to `build/conformance/report.txt`.
  - Line counts come from the painter's own text layout, so a stretched box doesn't count as extra lines.
- Compose-side divergences I fixed: tab chrome spacing, the Accordion count, device-pixel widths. The rest are core-side; see Contract notes.

**Snapshots, shots, catalog, docs**
- Component JSON and Roborazzi images re-recorded on purpose.
- Added 22 `views.json` entries for the new specimen ids.
- Captured 84 android shots on a new `Medium_Phone_API_34` AVD (1080×2400): 22 new, 62 updated. The shots pipeline also rewrote `shots/index.json`, which is derived.
- The example host maps the built-in glyphs; `MainActivity.live` exposes the model to tests.
- README rewritten: settings, painting, measurement, controls, overlays, lists, keyboard, accessibility, tests, numbers, divergences.

Key paths, under `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/`:
- `ui-compose/src/main/kotlin/at/exponential/ui/{format/IcuFormatter.kt,catalog/CatalogConstants.kt,compose/{Keyboard,ChartLeaf,NodeView,Containers,Semantics,PaintedBox,TextLeaves}.kt,model/SurfaceModel.kt,measure/SurfaceMeasurer.kt}`
- `ui-compose/src/test/kotlin/at/exponential/ui/{Round2Suites,RealFontConformanceTest,Round2InteractionTest,ListBenchTest,CatalogConstantsTest}.kt`
- `ui-compose/src/test/conformance-known.json`
- `example/src/androidTest/.../KeyboardTest.kt`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/view-catalog/views.json`

## Contract notes
- **The generated Kotlin catalog doesn't compile.** `styleMediaPattern` emits `\\$breakpoint`, which Kotlin reads as a string template. `scripts/generate.ts` needs to escape `$`. Compose mirrors the constants it needs in `CatalogConstants`, locked by `CatalogConstantsTest`.
- **`FfiLeaf` has no `owner_component`.** The Rust `LeafRequest` has it. Compose works it out from the node table and the id prefix; adding it to the FFI would be cleaner.
- **Missing FFI helpers for the conformance runner:** template instances, container direction/align (the visual only carries leaf direction), animation timing and `keyframesCss`. The runner ports small pieces of these and reads the timing from a visual.
- **Sticky stops early.** The core keeps a direct child of a scroller pinned only within the scroller's own box (170 max in a 200-tall scroller); CSS keeps it pinned through the whole content. Rust lane.
- **Core-side divergences** the Compose dump shares with gpui:
  - ToggleGroup stretches to the full width.
  - No `Accordion/count` part is emitted.
  - The carousel indicator count is 1, not 2.
  - A stretched Image/Video keeps its 320-wide height (180 instead of 182.25).
  - Table column widths differ.
  - Badge/pill gaps from absent parts.
  - The nav count badge is wider.
  - The CodeBlock gutter layout changed mid-run while another lane edited the core.
- **The ratchet budget is measured against today's (pre-rewrite) web baseline.** After the React lane rewrites it, run `EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`. The same applies whenever the core changes.
- **iOS test expectations need the same update on the Mac.** The kitchen sink's TalkBack walk now starts with the AppBar and breadcrumb: `AccessibilityOrderTest` expects `Back, Inbox, Search, Boards, Sprint 12, EXP-42, Alex Chen, …`.

## Deferred
- **Real `backdropBlur`**: it currently paints the translucent background only. Compose's `RenderEffect` blurs a node's own content, not what's behind it. Next step: record the surface into a `GraphicsLayer` and draw it blurred and clipped under each blurred node.
- **FileUpload drag-and-drop**: files arrive only through the host's picker. Next step: `dragAndDropTarget` on the drop zone, feeding `filesPicked`.
- **TalkBack heading levels and role descriptions**: Compose 1.7 has no API for them.
- **Scroll cost at 100k rows**: `scrollStepMs` is 14 ms on the JVM against the core's own 3.2 ms. Next step: read nodes via `FfiDelta` (`nodesAt`) instead of all nodes every pass.
- **Kotlin measurer speed**: about 35 µs per question on this emulator; a full re-measure takes 38 ms for the kitchen sink.
- **Not built**: arm64-v8a and armeabi-v7a `.so` files (only x86_64 here). Web, iOS and desktop captures for the 22 new ids belong to the other lanes.

## Tests run
- `./gradlew test` (all modules, debug and release): ui-compose 51 + 51 passed, 1 skipped each (the Swift snapshot comparison, because the Swift file predates the catalog); primitives 5 + 5 passed.
- `bun run --filter @exponential-at/ui conformance:check .conformance/exponential-ui-compose.json`: CONFORMANT, 1609 cases in 24 suites.
- `./gradlew :example:connectedDebugAndroidTest`: 3/3 passed on `Medium_Phone_API_34` (accessibility order, typing, keyboard). Also passed on the Pixel 3a API 34 emulator earlier.
- `bun --filter @exp/view-catalog test`: 35 passed.
- `bun run shots --platform android --views <84 ids> --skip-seed --skip-relay`: 22 new, 62 updated, 0 failed.
- `ListBenchTest` (JVM, Linux, 32 cores): firstPaintMs 139, scrollStepMs 14.1, scrollToIndexMs 17.0, renderedItems 30.
- Release build on the emulator, bench loop: kitchen sink warm 1.96 ms, cold 105–181 ms; bench 200 warm 0.17 ms.
- Environment setup: installed NDK 29 and `cargo-ndk`, added the Android Rust targets to toolchain 1.96.0, built the x86_64 `.so` (`cargo ndk … --profile mobile`), and rebuilt the host facade (`cargo build -p exponential-ui-ffi --release`, `CARGO_TARGET_DIR=/tmp/xui-r2-target`).
- `CLAUDE.md` not touched.


## measure:1 (Converge)

{
 "origins": 1,
 "compose_origins": 35,
 "groups": [
  {
   "cause": "gpui: Carousel controls row is not in the contract (the web paints prev/next buttons and dots under the pages, gpui paints only the dots). This is the known survivor in fixtures/conformance-known.json 'survivors'.",
   "cases": 24,
   "nodes": "carousel (Carousel); the 186-196 cascaded nodes per kitchen-sink case follow from it",
   "delta": "\u0394h -28 (e.g. 324\u00d7218.25 \u2192 324\u00d7190.25); \u0394w 0"
  },
  {
   "cause": "compose: text widths differ between Robolectric's native text stack and Chromium (no wrap changes). 18 origins.",
   "cases": 24,
   "nodes": "code.code.0-3, resp-states-text, resp-main-text, resp-only-wide, form-plan.label.0/1, form-volume.label, form-volume.value, nf-files.title/hint/fileName.0/fileMeta.0, empty.description, card-row.body.title/subtitle",
   "delta": "\u0394w -3.25\u2026+7.61 (usually +1\u2026+4), \u0394h 0"
  },
  {
   "cause": "compose: overlay trigger row in the 900/1280 kitchen-sink cases shifts sideways. Trigger label widths come out slightly wider, and each trigger moves by the accumulated amount (cascade of the text-width drift). 8 origins.",
   "cases": 12,
   "nodes": "context-menu, hover-card, alert-dialog, menu (DropdownMenu), popover, sheet, tooltip, drawer",
   "delta": "position \u0394x +1\u2026+2.3, \u0394w \u22640.67, \u0394h 0"
  },
  {
   "cause": "compose: Badge label '3' is too narrow (4.84 vs 7.53), so the badge is narrower and the header text column gets wider. Same family as known cause hdr-badge. 3 origins.",
   "cases": 6,
   "nodes": "hdr-badge.label, hdr-title, hdr-sub",
   "delta": "label \u0394w -2.87\u2026-2.53; title/sub \u0394w +2.31\u2026+2.98"
  },
  {
   "cause": "compose: ToggleGroup is 12 px wider than content-sized (known cause 'toggles', which the contract says the web gets right).",
   "cases": 24,
   "nodes": "toggles (ToggleGroup) \"BoldItalic\"",
   "delta": "\u0394w +11.86\u2026+12.22"
  },
  {
   "cause": "compose: TreeGuides is 20 px tall where the web has 16 (known cause tree-guides, which the contract decided in gpui's favour; the compose result differs from the web's).",
   "cases": 24,
   "nodes": "tree-guides (TreeGuides)",
   "delta": "\u0394h +4"
  },
  {
   "cause": "compose: two boxes are 8 px too tall, the carousel indicator row (16 vs 8) and resp-states (Box). This produces the -8 y shift further down the page and the +8 heights in the responsive cases. 2 origins.",
   "cases": 24,
   "nodes": "carousel.indicator, resp-states (Box)",
   "delta": "\u0394h +8 (indicator y -14)"
  },
  {
   "cause": "compose: Markdown paragraph gets an extra line at 900/1280 (known cause main-md, now fixed on gpui but still open on Compose), and the card in the same grid row grows with it. 2 origins.",
   "cases": 12,
   "nodes": "main-md (Markdown), nav-card (Box)",
   "delta": "\u0394h +20"
  }
 ],
 "summary": "## Done\n\nI ran the harness and grouped the divergence origins. Nothing was edited. All outputs are in /tmp.\n\n**Web vs the committed baseline:** no drift. 48 cases, 13518/13518 nodes matched, 0 diffs.\n\n**Desktop (gpui, headless) vs the web baseline:** 1 origin left.\n- `carousel` (Carousel) differs by \u0394h -28 in all 24 kitchen-sink cases.\n- This is the known survivor in `fixtures/conformance-known.json`: the web paints a controls row (prev/next buttons around the dots) that the contract has no parts for.\n- It shifts 186-196 nodes further down in each case, so position counts are 183/193 per kitchen-sink case.\n- The 24 responsive cases are clean.\n- Coverage gaps are unchanged: 6-8 nodes only the web places (media-img.fallback, accordion.count.1, ring.label, chip-user.image.fallback, page-1.fallback, video.controls, audio.track, audio.controls) and 1 only gpui places (panels.grip.0).\n\n**Compose (Robolectric) vs the web baseline:** 35 origins. All 48 cases diverge (758 size, 6848 position, 452 origin rows), including every responsive case (+8 height). By cause:\n\n| Cause | Origins | Cases | Typical delta |\n|---|---|---|---|\n| Text width drift, Robolectric vs Chromium (code lines, nf-files, form labels, resp text, empty.description, card-row) | 18 | 24 | \u0394w -3\u2026+7.6, usually +1\u2026+4, no wraps |\n| Overlay trigger row shifts sideways (cascade of the text-width drift) | 8 | 12 | \u0394x +1\u2026+2.3 |\n| Badge label \"3\" too narrow (hdr-badge.label, hdr-title, hdr-sub) | 3 | 6 | label \u0394w -2.7, header text +3 |\n| `toggles` ToggleGroup too wide | 1 | 24 | \u0394w +12 |\n| `tree-guides` too tall | 1 | 24 | \u0394h +4 |\n| `carousel.indicator` and `resp-states` too tall | 2 | 24 | \u0394h +8 |\n| `main-md` extra line; `nav-card` grows with it | 2 | 12 | \u0394h +20 |\n\n## Contract notes\n\n- **gpui:** the only blocker left is the Carousel controls parts. The next step is to add `Carousel/controls` plus previous/next parts to recipes.json/layout.json; that is a contract decision.\n- **Compose:** toggles, tree-guides and main-md are origins already decided in the known-causes file, now open on Compose. The other groups are new: the 8 px carousel.indicator/resp-states heights and the badge label width. The text-width drift is the expected engine difference.\n\n## Deferred\n\n- I did not run gradle for Compose. The compose lane's Gradle daemons were active, so I used the dump it had just written (`build/conformance/compose.json`, 23:46, 48 cases) and compared it with this harness. A newer Compose run may give different numbers.\n- I did not run the `--pixels` step.\n\n## Tests run\n\n- `bun run --filter @exponential-at/ui conformance -- --details --out /tmp/xui-conf-audit` with `LANG/LC_ALL=en_US.UTF-8`, `CARGO_TARGET_DIR=/tmp/xui-r2-target`, `RUSTUP_TOOLCHAIN=1.96.0`: exit 0. Web 48 cases in 14.8 s, gpui dump 48 cases in 5.4 s. Log in `/tmp/xui-conf-audit.log`.\n- `bun packages/exponential-ui/conformance/run.ts --skip-web --desktop /tmp/xui-compose-snap.json --out /tmp/xui-conf-compose`: exit 0. Log in `/tmp/xui-conf-compose.log`."
}


## fix:r1:5 (Converge)

## Done

The fix is in Compose. Its ToggleGroup items put a hardcoded 6 dp gap between the icon and the label. On the web there is no such gap: `.xui-ToggleGroup-item` in `base-css.ts` is `inline-flex` with no gap, and the `ToggleGroup/item` recipe sets none in any built-in theme. gpui already reads the recipe's `gap` and uses 0 when it is missing (`exponential-ui-gpui/src/measure.rs:820`, `view/paint.rs`). The kitchen-sink `toggles` node has two items with icons ("Bold", "Italic"), so the extra width was 2 × 6 = 12 px, which is the Δw of +11.86 to +12.22.

I changed two files so the measurer and the painter agree:
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/SurfaceMeasurer.kt` (`toggleGroup`): the inner gap is now `item.px("gap") ?: 0f` instead of `6f`.
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/ButtonLeaves.kt` (`ToggleGroupLeaf`): the icon/label row uses `Arrangement.spacedBy(itemGap.dp)` with the same recipe gap, instead of `6.dp`.

## Contract notes

- I picked the web side using the round-2 contract: §7 says a ToggleGroup is content-sized, and §8 / `conformance-known.json` list `toggles` with `right: web`. The web, gpui and now Compose all take the icon/label gap from the `ToggleGroup/item` recipe, with 0 when it is missing. I did not change the contract docs or CLAUDE.md.

## Deferred

- **The real-font ratchet still fails, by +3 position in 12 cases.** The affected cases are the 900 and 1280 kitchen-sink cases in all three themes, ltr and rtl. In each one, size went down by 3 (30 → 27) and position went up by 3 (346 → 349). `toggles` sits below the separate `main-md` Markdown lines cause, which pushes it +20 in y. Now that its size is right it counts as a cascaded position divergence, so total divergence did not grow. I did not rewrite the budget, per the ground rules. The next step is the next measuring round's `EXP_UI_WRITE_FIXTURES=1 … --tests '*RealFontConformanceTest'`, after which the 390 and 600 cases also show the gain (for example neutral/600/ltr went from position 176 / size 26 to 171 / 22).
- **The Roborazzi image snapshots fail, and I did not re-record them.** `ComponentSnapshotImagesTest` (3 tests) failed. The first reported mismatches were `components/Stack.png` (a text rendering diff, not from this change) and `kitchen-sink-neutral.png` / `kitchen-sink-exponential.png`. The kitchen-sink images include the toggle group, so this change also moves pixels there. Each test stops at its first mismatch, so I could not check `components/ToggleGroup.png`. I did not re-record because that would also capture other lanes' unfinished changes. The next step is to re-record the images once all lanes have landed.

## Tests run

- `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` (with `ANDROID_HOME`, `CARGO_TARGET_DIR=/tmp/xui-r2-target`, UTF-8 locale): **failed** on the ratchet as described under Deferred. `build/conformance/report.txt` now has 0 `toggles` FIX lines; before the change there were 24, for example `web 131.78×24 → compose 144.0×24`. In `build/conformance/compose.json` all 24 `toggles` frames are now `w 132 × h 24`, against the web's 131.78 × 24 (rtl x 225 vs web 225.22).
- `./gradlew :ui-compose:testDebugUnitTest --tests '*.ConformanceTest' --tests '*.SnapshotTest' --tests '*.MeasurerRulesTest' --tests '*.ComponentSnapshotImagesTest'`: 9 tests ran, 1 skipped, **3 failed**. The 3 failures are all in `ComponentSnapshotImagesTest` (image diffs, see Deferred). `ConformanceTest`, `SnapshotTest` and `MeasurerRulesTest` passed.
- I did not run any gpui or Rust tests, because gpui already matched the web here and I touched no Rust.


## fix:r1:6 (Converge)

## Done

**Root cause.** The Compose measurer gave `TreeGuides` an intrinsic height of `ts.lineHeight` (20). The core styles TreeGuides `alignSelf: stretch` in `layout_tree/mod.rs:683`, so that 20 px set the row height. In the kitchen sink, `tree` is a horizontal Stack holding `tree-guides` and a caption `tree-text` (16 px line), so the row came out 20 px tall instead of 16.

**Which side is right.** gpui. Round 2 §7 says TreeGuides is "depth × 16 wide, stretched to its row", and `conformance-known.json` cause `tree-guides` decides it with `right: gpui`. gpui's measurer (`measure.rs:914`) already returns `(depth × 16, 0)` and React stretches it. Compose was the only side out of line.

**Fix** (one line in `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/SurfaceMeasurer.kt`): TreeGuides now has no height of its own and stretches to its row, like gpui.
```kotlin
// depth × `treeGuideColumn` wide; no height of its own (it
// stretches to its row, round 2 §7).
"TreeGuides" -> plain(max((props.num("depth") ?: 0.0).toFloat(), 0f) * TREE_GUIDE_COLUMN, 0f)
```
The painter (`TreeGuidesLeaf`, `fillMaxSize` Canvas) already draws at whatever size it is given, so nothing else changed.

**Evidence.** I ran `RealFontConformanceTest` with the old line put back temporarily, then with the fix:
- Before: 24 cases had `FIX size tree-guides (TreeGuides) web …32.0×16.0 → compose …32.0×20.0 Δ 0.0×4.0`, and `tree-guides` was in the origins list.
- After: `tree-guides` does not appear anywhere in `build/conformance/report.txt`.
- At 390 and 600 (12 cases), size drops by 3 and position by 5 in every case (e.g. exponential/390/ltr: s21→s18, p180→p175).
- At 900 and 1280 (12 cases), size drops by 2 and position rises by 2 in every case. The total is unchanged because an earlier origin above (`card-row`/`nav-card`, +20 px) still pushes this row down. So `tree-guides` and its row are now counted as moved by that earlier problem (position) instead of being a size problem of their own.

## Contract notes
- No contract change. This applies the existing round 2 §7 rule and the `tree-guides` decision in `conformance-known.json`, which already says gpui is right.

## Deferred
- **Android ratchet still fails at 900/1280 position.** The before run (old line, current worktree, rebuilt FFI) already failed by +1 position in those 12 cases, from other lanes' in-flight changes. This fix adds a further +2 there because of the re-counting above. The 390/600 cases improved (IMPROVED lines printed). I did not rewrite `ui-compose/src/test/conformance-known.json`, per the ground rules. Next step: the measuring round runs `EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` after all lanes land.
- **`ComponentSnapshotImagesTest` was not run.** Its geometry snapshot (`components.json`) already records TreeGuides at height 0.

## Tests run
- `cargo build -p exponential-ui-ffi --release --features cli` (`CARGO_TARGET_DIR=/tmp/xui-r2-target`): passed. This was needed because the default shared target's `.so` was missing `uniffi_exponential_ui_ffi_fn_clone_theme`.
- `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` with `ANDROID_HOME`, `CARGO_TARGET_DIR=/tmp/xui-r2-target` and `LANG/LC_ALL=en_US.UTF-8`: run with the old line and with the fix. Both runs fail the ratchet at 900/1280 position (+1 before, +3 after, explained above). Reports: `/tmp/xui-tg-before.txt`, `/tmp/xui-tg-after.txt`; logs: `/tmp/xui-tg-before.log`, `/tmp/xui-tree-guides-compose.log`.
- `./gradlew :ui-compose:testDebugUnitTest --tests '*SnapshotTest' --tests '*FixtureReplayTest' --tests 'at.exponential.ui.ConformanceTest'`: BUILD SUCCESSFUL. ConformanceTest 1/1, FixtureReplayTest 5/5, SnapshotTest 1 passed and 1 skipped.


## fix:r1:4 (Converge)

## Done

**Root cause.** The bug was on the Compose side, in `TextShaper.measure`. At min-content (`wrap <= 0`) it gave a one-line text (`lines: 1`) a width of **0**. The Badge label is `{variant: caption, lines: 1}`, so in an overflowing row the Badge could shrink down to its padding. In the header row, `hdr-text` shrinks and so does the Badge, by 2.69 px. That made the "3" label 4.84 px wide instead of 7.53, and `hdr-text`/`hdr-title`/`hdr-sub` took the extra width.

**Which side is right.** The web and gpui are right. gpui's `measure.rs` `para` treats a one-line text as `white-space: nowrap`, so its min-content is the whole line. The core (`surface/restyle.rs`) separately makes every `lines == 1` leaf a clipping box on x, so its own minimum is 0 and it still ellipsizes. Compose runs that same core through FFI, so changing only the shaper's answer matches it to web and gpui.

**Changes (Compose only):**
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/TextShaper.kt`: one line in `measure` plus its doc comment.
  ```kotlin
  wrap <= 0f -> Size(if (single) maxContent(text, ts) else minContent(text, ts), ts.lineHeight)
  ```
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/test/kotlin/at/exponential/ui/MeasurerRulesTest.kt`: this test asserted the old rule (a one-line text measures 0 at min-content). It now asserts min-content = `maxContent`.

**Evidence.** I ran the same case with and without the fix. `kitchen-sink/exponential/dark/390/ltr`, in a tree that also has the other lanes' edits:
- Without the fix: size 9 · position 183 · origins 4. The origins include `hdr-title`, `hdr-sub` and `hdr-badge.label` (7.53 → 4.84).
- With the fix: size 4 · position 183 · origins 1 (`carousel`, not this cause).
- The only lines that differ between the two runs are 5 size diffs that are gone: `hdr-badge`, `hdr-badge.label`, `hdr-sub`, `hdr-text`, `hdr-title`. Nothing new appears.
- In the full 48-case run there is no `hdr-` line left in `build/conformance/report.txt` (copy at `/tmp/vapp99-c4-report.txt`).

## Contract notes
- No contract or fixture change. The rule already holds on web, gpui and in the core: a `lines: 1` text is nowrap, its min-content is the whole line, and the leaf clips on x.
- I left the Compose ratchet (`src/test/conformance-known.json`) and the baseline untouched, as instructed.

## Deferred
- `SurfaceMeasurer.row` (a one-line text with chrome beside it) still returns `lead + trail` at min-content even when `lines == 1`. gpui's `row` returns the whole row in that case. No current case diverges because of this, so I left it out of this lane. Smallest next step: add a `single` parameter to `row`, pass `leaf.lines == 1` at its one call site, and return `w` when `wrap <= 0f && single`.

## Tests run
All Gradle runs used `CARGO_TARGET_DIR=/tmp/xui-r2-target ANDROID_HOME=$HOME/Android/Sdk LANG/LC_ALL=en_US.UTF-8`. The first run without `CARGO_TARGET_DIR` failed because the FFI library was not found (environment, not the fix).
- `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` (full matrix): **fails on the ratchet, not because of this fix.** The ratchet failure is position +2 to +4 in the kitchen-sink 390/600 cases. The with/without comparison above shows this fix adds 0 position diffs, so those come from other lanes' edits. The same run lists many IMPROVED entries, e.g. exponential/390 size 22 → 4.
- The same test with `EXP_UI_CONFORMANCE_ONLY=kitchen-sink/exponential/dark/390/ltr EXP_UI_CONFORMANCE_VERBOSE=1`, once with the fix and once with it temporarily reverted (then restored): results as in Evidence.
- `--tests '*MeasurerRulesTest' --tests '*GeometryTest'`: pass (3 + 3 tests).
- `--tests '*FixtureReplayTest'`: pass (5 tests).
- `--tests '*.ConformanceTest'`: **fails**, only on Carousel theme-recipe diffs (`/recipes/Carousel/controls`, `/gap`, `/backgroundColor`). Those come from another lane's Carousel recipe change, not from this fix.


## fix:r1:8 (Converge)

## Done

**Root cause (Compose measurer).** `MarkdownShaper.width()` in `packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/TextShaper.kt` measured each hard line with the inline marks stripped. It rebuilt every line as a plain `MarkdownInline(text = line, code = …)`, so the bold run `Looking for a Linear alternative` was measured at weight 400.

- The core asks for max-content (`wrap == null`) and gets that plain width, 402.33 dp.
- The height comes from laying the real runs (bold, 406.67 dp) out at 402.33 dp. That gives 2 lines, so the paragraph is 40 dp instead of 20 and main-md is 88 instead of 68.
- `nav-card` sits in the same grid row and grows by the same 20.

`widestWord()` had the same defect at min-content: plain text at body weight.

**Fix.** It mirrors the gpui fix (`impl MdText for Shaper` in `exponential-ui-gpui/src/measure.rs`):
- `width()` = `shaper.lineWidth(annotated(inlines, spec), markdownStyle(spec))`. The runs are kept, and `maxIntrinsicWidth` already returns the widest `\n` line.
- `widestWord()` = the widest `\S+` segment, each one measured as an `AnnotatedString.subSequence` with the runs it covers.

These two methods are the only edit, all inside the `MarkdownShaper` class (+ a private `WORD` regex). Nothing else changed. A temporary debug test and a debug `println` in `SurfaceMeasurer.kt` were both removed.

**Evidence (conformance fonts, density 3):**

| Paragraph | Width | Height at that width |
|---|---|---|
| Old width (plain) | 402.33 | **40** (extra line) |
| New width (with runs) | 406.67 | **20** |

`build/conformance/report.txt` before the fix had `12× main-md (Markdown) size` and `12× nav-card (Box) size` (e.g. `main-md web 536.67×68.0 → compose 536.67×88.0`). After the fix both nodes are gone from the report (`grep -c "main-md\|nav-card"` = 0). In the 900/1280 cases the budget output now reads lower, e.g. `kitchen-sink/exponential/dark/900/ltr: size 4 < budget 26`.

## Contract notes
- Round 2 §7 sets "Text, Markdown: a paragraph = its runs, no extra line", and `conformance-known.json` `main-md` has `right: web`. So the web is right and Compose was the wrong side. The `fix` list there names only `gpui`. Compose now complies too; the next measuring round can add it there. I did not touch the fixtures.
- Ratchet and baseline are untouched.

## Deferred
- I did not rewrite the ratchet. `RealFontConformanceTest` still fails, but on `position` budgets at 390/600 (e.g. `183 > budget 180`). That comes from a concurrent lane's Carousel change (`carousel` Δh −28, cascading 186 positions). The Carousel subtree has no Markdown, so it is unrelated to this fix.
- `ConformanceTest` fails on `theme-resolved` / `theme-recipes` Carousel recipe diffs, also from another lane, not markdown. Smallest next step: that lane rebuilds the FFI/theme outputs. The next measuring round then rewrites `src/test/conformance-known.json`.

## Tests run
All runs in `packages/exponential-ui-compose` with `ANDROID_HOME=$HOME/Android/Sdk CARGO_TARGET_DIR=/tmp/xui-r2-target`:
- `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`: main-md and nav-card no longer appear. The test still fails only on the Carousel position budgets above.
- A temporary `ZzMdDebugTest` (since deleted): per-block widths and heights as in the table above. The list items are unchanged at 20 dp.
- `./gradlew :ui-compose:testDebugUnitTest --tests '*ModelSmokeTest' --tests '*MeasurerRulesTest' --tests '*GeometryTest'`: BUILD SUCCESSFUL.
- `./gradlew :ui-compose:testDebugUnitTest --tests 'at.exponential.ui.ConformanceTest'`: fails only on the Carousel recipe diffs from another lane.


## fix:r1:7 (Converge)

## Done

Both origins of the cause are fixed on the Compose side. The web and gpui were already right; nothing in gpui, the core or React changed.

1. **`carousel.indicator` was 16 px tall instead of 8.** In `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/SurfaceMeasurer.kt`, the `Box`/`indicator` rule measured `dot + spacing("sm")`. gpui (`measure.rs` `("Box", Some("indicator"))`) and the web make the dots row one dot tall. The row is now `recipe.height ?: dot`, the same rule as gpui:
   ```kotlin
   val recipe = part("Carousel", "indicator", props)
   val dot = recipe.width ?: 8f
   plain(n * dot + max(n - 1f, 0f) * gap, recipe.height ?: dot)
   ```
2. **`resp-states` was 8 px taller.** Nothing in the Box itself was wrong. The Compose harness gave the surface a fixed height (`m.setViewport(width, 1_000_000f)`). The core therefore saw a known height of at least the width, matched `@media (orientation: portrait)` and applied `paddingVertical: $spacing.lg` (16 instead of 12, so +8). The round-1 contract (§ style conditions) says "portrait = height ≥ width; landscape when the height is unknown". The web surface (a container that only tracks width) and gpui's dump (`set_viewport(width, 0.0, max_h)`) are both content-sized, so landscape. In `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/test/kotlin/at/exponential/ui/RealFontConformanceTest.kt` the call is now `m.setViewport(width.toFloat(), 0f, 1_000_000f)`: content-sized, with the max only bounding list windowing, as gpui does.

**Evidence** (RealFontConformanceTest, all 48 cases): no FIX line names `carousel.indicator` or `resp-states` any more; the origin summary went from `24× carousel.indicator size` and `24× resp-states (Box) size` to none. In `kitchen-sink/exponential/dark/390/ltr` the indicator is now 20×8, like the web. `resp-states` matches the web in every case (94/46 tall). Every `responsive/*` case dropped to size 0, and position 0–4 against budgets of 18–21. Size counts in the kitchen-sink cases fell: exponential 22–26 → 4, neutral 26–30 → 13, playful 26–30 → 14.

## Contract notes

- The Carousel indicator part is one dot tall (the recipe's `height`, falling back to its `width`), with no padding of its own. The round-2 contract lane may want this line in round-2-contract.md; I did not edit that doc because another lane owns it.
- An orientation query only matches when the host gives a definite surface height. A content-sized surface (height ≤ 0) is landscape. The harnesses must not stand in for "unbounded" with a huge fixed height.

## Deferred

- **The ratchet now fails in 12 cases**, but by reclassification, not regression. For example `kitchen-sink/exponential/dark/390/ltr` is position 183 > budget 180 while size went 22 → 4; the neutral and playful 390/600 cases are +2 to +4 the same way. Each fixed node and its parents moved from "size" to "position". In the verbose 390 run, every remaining position diff is the −28 px shift from the known web-only Carousel controls row (`conformance-known.json`). The indicator's own leftover −18 has the same cause. Per the ground rules I did not rewrite the budget; the next measuring round should run `EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`.
- **Roborazzi snapshots changed and were not re-recorded:** `kitchen-sink-neutral.png` and `kitchen-sink-exponential.png` (the indicator row is 8 px shorter), plus `kitchen-sink-themes` and `components/Stack.png`. I can't tell which of these come from my change and which from concurrent lanes. They should be re-recorded once all lanes have landed.
- **The real Compose app still differs from gpui on viewport height.** `ExponentialSurface` passes the visible height to the core as a definite surface height; gpui passes height 0 with the visible height as the max. So orientation queries can match portrait in a scrolling Compose surface and not in gpui. I did not change it because it also affects overflow reporting and list windows. Smallest next step: have `SurfaceModel.autoViewportHeight` pass the visible height as `maxHeight` with height 0, then re-run ConformanceTest and HostTest.

## Tests run

- First run of `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` failed: the shared cargo-target FFI `.so` is stale (missing symbol `uniffi_exponential_ui_ffi_fn_clone_theme`). I rebuilt it with `CARGO_TARGET_DIR=/tmp/xui-r2-target cargo build -p exponential-ui-ffi --release --features cli` (OK) and used that target for the runs below.
- `CARGO_TARGET_DIR=/tmp/xui-r2-target ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` (Android SDK and UTF-8 locale env set): both origins are gone, and the run prints a long IMPROVED list. It fails only on the +3/+4 position reclassification above.
- The same test with `EXP_UI_CONFORMANCE_VERBOSE=1 EXP_UI_CONFORMANCE_ONLY=kitchen-sink/exponential/dark/390/ltr`: all 184 position diffs are the −28 carousel cascade, plus the indicator's −18.
- Full `./gradlew :ui-compose:testDebugUnitTest`: 52 tests, 6 failed. MeasurerRulesTest, GeometryTest and the rest passed. The failures:
  - the Roborazzi snapshots above;
  - ConformanceTest, failing only on `theme-resolved`/`theme-recipes` for `Carousel/controls|previous|next|root`, which another lane is adding right now;
  - RealFontConformanceTest (the ratchet above);
  - `ZzMdDebugTest` class-not-found, a stale class from another agent's build.

Edited files:
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/measure/SurfaceMeasurer.kt`
- `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui-compose/ui-compose/src/test/kotlin/at/exponential/ui/RealFontConformanceTest.kt`


## fix:r1:2 (Converge)

## Done

The cause was on the Compose side; the web (Chromium) values are correct. Under round 2 §8, text is laid out at its fractional shaped width, which is what Chromium and gpui both do. Compose diverged in three ways:

1. **Hinted advances.** Compose's default `TextMotion.Static` uses hinted, whole-pixel glyph advances, about +0.3 px per glyph at density 3. That accounted for the usual Δw of +1…+4 on `form-*`, `nf-files.*`, `empty.description` and `card-row.*`.
2. **Ceiled widths.** `TextShaper.lineWidth` rounded `maxIntrinsicWidth` up to whole dp, so labels in a row each gained up to 1 dp (the `code.code.*` and `resp-*` cases).
3. **Markdown widths.** Markdown max-content was measured without its bold/code runs, and min-content without the runs each word covers.

Fix, all under `packages/exponential-ui-compose/`:

- `ui-compose/src/main/kotlin/at/exponential/ui/measure/TextShaper.kt`
  - `style()` sets `textMotion = TextMotion.Animated` (unhinted, fractional advances).
  - `lineWidth()` returns the fractional line extent (`getLineRight − getLineLeft`, widest line) instead of a ceiled intrinsic width.
  - `MarkdownShaper.width` and `widestWord` now shape with their runs.
- `primitives/src/main/kotlin/at/exponential/ui/primitives/MarkdownView.kt`: the painter uses the same `TextMotion.Animated`, so paint matches measure.
- `ui-compose/src/main/kotlin/at/exponential/ui/compose/TextSlack.kt` (new): `TEXT_SLACK_PX = 2` and `Modifier.textSlack(bias)`. Frames round to whole px, so a text sitting exactly at its fractional max-content can come out up to 1 px short. When it is short by 1–2 px it is laid out at its own width and placed by the alignment bias, so it does not wrap or truncate.
- `ui-compose/src/main/kotlin/at/exponential/ui/compose/TextLeaves.kt`: `textSlack` is applied to the 4 text-painting leaves.
- `ui-compose/src/test/kotlin/at/exponential/ui/MeasurerRulesTest.kt`: the "widths are whole dp (ceil)" assertion described the old rule. It now asserts that widths are fractional and that max-content equals the line width.

Evidence: `ui-compose/build/conformance/report.txt` shows **0 FIX lines across all 48 cases** (kitchen-sink and responsive × 3 themes × 4 widths × ltr/rtl). Every kitchen-sink case now reports `size 0 · position 0 · wrap 0 · origins 0`, with compose height equal to web height (for example 7007.75 / 7007.75). In the 23:56 run the only remaining origin was the separate `carousel` Δh −28 cause, and that is gone now too.

## Contract notes

- No contract text changed; this follows round 2 §8: text is measured at its fractional shaped width (Chromium and gpui agree), with no px ceiling in the measurer.
- `TEXT_SLACK_PX` is a painter-only tolerance for px rounding. It never changes measured frames.

## Deferred

- **The ratchet test still fails, and not because of text width.** `RealFontConformanceTest` asserts because every kitchen-sink case now has `onlyCand 4 > budget 1` (`carousel.controls`, `carousel.previous`, `carousel.next`, `panels.grip.0`). These are coverage-only parts, most likely from the concurrent carousel lane. The test also prints "IMPROVED" (for example size 22→0 and position 180→0 per case). Per the rules I did not rewrite `src/test/conformance-known.json`; the next measuring round does that with `EXP_UI_WRITE_FIXTURES=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`.
- **The snapshot images test fails as an expected result of the fix.** `ComponentSnapshotImagesTest` (3 of 3 fail): fractional glyph positioning moves glyph edges by a sub-pixel. For example, `Stack.png` differs only in the anti-aliasing of "hild"; I looked at its compare image. The kitchen-sink PNGs also pick up other lanes' changes, so I did not re-record them in the middle of the round. Next step, once all lanes have landed: `EXPONENTIAL_UI_RECORD=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*ComponentSnapshotImagesTest'`.

## Tests run

- `CARGO_TARGET_DIR=/tmp/xui-r2-target cargo build -p exponential-ui-ffi --release --features cli`: OK. This was needed first: the default target's `.so` was stale (`undefined symbol uniffi_exponential_ui_ffi_fn_clone_theme`). Gradle has to run with `CARGO_TARGET_DIR=/tmp/xui-r2-target`.
- `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`: 0 origins in all 48 cases; the assertion fails only on the `onlyCand` coverage counts above.
- `./gradlew :ui-compose:testDebugUnitTest --tests '*MeasurerRulesTest'`: 3/3 pass after the test update (before it, `measurerRules` failed with expected 70.21 but got 71.0).
- Full `./gradlew :ui-compose:testDebugUnitTest --continue`:
  - Pass: CatalogConstants 1/1, FixtureReplay 5/5, Geometry 3/3, Host 8/8, Interaction 10/10, ModelSmoke 3/3, Round2Interaction 9/9, Snapshot 2 (1 skipped).
  - Fail: ComponentSnapshotImages 3/3 and RealFontConformance 1/1, both explained above.
  - Gradle also could not write the result XMLs for `ListBenchTest` and `ComponentSnapshotImagesTest`, probably because another gradle run was using the same build directory. `ListBenchTest` was not rerun; I reran `ComponentSnapshotImagesTest` on its own.


## fix:r1:3 (Converge)

## Done
The cause was in Compose, and the web is right. After the fix, all 48 real-font cases show 0 size and 0 position divergences, and the report prints 0 FIX lines.

- **Root cause:** `TextShaper.lineWidth` used Compose's `maxIntrinsicWidth`, which is ceiled to a device pixel (whole dp at the test's mdpi). Every trigger label came out up to about 0.7 px too wide, and in a row of 8 overlay triggers that added up to the +1…+2.3 px sideways shift. The same drift hit resp-chip, `form-plan` labels and `nf-files` text.
- **Contract:** round 1/2 lay text out at its fractional shaped width (Chromium and gpui both do), so Compose was the wrong side.
- **Changes** (all in `packages/exponential-ui-compose/ui-compose/`):
  - `src/main/kotlin/at/exponential/ui/measure/TextShaper.kt`: `lineWidth` now returns the fractional width of the widest line (`getLineRight - getLineLeft`, divided by density), with no ceil.
  - `src/main/kotlin/at/exponential/ui/compose/TextSlack.kt` (new): `Modifier.textSlack(bias)` and `TEXT_SLACK_PX = 2`. Compose still ceils a text's own width and frames round to whole px, so a label sized at its fractional width can come out up to 1 px short. When the text needs at most 2 px more than its box, it is laid out at its full width and placed by alignment (left, centre, right, with rtl handled). It never wraps or gets an ellipsis.
  - `src/main/kotlin/at/exponential/ui/compose/TextLeaves.kt`: `textSlack` is applied at the text paint sites (plain text, the multi-line Text leaf, CodeBlock lines, the Link label).
  - `src/test/kotlin/at/exponential/ui/MeasurerRulesTest.kt`: the old check that widths are whole dp now checks that they are fractional. This edit was already in the file when I went to make it, probably from a concurrent lane; it matches the new rule.
- **Evidence:** in `kitchen-sink/playful/dark/900/ltr`, the overlay row went from hover-card +1.03 and context-menu +1.22 to |Δ| ≤ 0.07 for every trigger (dialog through context-menu). `/tmp/c3-full.txt` is the report before, `/tmp/c3b-report1.txt` the report after.

## Contract notes
- Text measures at its fractional shaped width on every renderer. A painter whose text widget rounds widths up has to absorb that when painting, not by widening the layout. Compose does this with `textSlack`, at most 2 px. This could become a rule line in `round-2-contract.md` (text measurement) when the contract lane next edits it. I did not touch the shared docs or fixtures.

## Deferred
- **`ComponentSnapshotImagesTest` (3 tests) fails.** The images differ: `Stack.png`, `kitchen-sink-*.png`. The `Stack_compare.png` diff is glyph anti-aliasing only ("hild"; the "C" and the text position do not move). That looks like the `TextMotion.Animated` change another lane made in `TextShaper.style`, but the kitchen-sink images may also hold sub-pixel trigger moves from this fix. I could not split the two without reverting shared files that other lanes are using, so I did not re-record. Next step: once the round's lanes land, re-record with `EXPONENTIAL_UI_RECORD=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*ComponentSnapshotImagesTest'`.
- **`RealFontConformanceTest` still fails, but only on `onlyCand 4 > budget 1`.** The extra Compose-only nodes are `carousel.controls`, `carousel.previous` and `carousel.next` from the carousel lane; there are no size or position regressions. Lowering the ratchet is left to the next measuring round.

## Tests run
- `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest' --tests '*MeasurerRulesTest' --tests 'at.exponential.ui.ConformanceTest' --offline --continue` (ANDROID_HOME, CARGO_TARGET_DIR=/tmp/xui-r2-target, en_US.UTF-8): `ConformanceTest` passed. `MeasurerRulesTest` failed on the old whole-dp check, then passed after the test update. `RealFontConformanceTest` failed only on `onlyCand`; the report has 0 FIX lines and every case shows size 0, position 0.
- `./gradlew :ui-compose:testDebugUnitTest --offline --continue` (full suite): 51 tests, 4 failed, 1 skipped. The 4 failures are the 3 `ComponentSnapshotImagesTest` image diffs and `RealFontConformanceTest` on `onlyCand` only, both covered under Deferred.


## fix:r1:1 (Converge)

## Done

The Carousel cause is fixed on all three Linux renderers. The web, gpui and Compose now place the same controls row under the pages, and the size divergence is 0 in all 24 kitchen-sink cases. Both ratchet gates still fail, though, because the committed web baseline is stale: see Deferred.

**Root cause:** the web painted a controls row: a 28 px Previous button, the dots, a 28 px Next button, plus `$spacing.sm` padding-top. Those were CSS literals in `base-css.ts`, and the contract had no parts for them. gpui painted only the dots row, so its box was 28 px shorter. The contract was the side to fix, then both renderers place the same parts.

What changed:
- **Contract:**
  - `catalog/recipes.json` Carousel parts now include `controls`, `previous` and `next`.
  - `themes/neutral.theme.json` gets recipes for them: `Carousel/root` gap `$spacing.sm`, `Carousel/controls` gap `$spacing.sm`, and 28×28 round ghost buttons with hover and disabled states.
- **Core** (`apps/desktop/crates/exponential-ui/src/layout_tree/containers.rs`): with 2+ pages it places a centred `controls` row holding `previous` Button, `indicator` (the dots) and `next` Button. Each button carries its `target` page and is disabled at the ends unless `loop` is set.
  - `surface/events.rs` handles a `press` on previous/next by moving to that page.
  - gpui and Compose get these nodes from the core. gpui's normal press path already sends the press to the core.
- **React** (`packages/exponential-ui-react/src/natives/media.tsx`, `base-css.ts`):
  - The controls row and both buttons are now addressable parts (`data-xui-id` / `data-xui-part`).
  - The CSS literals (28 px, padding-top, opacity) are gone; sizes come from the recipes.
- **This continuation:**
  - Removed the now-false `carousel` entry from `SURVIVORS` in `apps/desktop/crates/exponential-ui-gpui/tests/conformance.rs`.
  - Added the controls-row rule to the Carousel row of `docs/round-2-contract.md` §7.
  - Added a new core test, `apps/desktop/crates/exponential-ui/tests/carousel_controls.rs`.

Evidence: a fresh web dump compared with a fresh gpui dump, all 24 kitchen-sink cases, carousel nodes, ±1 px tolerance: 0 mismatches. Example, neutral 390 ltr, identical on both sides:

| node | frame (x, y, w×h) |
|---|---|
| carousel | 33, 4313.25, 324×218.25 (was 190.25 on gpui) |
| carousel.page | 33, 4313.25, 324×182.25 |
| carousel.controls | 33, 4503.5, 324×28 |
| carousel.previous | 149, 4503.5, 28×28 |
| carousel.indicator | 185, 4513.5, 20×8 |
| carousel.next | 213, 4503.5, 28×28 |

## Contract notes
- `Carousel/controls`, `/previous` and `/next` are layout parts the conformance dump compares (`carousel.controls`, `carousel.previous`, `carousel.next`). The web's dots wrapper keeps the id `carousel.indicator`; single dots carry no id.
- A Carousel with one page has no controls row.
- `fixtures/conformance-known.json` still lists the carousel survivor. I left it because the ratchet writer rebuilds `survivors` from the current origins, and the carousel origin is gone, so the next rewrite drops it.

## Deferred
- **Both gates fail, gpui and Compose, until the baseline is re-measured.** Every kitchen-sink case shows `onlyCand 4 > budget 1`, in all 24 cases. The committed web baseline (`conformance-baseline.json`) was dumped before React gave the new parts ids, so it has no `carousel.controls/previous/next`. The fourth node, `panels.grip.0`, is already in the budget and belongs to another cause. A fresh web dump has all three nodes and matches gpui and Compose exactly. Next step for the measuring round:
  1. `bun run --filter @exponential-at/ui conformance -- --write-baseline`
  2. `EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance`
  3. Rewrite the Compose `src/test/conformance-known.json`.
  
  I did not do these because the brief forbids it.
- Swift was not touched (owned by the Mac run). It needs the same three parts from the core.

## Tests run
- `cargo test -p exponential-ui-gpui --test conformance` (CARGO_TARGET_DIR=/tmp/xui-r2-target, RUSTUP_TOOLCHAIN=1.96.0): size 0 in all kitchen-sink and responsive cases. The FIX lines report size 4→0 and position 183/193→0 per kitchen-sink case. No carousel origin is left. The gate fails only on `onlyCand 4 > budget 1` (stale baseline, see Deferred).
- `bun run ./conformance/run.ts --only kitchen-sink`, plus a script diffing the fresh web and gpui dumps: web against the committed baseline has 0 size, position or wrap differences and 3 extra nodes per case (the new parts). Web against gpui on carousel nodes: 0 mismatches in 24 cases.
- `./gradlew :ui-compose:testDebugUnitTest --tests '*ConformanceTest*' --tests '*InteractionTest*'`, after `cargo build -p exponential-ui-ffi --release --features cli`: 21 tests, 1 failed. The failure is `RealFontConformanceTest`, for the same stale-baseline reason (`onlyCand 4 > budget 1`). Compose heights equal the web's in every case.
- `cargo test -p exponential-ui`: all suites pass.
- `cargo test -p exponential-ui --test carousel_controls`: 3 passed.
- `cargo test -p exponential-ui-gpui --test suites`: pass.
- `bun x vitest run src/primitives/carousel.test.tsx src/round2.test.tsx src/fixtures.test.tsx` (exponential-ui-react): 713 passed.
- `bun test src/round2.test.ts` (exponential-ui): 23 passed.


## ratchet:r1 (Converge)

## Done

- **Full conformance run** (`conformance/run.ts`, web + gpui desktop, 48 cases):
  - **Web vs. baseline:** 0 size or position diffs. The only drift was 3 new nodes per kitchen-sink case (`carousel.controls`, `carousel.previous`, `carousel.next`), 72 nodes in all. This round meant to add them: `recipes.json` Carousel parts now include `controls/previous/next`, and contract §7 has the Carousel row. So I rewrote the baseline with `--write-baseline`, and the re-check matches all 13,590 nodes with 0 drift.
  - **gpui vs. web:** 0 origins, 0 size, 0 position, 0 wrap. The 24× Carousel survivor is gone.
- **gpui ratchet lowered** (`EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance`). `fixtures/conformance-known.json` keeps `causes` (53) and `rules` (9); `survivors` is now `[]`.
  - Totals went from size 96 / position 4512 / wrap 0 / onlyRef 174 / onlyCand 24 to size 0 / position 0 / wrap 0 / onlyRef 174 / onlyCand 24.
- **Compose is wired** (`RealFontConformanceTest`), so I lowered its ratchet too. `packages/exponential-ui-compose/ui-compose/src/test/conformance-known.json` is a new, untracked file. Totals went from size 758 / position 6848 / wrap 0 / onlyRef 174 / onlyCand 24 to size 0 / position 0 / wrap 0 / onlyRef 174 / onlyCand 24, with 0 origins.
- **Fallout fixed:**
  - The Compose test failed with `UnsatisfiedLinkError: uniffi_exponential_ui_ffi_fn_clone_theme`, because it was loading an old FFI `.so` from the default cargo target. I rebuilt `cargo build --release -p exponential-ui-ffi` into `/tmp/xui-r2-target` and ran gradle with `CARGO_TARGET_DIR` pointing there.
  - Gradle skips the test as up to date when only `EXP_UI_WRITE_FIXTURES` changes, so the write and the re-check both need `--rerun`.
- **Docs that had become false:**
  - The suite count in `manifest.json` went from 1609 to 1613 (catalog +7, theme-recipes +11 versus HEAD, from the Carousel parts). I updated `packages/exponential-ui/conformance/README.md:9`, `packages/exponential-ui/docs/round-2-contract.md:287` and `apps/desktop/crates/exponential-ui-gpui/README.md:421`.
  - In `apps/desktop/crates/exponential-ui-gpui/README.md` I replaced the "one survivor left: the Carousel controls row" line with "0 origins, 0 survivors", and removed the Carousel bullet from Known divergences.
  - CLAUDE.md is unchanged.

## Contract notes

- **What is left is coverage only, the same for gpui and Compose:**
  - The web places parts that neither native renderer lays out as nodes: `media-img.fallback`, `accordion.count.1`, `ring.label`, `video.controls`, `audio.track`, `audio.controls`, plus `chip-user.image.fallback` and `page-1.fallback` at some widths. That is 6–8 per kitchen-sink case, 174 in total.
  - Both native renderers place `panels.grip.0`, which the web dump lacks: 1 per kitchen-sink case, 24 in total.
- The ratchet counts this coverage but does not compare it, and it is now locked.

## Deferred

- **The 174 web-only parts.** gpui and Compose paint them but do not place them as nodes (the gpui README says so). Smallest next step: place them as layout nodes in each renderer's dump, then run the writer again.
- **`panels.grip.0` missing on the web.** Smallest next step: add `data-xui-id="<id>.grip.<i>"` + `data-xui-part` to the React Resizable grip, then rewrite the baseline. That drops onlyCand from 24 to 0 on both ratchets.

## Tests run

| command | result |
|---|---|
| `bun run ./conformance/run.ts --out /tmp/xui-ratchet1` (web + gpui) | web: 3 coverage-only nodes per kitchen-sink case (Carousel parts); gpui: 0 origins |
| `bun run ./conformance/run.ts --skip-desktop --write-baseline --out /tmp/xui-ratchet2` | baseline rewritten; 13,590/13,590 matched, 0 drift |
| `cargo test -p exponential-ui-gpui --test conformance -- --nocapture` | 2 passed, IMPROVED |
| `EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance`, then the same without the variable | budget written; 2 passed, no IMPROVED |
| `cargo build --release -p exponential-ui-ffi` | ok (`clone_theme` exported) |
| `./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'` (old `.so`) | FAILED (`UnsatisfiedLinkError`) |
| the same with `CARGO_TARGET_DIR=/tmp/xui-r2-target` | passed, 0 origins |
| the same with `EXP_UI_WRITE_FIXTURES=1 … --rerun`, then plain `--rerun` | budget written; passed |
| `cargo test -p exponential-ui -p exponential-ui-gpui` | 337 passed, 0 failed (26 test binaries) |
| `bunx vitest run` (ui-react) | 1196 passed, 1 skipped (33 files passed, 1 skipped) |
| `bunx vitest run --config vitest.browser.config.ts` (ui-react, includes `suites.test.ts`) | 130 passed, 1 skipped (8 files) |
| `bun test ./src ./conformance` (`@exponential-at/ui`) | 320 pass, 0 fail |
| `conformance-check.ts` on the 4 reports in `.conformance/` | all CONFORMANT, 1613 cases in 24 suites (Compose report from 00:22, Rust core / gpui / React reports from this run) |

**Changed files:**
- `packages/exponential-ui/fixtures/conformance-baseline.json`
- `packages/exponential-ui/fixtures/conformance-known.json`
- `packages/exponential-ui-compose/ui-compose/src/test/conformance-known.json`
- `apps/desktop/crates/exponential-ui-gpui/README.md`
- `packages/exponential-ui/conformance/README.md`
- `packages/exponential-ui/docs/round-2-contract.md`


## measure:2 (Converge)

{
 "origins": 0,
 "compose_origins": 0,
 "groups": [
  {
   "cause": "Coverage only (not compared): parts painted only by the web renderer: media-img.fallback, accordion.count.1, ring.label, video.controls, audio.track, audio.controls (all 24 kitchen-sink cases); chip-user.image.fallback (18 on desktop); page-1.fallback (12)",
   "cases": 24,
   "nodes": "media-img.fallback, accordion.count.1, ring.label, video.controls, audio.track, audio.controls, chip-user.image.fallback, page-1.fallback",
   "delta": "n/a (missing node, not a geometry delta); 6-8 per kitchen-sink case, 174 total on desktop"
  },
  {
   "cause": "Coverage only (not compared): a part painted only by gpui/Compose: panels.grip.0, the resizable handle grip. The web dump does not tag it.",
   "cases": 24,
   "nodes": "panels.grip.0",
   "delta": "n/a; 1 per kitchen-sink case, 24 total"
  }
 ],
 "summary": "## Divergence origins: none\n\nDesktop (gpui) and Compose each have **0 divergence origins** across all 48 cases (24 kitchen-sink + 24 responsive), compared with the committed web baseline. Every matched node is within tolerance on size, position and wrap. The only remaining gaps are coverage: parts that one side paints and the other doesn't tag. The harness doesn't compare these.\n\n### Done\n\n**Web (react-chromium vs the committed baseline)**\n- 48 cases, 13,590 of 13,590 nodes within tolerance.\n- 0 differences of any kind; heights identical.\n\n**Desktop (gpui-cosmic-headless vs the web baseline)**\n- 48 cases, 13,416 of 13,416 matched nodes within tolerance.\n- Size 0, position 0, wrap 0, \u00b11-line 0, origins 0.\n- Largest delta: 1.0 px for the exponential/neutral kitchen sink, 0.3 for playful, at most 0.1 for responsive. All are within the \u00b11 px tolerance.\n- Page heights are identical in every case.\n\n**Compose (Robolectric RealFontConformanceTest)**\n- Passed. All 48 cases show origins 0 and size/position/wrap 0.\n- Heights match the web baseline (for example 7007.75 / 7007.75).\n\n**Coverage-only nodes** (identical on desktop and Compose):\n\n| Painted only by | Nodes | Cases | Total on desktop |\n|---|---|---|---|\n| web | `media-img.fallback`, `accordion.count.1`, `ring.label`, `video.controls`, `audio.track`, `audio.controls` | all 24 kitchen-sink | 6 per case |\n| web | `chip-user.image.fallback` | 18 kitchen-sink (widths 600 and up, plus playful 390) | |\n| web | `page-1.fallback` | 12 kitchen-sink | |\n| | **all web-only nodes** | | 174 |\n| gpui / Compose | `panels.grip.0` | all 24 kitchen-sink | 24 |\n\nNo responsive case has coverage gaps.\n\n### Contract notes\n- `fixtures/conformance-known.json` has decision entries for \"accordion count\" and for `Image/fallback` painting. I found no entries for `panels.grip`, `ring.label`, `audio.*` or `video.controls`.\n- The ratchet budget in `ui-compose/src/test/conformance-known.json` already holds these counts.\n\n### Deferred\n- **Compose has to be run with `CARGO_TARGET_DIR=/tmp/xui-r2-target`.** Without it, gradle falls back to `apps/desktop/target`. That path resolves to the shared `exponential.cargo-target`, where `libexponential_ui_ffi.so` dates from 18:14, older than the current Kotlin bindings. The run fails with `UnsatisfiedLinkError` on `uniffi_exponential_ui_ffi_fn_clone_theme`. This is an environment problem, not a regression; I did not rebuild the shared library.\n- **Swift was not run.** The Mac lane owns it.\n\n### Tests run\n- `LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 CARGO_TARGET_DIR=/tmp/xui-r2-target RUSTUP_TOOLCHAIN=1.96.0 bun run --filter @exponential-at/ui conformance -- --details`: exit 0. Dumps and `report.json` are in `/tmp/xui-conformance`.\n- `ANDROID_HOME=$HOME/Android/Sdk ./gradlew :ui-compose:testDebugUnitTest --tests '*RealFontConformanceTest'`: failed with the stale shared `.so` above.\n- The same command with `CARGO_TARGET_DIR=/tmp/xui-r2-target`: passed (exit 0). Output is in `packages/exponential-ui-compose/ui-compose/build/conformance/{compose.json,report.txt}`.\n\nI edited nothing."
}


## review:docs (Review)

{
 "findings": [
  {
   "file": "apps/ui-site/src/sdk/VariantGallery.tsx",
   "line": 11,
   "severity": "minor",
   "summary": "`hasVariants` checks `variantsOf(doc, {}, \u2026)` with an empty base, but the gallery itself uses the `thumbProps` base. When the specimen already sets a boolean prop to true, the boolean variant is dropped and the gallery is empty while the section still renders.",
   "failure": "Confirmed in the built dist: /components/app-bar/, /components/nav-row/ and /components/tree-guides/ each show a numbered \"03 Variants\" section (and a sidebar scroll-spy entry) whose `<div class=\"sdk-variants\"></div>` is empty. All three have 1 variant with the empty base and 0 with thumbProps.",
   "fix": "Compute the variants once, with the same base the gallery uses: build them in Component.tsx (or export a `variantsFor(doc, specimen)` used by both) and gate the section on `variants.length > 0`. Add a test that `hasVariants(doc)` equals `VariantGallery`'s list being non-empty for every doc."
  },
  {
   "file": "apps/ui-site/src/lib/docs-coverage.test.ts",
   "line": 62,
   "severity": "minor",
   "summary": "The variants test claims \"variants change exactly one prop\" but asserts `changed.length <= 1`, so variants that change nothing pass. 19 components have such no-op variants today (e.g. `Button variant \u00b7 default`, `Stack direction \u00b7 horizontal`), and the empty-gallery case above is not caught either.",
   "failure": "A variant identical to the base preview (or a regression where every variant collapses to the base) passes the test.",
   "fix": "Assert `changed.length === 1` for variants whose option differs from the base value, keeping the current-value variant on purpose. Also add the hasVariants and gallery-non-empty consistency assertion."
  },
  {
   "file": "apps/ui-site/src/sdk/ComponentStudio.tsx",
   "line": 131,
   "severity": "minor",
   "summary": "The studio stage and every MiniSurface (index cards, variants) prerender with `mode='dark'` and an inline dark theme background, because `useScheme` returns `dark` until after hydration. Light-scheme visitors see dark stage and thumbnail boxes flash to light. `.sdk-card-thumb`'s light override loses to the inline `style.background`.",
   "failure": "OS set to light (or stored `light`): the first paint of /components/ shows 82 dark thumbnail boxes and /components/button/ a dark hero stage until JS hydrates and the effects run. Lighthouse does not see this flash.",
   "fix": "Leave the background inline style out until hydrated, or drive it with CSS from `html[data-scheme]`: e.g. a `--stage-bg-light`/`--stage-bg-dark` custom property pair, chosen by `:root[data-scheme=light]`, so the pre-paint script already picks the right colour."
  },
  {
   "file": "packages/site-shell/src/docs.tsx",
   "line": 141,
   "severity": "minor",
   "summary": "`role=heading aria-level=2` on the section tag gives an accessible name built from adjacent spans with no separator (\"01Preview\", \"03Variants\"). The change also applies to every DocsSection on exponential.at (shared package), which the lane did not check for heading order.",
   "failure": "Screen readers announce \"zero one Preview\" or \"01Preview\" as the heading. Marketing pages that already have an `<h2>` inside a DocsSection now get two level-2 headings back to back.",
   "fix": "Mark `.docs-section-num` and `.docs-section-line` `aria-hidden` (or add an `aria-label={label}`), and spot-check the marketing DocsSection pages for duplicate h2s."
  },
  {
   "file": "apps/ui-site/src/sdk/ComponentStudio.tsx",
   "line": 121,
   "severity": "minor",
   "summary": "The preview bar is `role=\"toolbar\"` but has no roving tabindex or arrow-key handling, so it breaks the toolbar keyboard contract. It holds 4 groups (12 buttons) plus Reset, all separate Tab stops.",
   "failure": "An assistive-technology user told \"toolbar\" presses arrow keys and nothing moves.",
   "fix": "Drop `role=toolbar` (each `.sdk-seg` already has `role=group` with a label), or implement arrow-key roving across the buttons."
  },
  {
   "file": "apps/ui-site/src/sdk/embed.ts",
   "line": 36,
   "severity": "minor",
   "summary": "The embed code ignores the studio's Direction and Width switches: it carries only theme and mode. A user who sets RTL copies LTR code. The escaping tests only use `toContain` and never compile or round-trip the Swift, Kotlin or Rust output.",
   "failure": "With RTL selected, the React/SwiftUI/Compose/gpui snippets render LTR. A future escaping regression (e.g. `\"\"\"#` inside Swift JSON) is caught only if it happens to drop the asserted substring.",
   "fix": "Pass `direction` through (React `direction` prop; the native hosts' layout-direction option, or a comment) or say in the note that the direction is not included. Make the test parse each snippet's embedded string back to the original JSONL: JS template eval, and regex extraction for the raw-string forms."
  },
  {
   "file": "apps/ui-site/src/components/Shell.tsx",
   "line": 12,
   "severity": "minor",
   "summary": "The phone menu is a bare `<details>`: Escape does not close it, a click outside does not close it, and it stays open over the content until the summary is pressed again. `theme-color` in index.html follows only the OS preference, not the stored scheme choice.",
   "failure": "On a phone, opening the menu and tapping the page leaves the floating nav covering content. A user who chose light on a dark OS gets a dark browser chrome bar.",
   "fix": "Add a small effect that closes the details on Escape or an outside pointerdown. Have the pre-paint script and `setScheme` update the `theme-color` meta."
  },
  {
   "file": "apps/ui-site/src/lib/catalog.ts",
   "line": 42,
   "severity": "minor",
   "summary": "`hasShot` in catalog.ts duplicates `shotExists` in Shot.tsx and nothing uses it.",
   "failure": "Two sources for the same shot-index lookup can drift.",
   "fix": "Delete `hasShot`, or make Shot.tsx/labels.tsx import it and drop `shotExists`."
  },
  {
   "file": "apps/ui-site/src/pages/ComponentsIndex.tsx",
   "line": 160,
   "severity": "minor",
   "summary": "The index thumbnails at phone width (2 cards per row, zoom 0.72) squeeze some multi-column specimens until text breaks mid-word.",
   "failure": "components-phone.png shows the Grid card's cells rendered as \"Cel / l1\", \"Cel / l2\": the thumbnail looks broken on phones.",
   "fix": "On \u2264720px give the mini render a fixed logical width (e.g. 360px) scaled down with zoom, instead of `100/zoom %` of a ~170px card, so specimens lay out at a sane width."
  }
 ],
 "missing_from_scope": [
  "`playwright` is used by scripts/screenshots.ts but not declared in apps/ui-site/package.json. The lane noted it, but the only reason given was a shared bun.lock; nothing is tracked to fix it.",
  "No test ties the dist output to the CSP hash: scheme.test.ts hashes the source index.html, and nothing would catch a future Vite or prerender transform of the inline script. The current dist does match.",
  "The index and variants renders have no RTL and no light-mode coverage in the screenshots or tests, beyond the one RTL Button shot.",
  "Hidden-component exclusion is asserted only in the test; ComponentsIndex and ORDERED_DOCS rely on COMPONENT_DOCS never containing `hidden` entries, and nothing checks that."
 ],
 "verdict": "The lane's claims hold. I re-ran `bun run test` (28 pass, 0 fail) and ui-site plus site-shell typecheck (clean). The built dist's inline script matches the CSP hash. I checked the embed snippets against the real React, Swift, Compose and gpui host APIs (MemoryTransport.feedJsonl queues until connect; the HostOptions shapes, ThemeHandle.builtin, builtin_theme and Mode match) and they are correct. The copy facts I spot-checked are accurate: Sidebar is a macro, the five harness fonts exist, Table is native. I found no generated files hand-edited, no loosened ratchets and no edits outside the lane's paths in the diff. No blockers or majors. The one confirmed bug: the Variants section renders empty on the AppBar, NavRow and TreeGuides pages, because `hasVariants` uses a different base than the gallery. The rest are minor: the variants test asserts at most one change rather than exactly one, light-scheme visitors see a dark stage and thumbnails flash before hydration, section headings read as \"01Preview\" (plus marketing's heading order changes via site-shell), the toolbar role has no arrow keys, the embed code ignores RTL and width, the phone menu does not close on Escape or an outside tap, `hasShot` is dead, and some thumbnails are cramped on phones."
}


## review:compose (Review)

{
 "verdict": "CHANGES REQUESTED. Most of the lane's claims check out against the current tree. The manifest suites are CONFORMANT (re-ran conformance:check: 1613 cases now, 24 suites). RealFontConformanceTest, ConformanceTest, CatalogConstantsTest, Round2InteractionTest and ListBenchTest all pass, and view-catalog is 35/35. The generator's `\\$breakpoint` string-template bug is real (generated/ExponentialUICatalog.generated.kt:14).\n\nThe claim that the unit suite is green does not reproduce. With ANDROID_HOME and CARGO_TARGET_DIR=/tmp/xui-r2-target set, `./gradlew :ui-compose:testDebugUnitTest` gives 51 tests and 3 FAILED, all in ComponentSnapshotImagesTest: Stack.png, kitchen-sink-exponential.png and kitchen-sink-neutral.png. The failures are deterministic on rerun. They are probably caused by theme/recipe/core edits that landed after the lane recorded its snapshots (themes/*.theme.json, recipes.json and the .so all changed after 23:44). Either way the gate is red right now.\n\nThere are also several real code findings:\n- The Resizable drag reads pointer positions relative to the handle, which itself moves during the drag.\n- Chart value labels, the chart tooltip, the chart summary and the slider value skip the surface formatter (the contract requires it).\n- Every scroll step runs a full layout pass and re-reads all nodes, even for plain scroll containers.\n- The Swift\u2194Compose snapshot parity assertion was weakened to an assumption, so it now skips silently.\n\nThe README contradicts the code in two places: it says widths are ceiled, and that a `lines:1` text shrinks to 0 at min-content.",
 "findings": [
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/test/snapshots/images/",
   "severity": "major",
   "summary": "Roborazzi image gate is red at the current tree: ComponentSnapshotImagesTest fails 3/3 (Stack.png, kitchen-sink-exponential.png, kitchen-sink-neutral.png changed); the lane's '51 passed' does not reproduce.",
   "failure": "ANDROID_HOME=~/Android/Sdk CARGO_TARGET_DIR=/tmp/xui-r2-target ./gradlew :ui-compose:testDebugUnitTest gives '51 tests completed, 3 failed'; it fails the same way on rerun. Stack_compare.png shows the 'Child' text shifted by a sub-pixel. The SnapshotTest geometry (components.json) still passes, so this is a paint drift, probably from theme/recipe/core edits newer than the recording (themes/*.theme.json, catalog/recipes.json and libexponential_ui_ffi.so are all newer than the PNGs).",
   "fix": "Rebuild the facade from the final core, then re-record with EXPONENTIAL_UI_RECORD=1. Review the diffs before committing, and re-run the whole suite after the last core/theme lane lands."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/NodeView.kt",
   "line": 367,
   "severity": "major",
   "summary": "The Resizable handle drag computes `delta` from `c.position - start` in the HANDLE's local coordinates. The handle moves with the split after every `move` pass, so the computed travel shrinks by however far the handle has already moved.",
   "failure": "Drag a handle 40dp right. The first move resizes and the handle is re-laid out 40dp to the right. The next pointer event at the same screen x now reports local x \u2248 start.x, so delta \u2248 0, `move 0` snaps the panels back, and the handle oscillates or lags behind the finger. Round2InteractionTest only calls model.resizeDrag() directly, so the gesture path is untested.",
   "fix": "Accumulate `positionChange()` across events (or use window/root coordinates such as `positionInRoot` captured at down) so that delta = the pointer's travel since the drag started, independent of where the handle is now. Add a Compose UI test that drags through `performTouchInput`."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/ChartLeaf.kt",
   "line": 221,
   "severity": "major",
   "summary": "Chart value labels (221), the tooltip values (284/287), the chart/sparkline accessible summary (Semantics.kt:296-303) and the Slider stateDescription all format with the locale-free `ChartModel.formatValue`, not the surface formatter. Round 2 \u00a73 requires chart tick and value labels to go through the Formatter, and React does this (chart.tsx `fmt = ctx.formatter.number`).",
   "failure": "Same fixture under locale de-DE with value 1234.5: React shows '1.234,5' in the value label, tooltip and aria summary, while Compose shows '1234.5'. With locale en-US, Compose still shows no grouping ('1234.5' instead of '1,234.5'). The tick labels are fine only because the core pre-formats `chart.tickLabels`.",
   "fix": "Route every chart and slider number through `model.formatter` (fall back to `displayString`), and build the summary params the way `chartSummaryParams` does."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/model/SurfaceInteraction.kt",
   "line": 147,
   "severity": "major",
   "summary": "`scrollTo` sets nodesDirty and runs a full `pass()` (core layout + readNodes over every node + visuals) whenever the core's `scroll_to` returns true. The core returns true for ANY offset change of \u22650.5dp, including plain overflow containers where the core says 'a plain one only moves paint'.",
   "failure": "Fling an `overflow: auto` Box in the kitchen sink. The snapshotFlow emits every frame, each emit runs a full surface pass plus a full FFI node read-back, and the `scrolls` state change recomposes the container. The lane's own bench measures 14 ms per step on a 32-core JVM, which is over a 16 ms frame budget on a device. ExponentialSurface.kt:169 `autoViewportHeight(visible.height)` adds the same per-frame full pass whenever the surface is partly off-screen in a host scroller, because the visible height changes on every frame of host scroll.",
   "fix": "For non-windowed containers without sticky children, only translate paint (keep the offset locally and skip pass/readNodes). Read back only changed nodes (FfiDelta/nodesAt). Debounce or quantize autoViewportHeight, or only feed it when windowed host-scrolled lists or viewport layers exist."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/test/kotlin/at/exponential/ui/SnapshotTest.kt",
   "line": 104,
   "severity": "major",
   "summary": "The cross-painter Swift\u2194Compose snapshot parity check was weakened from `assertEquals(swift.keys, ours.keys)` to `assumeTrue(...)`. Any case-set mismatch now silently SKIPS the whole comparison instead of failing.",
   "failure": "If a catalog change adds or renames a component case and the Swift snapshot isn't re-recorded, the drift check reports 'skipped' indefinitely (it is already the 1 skipped test). Real paint drift between the two native painters then goes unnoticed, which turns a ratchet into a no-op.",
   "fix": "Keep the hard assert, or compare the intersection of keys and fail when there is drift on shared keys. File an issue for the iOS re-record instead of disabling the gate."
  },
  {
   "file": "packages/exponential-ui-compose/README.md",
   "line": 105,
   "severity": "minor",
   "summary": "The README contradicts the code: it says 'Widths are ceiled to a whole DEVICE pixel' (and the lane report says the same), but TextShaper.lineWidth now returns the FRACTIONAL extent (MeasurerRulesTest asserts it is NOT ceiled), and TEXT_SLACK_PX covers the rounding. The measurement list also still says 'a `lines: 1` text shrinks to 0 at min-content', while the code now uses min-content = the whole line (nowrap, matching gpui). Line 336 also mentions the 'measurer takes the ceiling'.",
   "failure": "Integrators or other lanes reading the README get the opposite measurement rules from the ones that ship. The Swift measurer still uses the 0-rule, and the README hides that this is now a drift.",
   "fix": "Update README \u00a7Text, \u00a7Measurement and the divergences to the fractional-width + TEXT_SLACK and nowrap-min-content rules. Note the Swift drift for the iOS lane."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/test/kotlin/at/exponential/ui/Round2Suites.kt",
   "severity": "minor",
   "summary": "Most of the new '1609 conformance cases' only replay facade functions or test-side Kotlin ports (bind, style-conditions, code-tokens, template-items `instances`, resizable, virtual-list, the `keyframesCss` port and the direction walk). They never exercise painter code, so the count overstates painter coverage.",
   "failure": "A Compose painter regression in windowing, sticky, resize or animation painting would still report CONFORMANT, because these suites assert the Rust core (already covered by tests/support/round2.rs) or the test's own port.",
   "fix": "State this in the README/report. Where possible, drive these suites through the model, e.g. virtual-list windows through SurfaceModel.scrollTo, or resizable through resizeDrag/handleKey and then the frames."
  },
  {
   "file": "packages/exponential-ui-compose/example/src/androidTest/kotlin/at/exponential/ui/kitchensink/KeyboardTest.kt",
   "severity": "minor",
   "summary": "The 'Escape closes the top layer' part calls `model.escape()` directly instead of sending KEYCODE_ESCAPE, so the hardware-key path (surfaceKeys \u2192 handleKey) is not covered on a device. The Tab focus is also seeded with `focusRequest`, not by tabbing.",
   "failure": "If the root onPreviewKeyEvent stops receiving Escape (e.g. focus outside the surface, or a native dialog window), the test still passes.",
   "fix": "Send `instrumentation.sendKeyDownUpSync(KEYCODE_ESCAPE)` and assert the layer closes."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/ExponentialSurface.kt",
   "line": 131,
   "severity": "minor",
   "summary": "The assertive announcement sends a second, empty `TYPE_ANNOUNCEMENT` via `view.sendAccessibilityEvent` after `announceForAccessibility(text)`.",
   "failure": "An `alert`/error toast announces its text, then dispatches a text-less announcement event. TalkBack either ignores it or re-reads the root view's content description, so assertive gains no interrupt semantics.",
   "fix": "Build one AccessibilityEvent(TYPE_ANNOUNCEMENT) with the text, or use a live-region node with LiveRegionMode.Assertive for assertive announcements."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/Semantics.kt",
   "line": 303,
   "severity": "minor",
   "summary": "Hard-coded English punctuation in the a11y strings: the chart summary joins series names with ', ', and field errors are joined with '. ' (fieldErrorSemantics).",
   "failure": "Under ar/ja locales TalkBack reads Latin list separators. The web uses the summary params and Intl list formatting.",
   "fix": "Use android.icu ListFormatter in the surface locale, or a string from catalog/strings.json."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/test/kotlin/at/exponential/ui/RealFontConformanceTest.kt",
   "severity": "minor",
   "summary": "The ratchet only prints 'IMPROVED \u2014 lower the budget' and never fails, so a fixed divergence can come back up to the old budget unnoticed. onlyRef (174 web-placed nodes Compose doesn't emit, e.g. the Accordion count part) is budgeted, not compared.",
   "failure": "A fix lands, nobody rewrites conformance-known.json, and a later regression to the old count passes.",
   "fix": "Fail on an improvement unless EXP_UI_WRITE_FIXTURES is set (as the TS gate does), or tighten automatically. Track the onlyRef coverage gap explicitly."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/ChartLeaf.kt",
   "line": 98,
   "severity": "minor",
   "summary": "Compose Chart diverges from React in three ways: cartesian kinds don't mirror x under RTL (React uses `mx`), the chart isn't focusable and arrows don't move the tooltip (React and gpui do), and null values are drawn as 0 instead of gaps (`values.map { it.number ?: 0.0 }`, where React skips non-finite points).",
   "failure": "An rtl dashboard reads left-to-right on Android, right-to-left on the web. Keyboard users can't reach chart values on Android. A line with a missing point dips to 0 instead of breaking.",
   "fix": "Mirror xOf under rtl for bar/line/area, add focusable + arrow handling that sets `hovered`, and keep NaN for missing values and skip them when drawing."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/model/SurfaceInteraction.kt",
   "severity": "minor",
   "summary": "Toast pause/resume restarts the FULL duration on release (syncToasts relaunches with t.durationMs). toastHeld is never pruned for gone toasts. Under a mouse, a press-release while still hovering resumes the timer.",
   "failure": "A 4s toast hovered at 3.9s gets a new 4s after the pointer leaves. The test advances 4100 ms after release, so it can't tell remaining-time from full-time.",
   "fix": "Track the remaining ms at hold and resume with that. Remove gone ids from toastHeld. Count hover and press holds separately."
  },
  {
   "file": "packages/exponential-ui-compose/ui-compose/src/main/kotlin/at/exponential/ui/compose/NodeView.kt",
   "severity": "minor",
   "summary": "A node with `visibility: hidden` paints at alpha 0 and is hidden from TalkBack, but stays pressable and focusable: the `pressable`/`isFocusable` branches only check pointerEventsNone.",
   "failure": "An invisible button still eats taps and takes a Tab stop. CSS visibility:hidden removes both.",
   "fix": "Treat `base.visibilityHidden` like pointerEventsNone when building `pressable` and the focusable branches."
  }
 ],
 "missing_from_scope": [
  "Chart numbers through the surface Formatter (round 2 \u00a73: chart value labels/tooltip/summary). Neither done nor deferred; only the core-preformatted ticks are localized.",
  "Chart RTL mirroring and chart keyboard (focus + arrows moving the tooltip), which React/gpui have. Not mentioned in Done or Deferred.",
  "Re-recording the Roborazzi image snapshots against the final core/themes. The suite is red at the current tree.",
  "A gesture-level test for the Resizable handle drag (only the model call is tested), and an Escape key-event test on device.",
  "The generator fix itself (escape `$` in scripts/generate.ts styleMediaPattern) is only reported as a contract note. It needs an owner/issue so CatalogConstants can be dropped in favour of the generated catalog.",
  "Following up on the Swift snapshot re-record and the iOS AccessibilityOrderTest update. Reported, but the parity gate was disabled (assumeTrue) instead of filing an issue.",
  "README corrections for the now-fractional text widths and the nowrap min-content rule (the README still documents the old rules)."
 ]
}


## review:core (Review)

{
 "findings": [
  {
   "file": "apps/desktop/crates/exponential-ui/src/surface/events.rs",
   "line": 948,
   "severity": "major",
   "summary": "Resizable `key` and drag `end` always write `sizes` back and fire `change`, even when nothing changed. That includes unrecognised keys and a click with no movement. React commits only when the sizes differ and ignores keys outside RESIZE_KEYS.",
   "failure": "Confirmed with a scratch crate against the lane's code. Pressing Tab on `split.handle.0` returns `[DataChanged{/ui/split:[30,70]}, Relayout]`, and so does a drag start\u2192end with delta 0. Any host that forwards every key on a focused handle (gpui/Compose) writes the data model, fires `change {sizes}` and forces a relayout on Tab, Shift and similar keys. The same fixture on React emits nothing. Arrow keys at a limit also fire `change`.",
   "fix": "In `resize_event`, return early (no write, no fire, no Relayout) when `next == sizes`, or `== start` for the drag end. For `key`, also ignore keys outside ArrowLeft/Right/Up/Down/Home/End/Enter, the way React's RESIZE_KEYS does. Add a test for both."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/layout_tree/misc.rs",
   "line": 107,
   "severity": "major",
   "summary": "Chart `tickLabels` are formatted with fixed decimals taken from the tick step (`decimals: Some(decimals_of(step))`). React calls `ctx.formatter.number(v)` with the default 0..3 digits, so the same fixture shows different text.",
   "failure": "Confirmed: values [0.1, 0.35, 0.9] give step 0.2 and `tickLabels` [\"0.0\",\"0.2\",\u2026,\"1.0\"]; React shows \"0\",\"0.2\",\u2026,\"1\". Tick label widths also feed the plot's left margin, so frames drift too. A step that `js_number_string` writes in exponent form (1e-7, 2.5e-7) yields 0 or 4 decimals, because `decimals_of` counts characters after '.' in \"2.5e-7\".",
   "fix": "Use `formatter.number(t, NumberOptions::default())` to match React, or settle the rule in the contract and change both renderers. Do not derive decimals from the `js_number_string` text."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/layout_tree/table.rs",
   "line": 200,
   "severity": "major",
   "summary": "Table `cell_text` differs from React on the same data in two cases: a `currency` column without `column.currency`, and an unreadable `date`/`relativeTime` value.",
   "failure": "Confirmed: a currency column with no `currency` and value 12.5 shows \"12.5\" in Rust; React does `str(c.currency, 'USD')` and shows \"$12.50\". A date cell holding \"next week\" shows \"next week\" in Rust (the display_string fallback); React's `formatter.date` returns \"\". Rust also formats numeric strings (\"1234.5\") with display_string, while React coerces them with `num(v)`, giving \"1234.5\" vs \"1,234.5\".",
   "fix": "Align with the contract and React. Default the currency to USD, or update the catalog/contract and React together. Return \"\" for unreadable dates and relative times. Coerce numeric strings like `num()` for number, currency and percent. Add the cases to a shared fixture (e.g. format.json or bind-time.json) so all four renderers replay them."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/layout_tree/containers.rs",
   "line": 231,
   "severity": "major",
   "summary": "In a `divided` + `sectionBy` List the window offsets assume `gap + hairline` between every row, but dividers are only inserted between consecutive items. Header\u2194item boundaries are laid out with only `gap/2`. Divider part ids also use the flat ROW position instead of the item index.",
   "failure": "Confirmed: 120 items in 12 sections with gap md and divided give a window `content_height` of 6776 against a scroll content of 6755. The gap from row.i9 to section.1 is 6 px, while the offsets assume 13. The spacers, `virtual_window`, `sticky_header` pin offsets and `scrollToIndex` targets drift by up to about 14 px per header. Divider ids come out as `l.divider.2`, `.3`, \u2026, `.13` (row positions); contract \u00a76 says `divider.1` = before item 1, so the conformance parts dump diverges from React.",
   "fix": "Either compute the offsets with per-boundary gaps (gap_eff only between Item\u2192Item), or put a divider or equivalent spacer at every boundary. Suffix dividers with the item's data index. Add a sectioned + divided case to virtual-list coverage."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/surface/events.rs",
   "line": 425,
   "severity": "major",
   "summary": "NumberField text now goes through the host formatter, but `change`/`commit` still parse with ASCII `str::parse::<f64>`. With a non-English HostFormatter, the displayed text does not parse back.",
   "failure": "Plausible: with a de-DE HostFormatter the field shows \"2,50\" (and \"1.234,50\" for larger values). A host that commits the field's text sends `{value: \"2,50\"}`, `parse` fails, and the core writes `null` to the bound value. Grouped English text (\"1,234.50\") was already affected; localizing the display makes it hit every decimal value.",
   "fix": "Parse with the formatter's locale symbols, e.g. add `Formatter::parse_number`, or have hosts send a number. At least strip the group separators and map the decimal separator from `formatter.number(1234.5)`. Add a surface test with a comma-decimal formatter."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/surface/pass.rs",
   "line": 565,
   "severity": "minor",
   "summary": "`position: sticky` against the host viewport never pins on an auto-height surface, and the `inset` shorthand is ignored.",
   "failure": "Confirmed: a sticky `top: 0` bar on a surface with `set_viewport(390, 0)` gets `set_surface_scroll(0, 500)` \u2192 true, but `out.sticky` is empty. The fallback port has h = 0 and the `pf.h > 0.0` gate covers the `top` case too. This is exactly the host-scrolled-surface case `set_surface_scroll` is meant for. `style: {position: sticky, inset: 0}` on a bounded host also gives no pin, because only top/bottom/left/right are read.",
   "fix": "Gate only the `bottom` and `right` branches on the viewport extent. Read `inset`, and `insetHorizontal`/`insetVertical` if they are valid keys, as fallbacks for the physical sides."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-ffi/src/lib.rs",
   "line": 1466,
   "severity": "minor",
   "summary": "`listJson` `sticky` always builds the offsets with gap 0, indexes the arrays with caller-supplied `headerRows` unchecked, and assumes they are sorted. `resizableJson` `drag` hard-codes a handle extent of 1.0 instead of taking the theme hairline.",
   "failure": "A native list with a gap or dividers gets wrong pinned offsets from `listJson({op: sticky})`. An out-of-range header row panics inside the FFI (uniffi turns it into an internal exception). `drag` previews are off whenever `$control.hairline` \u2260 1.",
   "fix": "Accept `gap` (or `rowOffsets`) in the `sticky` op, and validate or sort `headerRows`, returning UiError when invalid. Accept a `handleExtent` in `drag`, as `extents` already does."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-ffi/src/lib.rs",
   "line": 1322,
   "severity": "minor",
   "summary": "`bindRowSlotJson` and `bindSectionHeaderJson` take no formatter, so format calls inside row slots and section headers bound by a native always use the English fallback. `bindSectionHeaderJson` also ignores `options.now`.",
   "failure": "A Compose or Swift Table slot cell with `formatCurrency`/`formatDate`, bound through the facade, shows en-US text under a de-DE surface. The surface path and React use the locale formatter.",
   "fix": "Add an optional `HostFormatter` parameter, or a `Surface`-scoped variant that uses the surface's formatter, and apply `now` in both helpers."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/layout_tree/containers.rs",
   "line": 252,
   "severity": "minor",
   "summary": "Lists with `stickyHeaders` recompute every item's extent and offset (O(n) HashMap lookups and allocations) twice per pass: in `pinned_header` during the build and in `sticky_offsets` in `output`. The `offsets_cached` result is not reused.",
   "failure": "A 100k-row sectioned sticky list pays about 2 \u00d7 O(n) per scroll step on top of the 3.2 ms the report already attributes to O(n) offset recomputation. The bench does not cover sections, so this is unmeasured.",
   "fix": "Use `window.offsets_cached(keys)` in both places and keep extents implicit (offsets[i+1] \u2212 offsets[i] \u2212 gap). Binary-search the headers."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/surface/mod.rs",
   "line": 365,
   "severity": "minor",
   "summary": "`SurfaceSettings.time_zone` / `FfiSettings.timeZone` is stored and returned but never read, and the facade test only checks that it round-trips.",
   "failure": "A host that sets `timeZone` and keeps the default formatter sees no change: the English fallback always formats in UTC. The setting looks functional but does nothing.",
   "fix": "Document it as host-informational in the FFI docs, or have `set_settings` rebuild the formatter (a zone-aware English fallback, or a host callback). Replace the round-trip assertion with a behavioural test, or drop it."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/layout_tree/fields.rs",
   "line": 237,
   "severity": "minor",
   "summary": "`format_bytes_with` routes the number through the host formatter but hard-codes the English unit names \"byte\"/\"kB\"/\"MB\"/\"GB\". React localizes them with Intl `style: unit`.",
   "failure": "With a de-DE HostFormatter, FileUpload shows \"47,1 kB\" / \"500 byte\", where React shows the localized unit (e.g. \"500 Byte\"). This hard-coded English is not listed under Deferred.",
   "fix": "Add a `bytes(value, unit)` method to the Formatter (and HostFormatter), or string ids for the units, or list it explicitly as deferred."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-ffi/tests/facade.rs",
   "line": 745,
   "severity": "minor",
   "summary": "The test named `scroll_to_index_resizable_and_sticky_cross_the_facade` makes no assertion about sticky: neither `FfiLayout.sticky` nor `FfiVisual.sticky` is checked.",
   "failure": "A regression in the FfiSticky conversion or in `FfiVisual.sticky` would pass this test.",
   "fix": "Lay out a sticky node through the facade, scroll it, and assert `layout.sticky` contains its index and dy, and that `visual.sticky == true`."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/surface/restyle.rs",
   "line": 298,
   "severity": "minor",
   "summary": "The core crate contains changes the lane report does not mention, which look like they came from the gpui lane: `round2_parity.rs` and `carousel_controls.rs` (untracked), the engine.rs aspect-ratio transfer, and restyle's type inheritance, Slider margins, CodeBlock gutter and Table cell basis floor. They leave dead code behind (`let _ = \u2026font_relative;`), and the doc comment of `lift_templates` sits on the `BuildMissing` type alias (reducer.rs:79).",
   "failure": "Reviewers cannot tell which lane owns these layout-affecting changes. `font_relative` is now unused state, and `cargo doc` attaches the `lift_templates` description to the type alias.",
   "fix": "Have the owning lane list these changes in its report (or move them into it). Remove `font_relative` and the dead read. Move the doc comment onto `fn lift_templates`."
  }
 ],
 "missing_from_scope": [
  "Chart VALUE labels through the formatter (contract \u00a73 'chart tick labels and value labels'): only `tickLabels` is done, and value labels are neither implemented nor deferred.",
  "No signal for when to call `tick`. Contract \u00a73 says a node using `formatRelativeTime` without `now`, and Table relativeTime cells, re-bind at least once a minute, but the core exposes no flag or OutEvent telling the host whether a relative time is on screen, so hosts must tick unconditionally.",
  "The Swift package copy (packages/exponential-ui-swift/Sources/ExponentialUICore/ExponentialUIFFI.swift) differs from the regenerated bindings. Building the Swift package against the new library will fail uniffi's API checksum. This is only mentioned under 'Tests run', not handed off under Deferred.",
  "Contract \u00a75 says an unmeasured item takes `$control.row`; the lane implements 'mean measured extent' instead. It is flagged as a proposal, but neither `round-2-contract.md` nor React was updated, so React and Rust will report different content extents for partly measured lists.",
  "No shared fixture covers the surface-side formatting rules (Table currency default, unreadable date cells, chart tick decimals, sectioned + divided offsets). These are exactly where Rust and React diverge today.",
  "Table window threshold still comes from `chart::WINDOW_THRESHOLD` (hard-coded 50) rather than `layout.json`/`crate::layout::WINDOW_THRESHOLD`.",
  "Calendar day-cell accessible labels: React uses a localized full date per day cell, but the Rust calendar's day labels were not routed through the formatter, and this is not listed as deferred."
 ],
 "verdict": "Mostly sound, with 5 major issues (4 confirmed, 1 plausible) and 8 minor ones; none is a blocker. I re-ran the lane's checks:\n- `cargo test -p exponential-ui -p exponential-ui-ffi`: 248 passed. That is the claimed 238 plus 10 from two untracked test files the lane did not report (`round2_parity.rs`, `carousel_controls.rs`).\n- clippy `-D warnings`: clean.\n- `conformance:check` on the core's report: CONFORMANT, 1613 cases in 24 suites. The report claimed 1609, which matches the manifest before it was updated.\n- The committed Swift and Kotlin bindings match a fresh uniffi regeneration exactly.\n- The bench gives first paint 59.6 ms, scroll step 2.74 ms and scrollToIndex 4.34 ms, in line with the README.\n- The generated catalog files pass the drift test.\n\nThe templates lift, the keys, `number_to_string`, the format arithmetic, the resizable functions and the virtual-window functions match the TS reference on review.\n\nProblems the fixtures do not cover:\n- **Resizable** writes `sizes` back and fires `change` on every key (Tab too) and on a click with no movement; React commits only when the sizes change.\n- **Chart tick labels** use fixed step decimals (\"0.0\"/\"1.0\"); React shows \"0\"/\"1\".\n- **Table cells**: a currency column without a code shows a plain number where React defaults to USD, and an unreadable date shows the raw value where React shows nothing.\n- **Divided + sectioned Lists** have window offsets that drift from the actual layout (21 px over 12 sections in my test), and their divider ids use row positions instead of item indices.\n- **NumberField** with a non-English host formatter: the displayed text no longer parses back, so a commit can write null (plausible, not reproduced).\n\nMinor:\n- Sticky against the host viewport does nothing on an auto-height surface, and the `inset` shorthand is ignored.\n- Two FFI list/resizable helpers hard-code a zero gap or a 1 px handle.\n- The row-slot and section-header bind helpers take no formatter.\n- `timeZone` is stored but never read.\n- Byte-size units are hard-coded English.\n- One facade test is named for sticky but checks nothing about it.\n- Sticky sectioned lists recompute all offsets twice per pass.\n\nThe core crate also contains layout changes the report does not describe (type inheritance, the aspect-ratio transfer, Slider/CodeBlock/Table tweaks), apparently from the gpui lane.\n\nNothing was edited outside the lane's paths: the `Cargo.lock` change belongs to the gpui crate, and the conformance report and `out/` folders are gitignored. The generated files were not hand-edited."
}


## review:contract (Review)

{
 "verdict": "The lane's claims mostly hold, but there are 4 major contract defects to fix before the renderer lanes build on this contract.\n\n**What I verified**\n- `bun run --filter @exponential-at/ui test`: 320 pass, 0 fail. Typecheck exits 0.\n- The generate drift gate (`generate.test.ts`, which re-renders all 40 outputs and byte-compares them) is green. No generated file was hand-edited.\n- The prompt budget was not raised (6475 of 6500).\n- No tolerance was loosened.\n- The English fallback really does equal Intl en-US. I ran my own fuzz of 20k random number and percent values plus edge cases (rounding ties, `-0`, 1e21, many currencies, relative-time boundaries, every date style and pattern, years below 1000) and got 0 differences for finite inputs.\n\n**Report figures that are out of date or wrong**\n- React and Rust have since landed in this worktree. The React suite now passes 1196 of 1197 (1 skipped) and `cargo test -p exponential-ui --lib` passes 81 of 81. The report's 21 and 6 failures no longer apply.\n- `conformance-baseline.json` and `conformance-known.json` were rewritten after the lane finished. Every ratchet case now reads size 0 / position 0, and `causes` and `rules` were kept. That rewrite comes from a later lane, not this one.\n- The manifest has 1613 cases, not the 1609 the report states.\n\n**Major defects**\n1. **Animation opacity differs between web and the contract.** On the web, `animation` replaces the node's own opacity instead of multiplying it. Because of `both` fill, a fade-in node loses its author opacity for good.\n2. **A template that names its own node or an ancestor breaks the tree silently.** The node is lifted out, the tree is left empty, the template becomes self-recursive and no issue is reported.\n3. **Dragging a collapsed panel open sticks at its min.** Because drags are computed from the start sizes, the panel stays at its min for the whole drag. The fixture locks this in.\n4. **The English fallback formatter ignores `timeZone`.** It always formats times in UTC, and it is the default on desktop, so desktop users outside UTC see wrong times.\n\nThe remaining findings are minor: fixture coverage, packaging, id collisions, hard-coded English, performance, and docs gaps.",
 "findings": [
  {
   "file": "packages/exponential-ui/src/animation.ts",
   "line": 156,
   "severity": "major",
   "summary": "The web output breaks the opacity rule: style.json says an animation's `opacity` multiplies the node's opacity, but `keyframesCss` and `animationCss` emit plain `opacity` keyframes with `both` fill, so CSS replaces the node's opacity instead.",
   "failure": "Box style `{opacity: 0.5, animation: 'pulse'}`: on web it pulses 1 \u2192 0.5 \u2192 1, while gpui and the natives (multiply, per the contract) pulse 0.5 \u2192 0.25 \u2192 0.5. Box `{opacity: 0.6, animation: 'fade-in'}`: web ends at opacity 1 and keeps it forever (fill `both` holds the last keyframe), so the author's 0.6 is lost. The same fixture gives different pixels on each platform. styleToCss (css.ts:75) emits the node's `opacity` and the `animation` on the same element.",
   "fix": "Animate a registered custom property (`@property --xui-a-opacity`) and set `opacity: calc(<own> * var(--xui-a-opacity))`, or put the animation on a wrapper. Alternatively change the contract to 'replaces' and update the gpui/native checklists. Add an animations.json case with a non-1 base opacity."
  },
  {
   "file": "packages/exponential-ui/src/reducer.ts",
   "line": 66,
   "severity": "major",
   "summary": "`liftTemplates` has no cycle check. A `template.component` that names the template's own node or an ancestor is lifted out of the tree with no issue, leaving the root empty and the template recursive. A template naming the root is skipped silently.",
   "failure": "Reproduced: flat `[root Stack \u2192 col Stack \u2192 l List{children:{path:'/items', componentId:'col'}}]` reduces to issues `[]`, an empty root and `templates: {col}`, where `col` contains `l`, which instantiates `col` again. With an absolute path every renderer recurses with no end (hang or stack overflow). Self-reference (`l` naming `l`) behaves the same. Model output is untrusted, so this is a crash vector the flat `build` path guards against (`cycle through this id`) but the lift does not.",
   "fix": "Before lifting, report `template: cycle through this id` when the template id is the owner itself or an ancestor of the owning node (in the tree or inside the template's own subtree), and drop that template. Also report a template that names the root. Add both cases to template-items.json `reduce`."
  },
  {
   "file": "packages/exponential-ui/src/resizable.ts",
   "line": 89,
   "severity": "major",
   "summary": "Reopening a collapsed panel always sets it to exactly `min`, whatever the delta. Drags are computed from the start sizes, so a drag that begins collapsed can never take the panel past its min.",
   "failure": "`resizePanels([0,100], 0, 50, [{collapsible:true,min:20},{}])` returns `[20,80]` (the pointer is 50 points in). The same happens for the second panel: `[100,0]` with delta -50 returns `[80,20]`. The fixture `reopen-at-min` only tests a delta near half-min, so the bug is locked into resizable.json for every platform.",
   "fix": "Once past the half-min threshold, follow the pointer: `d = clamp(delta, la.min, hi)` (mirrored for b). Regenerate resizable.json with a large-delta reopen case."
  },
  {
   "file": "packages/exponential-ui/src/format.ts",
   "line": 308,
   "severity": "major",
   "summary": "The English fallback Formatter, which is the Rust core's default and what gpui/desktop uses without a host formatter, always formats instants in UTC and ignores `SurfaceSettings.timeZone`. The contract's own default is 'the platform's' zone.",
   "failure": "On desktop, a user in Europe/Berlin sees `formatDate({value:'2026-10-14T22:30:00Z', format:'HH:mm'})` as `22:30` instead of `00:30`. Table date cells, picker triggers and `time:true` presets show UTC as well. The setting is silently ignored.",
   "fix": "Let the fallback take a zone resolver (at least a fixed UTC offset per instant from the host, or the Rust core resolving the IANA zone via chrono-tz) and apply it in `utcFields`. Add format.json cases with a non-UTC `timeZone`."
  },
  {
   "file": "packages/exponential-ui/scripts/generate-round2.ts",
   "line": 3,
   "severity": "minor",
   "summary": "Every round-2 `expected` value is computed by the TS reference itself, so `round2.test.ts` only replays the reference against its own output. The only independent checks are the 'sum to 100' invariant and English == Intl.",
   "failure": "Named decisions (for example `resize/reopen-at-min` and `collapse-below-half-min`) are never asserted against hand-written values, which is how the reopen bug became the cross-platform contract. A future change regenerates the fixtures and still passes.",
   "fix": "Hand-write `expected` for the decision cases (resizable collapse/reopen, key rules, sticky handoff, `#<index>` keys), or assert them separately in round2.test.ts."
  },
  {
   "file": "packages/exponential-ui/release/prepare-npm.ts",
   "line": 68,
   "severity": "minor",
   "summary": "The new filter `!p.includes('/conformance/fonts')` also matches `conformance/fonts.json`, so the font map is dropped from the npm package even though the README lists it beside the harness.",
   "failure": "The published `@exponential-at/ui/conformance/` lacks fonts.json, so an outside runner following conformance/README.md cannot find the font set.",
   "fix": "Match the directory exactly: `!p.includes('/conformance/fonts/') && !p.endsWith('/conformance/fonts')`."
  },
  {
   "file": "packages/exponential-ui/src/list.ts",
   "line": 62,
   "severity": "minor",
   "summary": "Instance suffixes join raw keys with `.` and do not escape them, so dotted keys (emails, versions) collide across nesting levels.",
   "failure": "Outer key `a.b` with inner key `c`, and outer key `a` with inner key `b.c`, both give the painted id `issue.a.b.c`. The result is duplicate `data-xui-id`s, merged conformance nodes and duplicate a11y ids. It can also collide with `<id>.<part>` ids.",
   "fix": "Escape `.` (and the escape character) in keys before appending, as tableRowKeys does for `/`, and add a template-items case."
  },
  {
   "file": "packages/exponential-ui/src/format.ts",
   "line": 342,
   "severity": "minor",
   "summary": "Locale data is hard-coded in code instead of `locale.json` `englishFallback`, which the comment calls the data source: the ` at ` and `, ` joiners, the date/time presets (`ENGLISH_DATE_PATTERNS`, `h:mm a`) and the `${code}\u00a0` currency fallback (line 320).",
   "failure": "Rust's format.rs already hand-copies these patterns. Kotlin/Swift fallbacks must copy them again, and any edit drifts silently because only fixture cases catch it.",
   "fix": "Move the patterns, joiners and unknown-code template into `englishFallback` and have every target read them."
  },
  {
   "file": "packages/exponential-ui/src/list.ts",
   "line": 103,
   "severity": "minor",
   "summary": "`virtualWindow` and `scrollOffsetForIndex` rebuild the full offsets array and scan linearly on every call.",
   "failure": "With the 100,000-row bench, each scroll frame allocates a 100k-element array and scans it, and sticky headers add another O(H) pass. The bench measures this as reference behaviour that every port copies.",
   "fix": "Take cached prefix offsets (`itemOffsets` once per measure change) and binary-search the start and end. The signatures can accept offsets."
  },
  {
   "file": "packages/exponential-ui/src/conformance.ts",
   "line": 118,
   "severity": "minor",
   "summary": "The format suite tells runners to format 'through your Formatter at en-US'. Foundation (iOS 17) and Android ICU 72+ put U+202F before AM/PM, so native Formatters cannot pass. format.json also locks only `'42'` and `'abc'` for numeric strings, though the TS semantics are JS `Number()` (hex, whitespace).",
   "failure": "A Swift/Kotlin runner using its platform formatter fails every time case. A Kotlin port using `toDoubleOrNull` diverges on `' 12 '` and `'0x10'` with no fixture to catch it.",
   "fix": "Require the English fallback for the format suite, or normalise U+202F. Add numeric-string cases (`' 12 '`, `'0x10'`, `'1e3'`, `'Infinity'`)."
  },
  {
   "file": "packages/exponential-ui/catalog/core.catalog.json",
   "line": 437,
   "severity": "minor",
   "summary": "Image now defaults to 16:9 when neither `aspectRatio` nor a height is set, but \u00a710 (Breaking changes) does not list it.",
   "failure": "Existing surfaces with portrait or square images given only a width now render in a 16:9 box, cropped or letterboxed. Hosts are not warned.",
   "fix": "Add the change to round-2-contract.md \u00a710 and the lane report."
  },
  {
   "file": "packages/exponential-ui/docs/round-2-contract.md",
   "line": 346,
   "severity": "minor",
   "summary": "The docs disagree with themselves and with the report: React checklist item 4 says to replace 'the five hard-coded labels' while \u00a73 defines 8 ids, and the report states 1609 cases where the manifest and doc say 1613.",
   "failure": "A renderer lane following the checklist may stop after five labels.",
   "fix": "Say 'the eight ids of \u00a73' and correct the report's case count."
  }
 ],
 "missing_from_scope": [
  "The host API contract (`catalog/host.json`, mirrored by TS `ExponentialHost` and Rust `host`) was not updated for the new host-supplied Formatter, `SurfaceSettings.timeZone` or the `scrollToIndex` command. They live only in `types.ts` and `a11y.json`, and host.json stays at version 1.",
  "The template contract (\u00a74) does not define cycle handling: a template naming its own node, an ancestor or the root.",
  "\u00a73's hard-coded label inventory leaves out SwiftUI: ExponentialUI `NodeInfo.swift` / `NodeView.swift` 'Loading', and `ControlLeaves.swift` / `SurfaceMeasurer.swift` 'Message'.",
  "No rule for how `scrollToIndex` maps a data index to a flat row index when `sectionBy` inserts header rows, and no fixture case for it.",
  "No RTL rule for shimmer band direction or `slide-in-left/right`: they are physical names, and whether they mirror under an inner `direction: rtl` is not stated."
 ]
}


## review:react (Review)

{
 "findings": [
  {
   "file": "packages/exponential-ui-react/src/theme-css.ts",
   "line": 136,
   "severity": "major",
   "summary": "The 3 px fix makes `declarations()` start with `border-width:0` whenever a style sets only per-side widths. That runs in every layer: node styles, state blocks and media blocks, not just the part element that had the bug.",
   "failure": "A node style `{borderBottomWidth: 2}` on a Card whose recipe has `borderWidth: 1` now compiles to `border-width:0; border-bottom-width:2px` in the xui-node layer, which beats the recipe layer, so the top, left and right borders disappear. gpui's `border_sides` reads the merged recipe and node map and gives [1,1,2,1]. Same fixture, different result. A recipe state rule such as `:focus {borderBottomWidth: 2}` over a base `borderWidth` also loses the other sides on focus. round1.test.tsx was changed to expect the new `border-width:0` output.",
   "fix": "Remove the unshift. Reset part elements once in the base layer instead, e.g. `[data-xui-part]{border:0 solid transparent}` inside `@layer xui-base` (`.xui-el` already has this). The UA `medium` default is then gone and per-key overrides cascade."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/table.tsx",
   "line": 159,
   "severity": "major",
   "summary": "A `currency` cell calls `ctx.formatter.currency(num(v), str(c.currency, 'USD'))` without checking the code. Intl throws RangeError on a malformed code, and the renderer has no error boundary.",
   "failure": "An agent-written column `{type: \"currency\", currency: \"euro\"}` (or `\"us\"`, `\"$\"`) throws `RangeError: Invalid currency code` during render and unmounts the whole surface and its host subtree. The core `formatCurrency` guards the code with `/^[A-Za-z]{3}$/` and returns ``. A non-numeric value also prints `$0.00`, because `num()` falls back to 0, where formatCurrency returns ``. That is drift from the bind table.",
   "fix": "Use the same guard as `formatFunctions`: a valid 3-letter code, otherwise an empty cell (or a plain number), and a finite value, otherwise an empty cell. Wrap the painter with an error boundary so one bad node cannot take down the surface."
  },
  {
   "file": "packages/exponential-ui-react/browser/suites.test.ts",
   "line": 290,
   "severity": "major",
   "summary": "The new manifest-v2 suites (bind, style-conditions, format display, template-items, text-direction, resizable, virtual-list, animations) run the core `@exponential-at/ui` reference functions, not the React renderer. So 'conformant on manifest v2' tells us nothing about React.",
   "failure": "`bind` checks the CORE `bindTree` and `runAction`, but React paints with its own `bindTree` in src/data.ts. `text-direction` checks the core `nodeDirections`, but React uses `ownDirection` and `DirectedNode`, which ignore a `direction` inside an `@media` block that the core resolves. `style-conditions` checks `resolveConditions`, but React compiles CSS in box-css.ts. `resizable` and `virtual-list` never mount ResizableNative or WindowedList. A React-only regression in any of these would still report conformant.",
   "fix": "Point each suite at the renderer: React's exported `bindTree` and `resolveValue`, and fixtures rendered through `<ExponentialSurface>` in the harness (dir attributes, computed text-align, drag and key replay on the painted handles, window ranges read from `data-window-start/end`). Keep the core replays only as a sanity check."
  },
  {
   "file": "packages/exponential-ui-react/src/host-surface.tsx",
   "line": 67,
   "severity": "major",
   "summary": "Perf cliff on the production HostSurface path. `templatesOf()` runs `reduceSurface` over the whole component list on every store version bump, and `SurfaceStore.setData` bumps the version, so this happens on every keystroke in a bound field.",
   "failure": "In a host surface that uses a data template (e.g. the app's Devices package), each keystroke in a bound Input runs a full reduce. It also returns a new `templates` object, so `templateRoots`, `compileNodeSheet` (the whole node CSS sheet), `templateNode` and the context value are rebuilt, and every template-item `node` identity changes, which defeats the ListNative and TemplateItemView memos.",
   "fix": "Cache the reduced templates per `store.components` array identity (WeakMap keyed on the array, which only changes in setComponents), or add `get templates()` to SurfaceStore now; it already memoises `reduced`."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/layout.tsx",
   "line": 131,
   "severity": "major",
   "summary": "A divided List spaces its items differently depending on whether it windows. In the flat path the divider is its own flex child, so the root `gap` applies on both sides of it. The windowed path uses gap + hairline as contract \u00a75 says.",
   "failure": "With `divided: true, gap: md`, a list of up to 50 items gives 2\u00d7gap + hairline between items. At 51 items it windows and gives gap + hairline, so spacing jumps when the count crosses WINDOW_THRESHOLD. The flat path also breaks \u00a75 ('item i starts at \u03a3(extent + gap), a divided list adds the hairline to the gap, the divider centred in it'). The new baseline records the flat geometry, so gpui is being ratcheted toward the non-contract spacing.",
   "fix": "In the flat path, place the divider with no flex gap of its own: make it absolutely positioned inside the following row wrapper, centred in the gap as WindowedList does, or use gap/2 margins with no root gap between a divider and its row. Then regenerate the baseline."
  },
  {
   "file": "packages/exponential-ui/builder/preview.tsx",
   "line": 58,
   "severity": "minor",
   "summary": "The in-tree template fallback and the `templateNodeFrom` export were removed with no fallback and no dev warning. Every `root=`-only consumer now silently renders 0 template items. The builder is one; it still passes only `root`.",
   "failure": "The builder's kitchen-sink and specimen previews render empty lists. Outside SDK users who pass `root` from `reduceNested` get the same silent result, and dropping the `templateNodeFrom` export breaks the published API. The lane only mentioned this in its Contract notes; it is neither fixed nor filed.",
   "fix": "Pass `templates` in preview.tsx (both call sites). In dev, warn when a node has a `template.component` that is missing from `templates`. Record the API removal in the SDK release notes."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/table.tsx",
   "line": 164,
   "severity": "minor",
   "summary": "Table `relativeTime` cells never refresh. `usesLiveClock` only looks for `formatRelativeTime` calls, so a surface whose only relative times are in a Table column never starts the minute timer.",
   "failure": "A Table column with `{type: \"relativeTime\"}` keeps showing '1 minute ago' indefinitely, while a Text bound to `formatRelativeTime` in the same surface updates every minute.",
   "fix": "Make `usesLiveClock` also return true for Table nodes that have a `relativeTime` column (walk `props.columns`)."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/table.tsx",
   "line": 157,
   "severity": "minor",
   "summary": "Table cells used to share memoised `numberFmt`/`dateFmt`. They now go through `intlFormatter`, which builds a new Intl.NumberFormat or DateTimeFormat on every call. Separately, in data.ts `formatCall` rebuilds the `formatFunctions` table on every call whenever `ctx.now` is set, and the surface always sets it.",
   "failure": "Measured in Node: 18 \u00b5s to construct and format a currency vs 0.3 \u00b5s with a cached formatter; zoned dates about 33 \u00b5s. WindowedList re-renders every visible row on each scroll step, so a 30-row \u00d7 10-column numeric table adds roughly 5\u201310 ms per step. The 100k bench only measured List, so this was not caught.",
   "fix": "Cache Intl instances inside `intlFormatter`, keyed by the options (this also fixes the calendar aria-label case the lane mentioned), or memoise per-column formatters in Table. In `formatCall`, cache the table per (formatter, now) pair."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/dates.tsx",
   "line": 229,
   "severity": "minor",
   "summary": "The DatePicker, DateRangePicker and TimePicker popover content is portalled to the surface layer without `dir`, so a node-level direction (new in round 2) does not reach it. Radix Popover has no dir prop; the other overlays set `dir={ctx.direction}`.",
   "failure": "A DatePicker inside a `direction: rtl` box in an ltr surface lays its calendar out ltr, but CalendarGrid reads `ctx.direction`, which is rtl. ArrowLeft therefore moves +1 day, which is visually to the right, and the `:dir(rtl)` chevron mirroring does not apply.",
   "fix": "Add `dir={ctx.direction}` to each `PopoverPrimitive.Content` in dates.tsx."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/resizable.tsx",
   "line": 56,
   "severity": "minor",
   "summary": "`onPointerEnd` commits the `live` value captured in the render closure. Pointermove updates have continuous priority, so a pointerup that arrives before the last move has re-rendered commits stale sizes and the panel snaps back. The drag test is also too weak: it is named 'commits once at the end' but only asserts that 0 events fire, and it never checks the bound write.",
   "failure": "A quick drag-and-release can commit the sizes from one move earlier, so the handle jumps back by the last delta. The test would still pass if commit were never called.",
   "fix": "Keep the latest sizes in a ref that onPointerMove updates and commit from that ref. Give the test an `on.change` and a bound `sizes` path, and assert one `change {sizes:[60,40]}` plus the data write."
  },
  {
   "file": "packages/exponential-ui-react/src/list.tsx",
   "line": 131,
   "severity": "minor",
   "summary": "WindowedList re-reads the viewport only on scroll and window resize, and picks its scroller once per count/axis change. A scroller that resizes on its own (the new Resizable panel) or only becomes scrollable after rows are measured is not tracked.",
   "failure": "A List in the lower panel of a vertical Resizable: dragging the handle up grows the panel, but `view.viewport` keeps the old height, so a blank band appears past the 5-row overscan until the user scrolls. A bounded list whose estimated total fitted at mount stays subscribed to `window` scroll after it starts scrolling internally.",
   "fix": "Observe the scroller (or the list element) with the existing ResizeObserver, re-run `read()` on resize, and re-resolve `scrollParent` when `total` changes."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/layout.tsx",
   "line": 166,
   "severity": "minor",
   "summary": "Section headers break the list's ARIA structure. A `role=heading` sits inside a `role=presentation` wrapper that is a direct child of `role=list`, and a list may only own listitems. Flat rows also put `role=listitem` and `aria-posinset` on `display:contents` wrappers, which some Safari versions drop from the accessibility tree.",
   "failure": "axe reports aria-required-children on every sectioned List. VoiceOver on older Safari loses the setsize/posinset that the lane listed as an a11y feature.",
   "fix": "Render each section as `role=group` with an aria-labelledby heading, or wrap the heading in a listitem. Use a real box (not `display:contents`) for the listitem wrapper, or put the role on the item's own root."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/layout.tsx",
   "line": 100,
   "severity": "minor",
   "summary": "In a non-windowed List or Table, `scrollToIndex` finds its target with a descendant `querySelector` (`[data-xui-row=\"r\"]`, `[role=row][data-key]`). A nested List or Table in an earlier row matches first.",
   "failure": "`run({scrollToIndex: {id: \"outer\", index: 5}})` scrolls to row 5 of a list nested inside outer row 0, not to outer row 5.",
   "fix": "Use `:scope > [data-xui-row=\"r\"]` (Table: `:scope > [role=row]`), or keep a ref map per row."
  },
  {
   "file": "packages/exponential-ui-react/src/natives/media.tsx",
   "line": 192,
   "severity": "minor",
   "summary": "Inactive carousel pages get `inert: true` as a boolean prop, but the package's peerDependencies still allow React 18, which does not support boolean `inert`.",
   "failure": "Under React 18, inactive pages log a non-boolean attribute warning and lose `inert`, so their focusable content stays reachable by Tab. The contract requires them to be inert.",
   "fix": "Under React 18 pass `inert=\"\"` (or set it on a ref), or raise the peer range to React 19."
  },
  {
   "file": "packages/exponential-ui-react/src/hardening.test.tsx",
   "line": 383,
   "severity": "minor",
   "summary": "The calendar tab-stop test's timeout was raised from 5 s to 30 s, which loosens a test limit. In isolation it takes 3.2 s; it only exceeds 5 s under parallel load.",
   "failure": "A large slowdown in CalendarGrid (now built on the Formatter) would stay hidden up to 30 s.",
   "fix": "Walk fewer months, or jump the view directly instead of clicking about 900 times, and keep the default timeout."
  }
 ],
 "missing_from_scope": [
  "packages/exponential-ui/builder/preview.tsx still passes only `root`, so the builder's kitchen sink renders 0 template items. The lane noted this but neither fixed nor filed it.",
  "`SurfaceStore.templates` getter: HostSurface re-reduces on every version bump until it exists. Only noted, no issue filed.",
  "Table `relativeTime` cells need the once-a-minute re-bind; only `formatRelativeTime` calls get the live clock.",
  "React-renderer-driven conformance for bind, text-direction, resizable and virtual-list: the v2 suites test the core reference functions, not React's own `bindTree`, `DirectedNode`, `ResizableNative` or `WindowedList`.",
  "Table `scrollToIndex` and the windowed-Table formatting cost have no test or bench; round2.test.tsx covers List only.",
  "A dev-time warning when a node's `template.component` is missing from `templates`, since a `root=`-only consumer now fails silently."
 ],
 "verdict": "The lane's test claims hold, but there are 5 major findings: a cross-renderer border drift, a surface crash on a bad currency code, conformance suites that don't exercise React, a per-keystroke re-reduce on HostSurface, and List spacing that changes at 50 items. Re-runs: react vitest 1196 passed / 1 skipped, tsc clean; Chromium suites 130 passed / 1 skipped; core `bun test` 320 passed; apps/web tsc clean; the jsdom bench passes. The gpui conformance ratchet the lane said still needed a re-run now passes against the rewritten baseline (2/2). Rewriting the baseline was step 9 of the lane's contract checklist, and apps/web/src is unchanged, so I found no edits outside the lane's paths."
}


## review:gpui (Review)

{
 "findings": [
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/paint.rs",
   "line": 363,
   "severity": "major",
   "summary": "To paint pinned sticky nodes on top, `children_elements` re-sorts them to the end of their siblings. That reorders the element tree and with it the AccessKit tree. a11y.json says a pinned section header 'stays first in reading order while pinned'.",
   "failure": "Take a List with `stickyHeaders`, scroll so section d1's header pins, then read the list with a screen reader. The d1 heading comes AFTER every row of its section (and after any later siblings), because `sort_by_key(|c| self.sticky.contains_key(c))` moves it last. A `position: sticky` toolbar is moved the same way. On the web the DOM order never changes. Tab order is unaffected because `focus_order` uses the core's order, so the bug is easy to miss.",
   "fix": "Keep the element order and lift only the paint: wrap the pinned child in `gpui::deferred(...)` (with a priority) or paint a non-accessible overlay copy, so the a11y node stays in DOM order. Add a round2.rs assertion that the pinned header's a11y position still precedes its rows."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/paint/chart.rs",
   "line": 336,
   "severity": "major",
   "summary": "Chart tick, value and tooltip labels still go through the hard-coded `fmt_value`, not the surface formatter. The round-2 contract \u00a79 (gpui item 3) requires 'Formatter \u2026 for Table cells, NumberField, chart ticks', and React formats them with `ctx.formatter.number`. The lane never mentions this.",
   "failure": "A bar chart with values 0\u20135000 under the core's English formatter shows ticks '1,000 \u2026 5,000' on the web and '1000 \u2026 5000' in gpui. With a `de` formatter the web shows '2,5' and gpui '2.5'. The y-label width estimate at line 167 also uses the unformatted string. The chart cases have no numeric text in the conformance dump, so the ratchet does not catch this.",
   "fix": "Route `fmt_value` through `self.surface` formatter `number(v, NumberOptions{decimals: up to 2, grouping: true})`, like React's chart.tsx, for ticks, value labels and tooltips, and size `y_label_w` from the formatted strings."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/state.rs",
   "line": 407,
   "severity": "minor",
   "summary": "A11y gaps the contract states: section headers become `Role::Heading` with no level (a11y.json: level 3), and list items never get position-in-set or set-size, although the core already emits `posInSet`/`setSize` and the pinned gpui has `aria_level`, `aria_position_in_set` and `aria_size_of_set`.",
   "failure": "In a 100k-row windowed List, a screen reader announces the rendered row as 'list item' with no '50,001 of 100,000'. Section headers have no heading level. round2.rs only asserts the role.",
   "fix": "In `accessible()`, call `aria_level(3)` for List/section, and forward `accessibility.posInSet`/`setSize` as `aria_position_in_set`/`aria_size_of_set` for ListItem rows. Assert both in round2.rs."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/paint/natives.rs",
   "line": 331,
   "severity": "minor",
   "summary": "The RLM/LRM that `with_paragraph_direction` prepends is counted as a character by the letter-spacing path (`tracking_width` counts `chars()`, and `tracked_text` offsets each glyph by `ls * chars_before`). Tracked text whose first strong character disagrees with the node's direction therefore gets one extra `letterSpacing` that the measurer never reserved.",
   "failure": "Neutral theme, `ar`/`he` surface, a DropdownMenu item with `shortcut: \"\u2318K\"`: the recipe sets `letterSpacing: 1`, the first strong character 'K' is L, so the line becomes RLM+\u2318K. Every glyph shifts 1 px and the last pixel is clipped by the content mask. A `lines`-clamped tracked text that fitted exactly now has `full > room + 0.5` and gets a false ellipsis.",
   "fix": "Skip invisible bidi marks when counting tracked characters (`tracking_width` and the `char_at` map), or apply the paragraph direction through the shaper instead of in-band marks for the tracked path."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/tests/round2.rs",
   "line": 206,
   "severity": "minor",
   "summary": "The test name says `scroll_to_index_moves_the_own_scroller_and_asks_the_host_for_its_own`, but the Recorder log is bound as `_log` and never checked. The list has its own height, so `OutEvent::ScrollSurface` \u2192 `HostPlugin::scroll_surface` (new in this lane, also used by the IDE screen) is never exercised.",
   "failure": "A regression that drops `OutEvent::ScrollSurface` in `dispatch`, or the `HostAdapter::scroll_surface` forward, passes the whole suite.",
   "fix": "Add a case with an unbounded List windowing against the host viewport, call `scroll_to_index`, and assert that `log.scrolled` holds the expected (x, y)."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/mod.rs",
   "line": 1203,
   "severity": "minor",
   "summary": "The host scroll reported to the core is always `(0.0, region.top)`, so the horizontal component is dropped.",
   "failure": "A horizontal List with no bounded width that windows against a horizontally scrolling host never re-windows: `set_surface_scroll` always sees x = 0, so cells past the first window stay unrendered. Only vertical host scrolling works.",
   "fix": "Report `(region.left, region.top)`, using the same 1 px hysteresis on both axes."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/paint.rs",
   "line": 480,
   "severity": "minor",
   "summary": "The Resizable handle registers `on_hover` on the 1 px hairline element, not on the 8 px hit area.",
   "failure": "Pointer 3 px beside the handle: the cursor changes to resize and a press drags, but the handle's `hover` recipe state (the highlighted line) never shows until the pointer is within the 1 px line.",
   "fix": "Attach the hover listener (and the cursor) to the `{id}.hit` area div."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/paint.rs",
   "line": 210,
   "severity": "minor",
   "summary": "The shimmer band is added before the border and the children, and it sets `overflow_hidden` on the node itself. React paints the band in `::after`, on top of the content, and does not clip the node's children.",
   "failure": "A container with `animation: shimmer` and children (a placeholder card): in gpui the band sweeps UNDER the children, and children or focus rings that overflow the box are clipped. On the web the band sweeps over them.",
   "fix": "Drop the redundant `el.overflow_hidden()`; the band wrapper already clips itself. Append the band after the children (or with `deferred`) so it paints on top, like `::after`."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/paint/natives.rs",
   "line": 432,
   "severity": "minor",
   "summary": "The Icon glyph size comes from a hard-coded size\u2192token map (`sm`\u2192iconSm, `default`\u2192iconMd, `lg`\u2192iconLg) instead of the theme's `Icon/root` recipe. That recipe is what sizes the icon on the web.",
   "failure": "A theme whose Icon/root recipe gives `size: sm` a different width, or keys its width on `tone`, sizes the web's svg by the recipe. gpui draws the token's size, so theme authors get a different glyph size on desktop.",
   "fix": "Read the width from `part_props(theme, mode, \"Icon\", \"root\", &node.props, states)` (as `button()` now does for its icon part), clamped to the box."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/mod.rs",
   "line": 395,
   "severity": "minor",
   "summary": "Every SurfaceView spawns a 60 s ticker that calls `surface.tick()` (it sets `needs_build = true`) and `notify()`. That is a full re-reduce, re-bind and layout of every surface once a minute, even when nothing binds a relative time.",
   "failure": "In an IDE with several surfaces, or one bound to the 100k-row bench list, each one rebuilds every minute for no visible change. That is a periodic hitch proportional to tree size. React does the same with `setMinute`, so this is perf rather than drift.",
   "fix": "Have the core report whether the last bind read the clock (`formatRelativeTime` without `now`), and tick only those surfaces, or tick at the next relative-time boundary."
  },
  {
   "file": "packages/exponential-ui/fixtures/conformance-known.json",
   "line": 1,
   "severity": "minor",
   "summary": "The ratchet budgets were rewritten wholesale: kitchen-sink `onlyRef` went UP from 1 to 6\u20138 per case. The new SURVIVORS gate covers only divergence origins, not coverage gaps, so web-only nodes can keep growing without anyone deciding to accept them.",
   "failure": "media-img.fallback, accordion.count.1, ring.label, chip-user.image.fallback, page-1.fallback, video.controls, audio.track and audio.controls are now not compared at all. A future divergence in those parts (or new web-only parts) passes as long as `onlyRef` stays within the raised budget.",
   "fix": "Hold `onlyRef` to an explicit list (for example a `coverageGaps` array next to `survivors`, each with a reason) and fail when an unlisted id is web-only. Better, place these parts as core nodes, as the lane's own deferral suggests."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/README.md",
   "line": 1,
   "severity": "minor",
   "summary": "The lane report and README do not match the artifacts. The report's main deferral ('one survivor, carousel', 4 size and 183\u2013193 position divergences per case) is stale: the written known file and my rerun show 0 size/position/wrap divergences, 0 origins and `survivors: []`. The report says 1609 cases where README/contract say 1613. README says the suites test writes conformance-known.json (the conformance test does) and that 'every built-in label comes from the strings table' (false: see `Clear search`).",
   "failure": "Reviewers and the Compose/Swift lanes act on a non-existent carousel survivor, and the README overstates string coverage.",
   "fix": "Update the report and README: carousel controls landed (0 origins), give the actual case count, name the conformance test as the writer, and qualify the strings claim."
  },
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/controls/inputs.rs",
   "line": 115,
   "severity": "minor",
   "summary": "The search field's clear button tooltip is hard-coded English ('Clear search'), although strings.json has `clear`. The \u00a73 string sweep (whose README claims every built-in label comes from the strings table) missed it.",
   "failure": "In an `ar` surface the Select search's clear button shows an English tooltip and accessible name.",
   "fix": "Use the surface's `clear` string (thread `builtin_string` or `strings` into the control builder)."
  }
 ],
 "missing_from_scope": [
  "Chart tick, value and tooltip labels through the surface Formatter (contract \u00a79 gpui item 3); `fmt_value` is still hard-coded and the lane does not mention it.",
  "List items' position-in-set/set-size forwarded to AccessKit (contract \u00a75, a11y.json List notes); the core emits `posInSet`/`setSize` and gpui drops them.",
  "Section header heading level 3 (a11y.json List notes); the role is set but there is no `aria_level`.",
  "RTL coverage for the new interactions: no gpui test for a Resizable drag in rtl (where ArrowRight/drag direction flips) or for horizontal-list windowing in rtl (offsets from the inline start).",
  "The bench does not check the fixture's `renderedItems` \u2264 window + 2\u00d7overscan + pinned header bound; it only prints the numbers.",
  "Host-viewport `scrollToIndex` path (`OutEvent::ScrollSurface` \u2192 `HostPlugin::scroll_surface`) has no test, although the test name claims it.",
  "`Clear search` English tooltip left behind in the strings sweep.",
  "SwiftUI has not been re-run against the core layout changes (type inheritance, recipe-under-defaults, one-line x-clip, aspect ratio). No Swift package files changed. Compose's known file was rewritten afterwards. The lane flags the re-run but does not record it as a blocker."
 ],
 "verdict": "Mostly holds, with 2 major and 11 minor findings. I re-ran `cargo test -p exponential-ui -p exponential-ui-gpui --no-fail-fast` (every suite passes, including round2 9/9, round2_parity 7/7, conformance, suites), clippy -D warnings (clean) and `cargo check -p ui` (compiles). The conformance run is actually better than the report says: 0 size/position/wrap divergences and 0 origins in all 48 cases, with SURVIVORS empty. So the report's carousel survivor is stale. The ratchet still went up in one place: kitchen-sink onlyRef rose from 1 to 6\u20138 per case, and no survivor-style gate covers it. Major issues: (1) the sticky paint-on-top is done by re-sorting the element tree, which moves a pinned section header to the end of the AccessKit reading order, against a11y.json; (2) chart ticks and value labels skip the Formatter the contract requires, so they differ from the web for values of 1,000 and up and for non-English locales. Minor issues: missing list posinset/setsize and heading level; the bidi mark adds a letterSpacing on tracked text in a direction-mismatched node; the test named for host scrollToIndex never checks it; host scroll ignores x; the resize hover sits on the hairline only; the shimmer band paints under children; Icon size ignores the theme's recipe; every surface rebuilds once a minute; the report and README are stale; one English tooltip remains. The core edits outside the lane's paths are disclosed and locked by round2_parity tests. They match the Compose measurer's one-line rule, and Compose's known file was rewritten after them. SwiftUI has not been re-run."
}


## fix:docs (Review)

## Done

There were no blockers or majors. All 9 minors and all 4 missing-from-scope items are fixed.

1. **Empty Variants section (AppBar, NavRow, TreeGuides).** `VariantGallery.tsx` now exports `galleryVariants(doc, specimen)`, built on the same `thumbProps` base the gallery renders. `hasVariants(doc, specimen)` and `Component.tsx` use that same list, so the section only shows when the gallery has entries. In the rebuilt dist those three pages have no Variants section, no empty `sdk-variants` div, and Button shows 11 variants.
2. **Variants test.** `docs-coverage.test.ts` now checks that a variant changes exactly `[v.prop]`, or nothing when it is the base's current value (kept on purpose). It also checks that each component has at least one variant that differs from the base, and that `hasVariants` agrees with a non-empty gallery for every doc.
3. **Dark flash for light-scheme visitors.**
   - The theme's two backgrounds now go out as CSS variables (`groundVars()` in `backgrounds.ts`).
   - The new `useModePick()` in `scheme.ts` gives the stage an `is-auto` class until the visitor picks a mode, and `<html data-scheme>` decides the colour in CSS.
   - This covers the studio stage, the theme gallery stage, and every MiniSurface (index cards and variants).
   - A screenshot with the JS bundle blocked (the page before hydration, light OS) shows light thumbnails and a light stage.
   - Left over: before hydration the Mode switch still shows "Dark" as pressed. Only the pressed state is wrong, not the colours.
4. **Heading names like "01Preview".** `DocsSection` in site-shell takes a new opt-in `heading` prop. With it on, the tag gets `role=heading aria-level=2` and the number is `aria-hidden`; the line is always `aria-hidden`. Only `Component.tsx` turns it on.
   - The marketing docs pages and the ui-site Concepts, Conformance and Guide pages bring their own `<h2>`. They get no heading role, so the duplicate h2s you predicted (confirmed in the marketing source) are avoided.
   - Checked in a browser: the headings read "Preview | Platforms | Variants | …".
5. **Toolbar role.** The studio bar is now `role="group"` with the same label. No `[role=toolbar]` is left.
6. **Embed code ignored RTL and Width.** `embed.ts` now takes `direction` and `width` from the studio.
   - React passes `direction="rtl"` and `width={N}`.
   - Swift gets `.frame(width:)`, Compose `Modifier.width(N.dp)` with its imports, and gpui a `.w(px(N.))` hint.
   - The native hosts have no direction option (RTL follows the device locale), so their code says that in a one-line comment.
   - Real bug found while writing the round-trip test: a JSON backslash followed by `#` turns into an escape inside a Swift `#"""` raw string. `rawHashes` now also avoids `\` plus the hashes.
   - The new test reads each snippet's string back the way its compiler would: it evaluates the JS template literal, parses Swift with the escape rule, replaces Kotlin's `${'$'}` and checks no other `$` is left, and checks the Rust raw string's delimiter. Each result must equal `jsonl(messages)`, for 5 hostile labels. A second test checks that direction and width appear only when set.
7. **Phone menu and browser bar colour.**
   - `Shell.tsx`: Escape closes the menu (focus goes back to the menu button) and a tap outside closes it. Without JS the plain `<details>` still works.
   - `index.html` now has one `theme-color` meta. The inline scheme script sets its colour, and `setScheme` and the OS-change handler update it through the `THEME_COLOR` constant in `scheme.ts`.
   - The CSP hash in `public/serve.json` is updated to the new script.
   - Checked in a browser: `#0a0a0a` before the toggle, `#fafafa` after it and after a reload; Escape, outside tap and a click inside all behave correctly.
8. **Dead `hasShot`.** Removed from `catalog.ts`, along with its `SHOT_INDEX` import. `shotExists` in `Shot.tsx` is now the only lookup.
9. **Cramped phone thumbnails.** `MiniSurface` takes a new `minWidth` prop: the zoom becomes `min(zoom, box width / minWidth)`, so a narrow box zooms out instead of squeezing the layout. The index cards use `minWidth={300}`; on desktop the zoom stays close to 0.72. In the 390px screenshot the Grid card shows "Cell 1" … "Cell 6" whole.

**Missing from scope:**
- `playwright` is now in ui-site's devDependencies.
- The dist pages are now checked against the CSP: a new test in `scheme.test.ts` hashes every inline script of every `dist/**/index.html` and checks it against the built `dist/serve.json`. It is skipped when there is no dist.
- The screenshot script has new light, phone-light, light-before-hydration and light-variants shots.
- A test now checks that every entry in `ORDERED_DOCS` is a non-hidden catalog component.

**Files outside my paths:** `bun.lock` (one added line, the ui-site workspace's `playwright` entry, written by `bun install`).

## Contract notes
- None. No fixtures, generated files or SDK packages were touched. The embed snippets only use host APIs that already exist (React `direction` and `width` props; the native hosts' locale-based direction).

## Deferred
- **Studio and gallery Mode switch before hydration:** "Dark" shows as pressed for light visitors until JS runs; the colours are already right. Smallest next step: drive the pressed style from CSS using `html[data-scheme]` and `is-auto`.
- **Variants in RTL:** the gallery always renders LTR (only the studio has a Direction switch), and no screenshot covers it. Smallest next step: if wanted, pass the studio's direction to the gallery.

## Tests run
- `cd apps/ui-site && LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 bun run test`: 30 pass, 0 fail, 6 files. The new CSP check against dist ran, because dist was present.
- `cd apps/ui-site && bun run build`: gen, tsc, vite build and prerender all clean (99 URLs).
- `cd packages/site-shell && bunx tsc --noEmit`: clean. `cd apps/marketing && bunx tsc --noEmit`: clean.
- `OUT=/tmp/ui-site-shots-fix bun run scripts/screenshots.ts`: 27 screenshots, no page errors. I looked at `components-light-prerender.png`, `component-button-light-prerender.png`, `components-phone-light.png` and `component-button-variants-light.png`.
- A one-off Playwright check against the built dist (script deleted afterwards) confirmed the browser bar colour, the menu closing, the heading names and that no toolbar role is left.
- Built dist checked with grep: no Variants section and no empty variants div on the app-bar, nav-row and tree-guides pages; the stage prerenders with the `is-auto` class and both background variables.


## recheck:docs (Review)

{
 "findings": [],
 "missing_from_scope": [],
 "verdict": "Nothing is still open: all 9 minor findings and all 4 missing-from-scope items are fixed.\n\nChecks I ran:\n- `bun run test` in apps/ui-site: 30 pass, 0 fail. This includes the dist CSP check, which ran.\n- `tsc --noEmit` in ui-site and site-shell: both clean.\n\nWhat the code now shows:\n- **Variants section:** `galleryVariants` is the single source for both `hasVariants` and the gallery. The test asserts that they agree, that each variant changes exactly its own prop (`[v.prop]`, or nothing for the base's current value), and that no gallery is only copies of the base.\n- **Dark flash:** stage and thumbnail backgrounds use the `groundVars` CSS variables plus the `is-auto` class, chosen by `html[data-scheme]` before JS runs.\n- **Section headings:** site-shell `DocsSection` has an opt-in `heading` prop, and the number and line are `aria-hidden`.\n- **Toolbar role:** none is left in ComponentStudio.\n- **Embed code:** it now gets the studio's direction and width. A round-trip test reads each platform's snippet back to the original JSONL.\n- **Phone menu:** it closes on Escape or a tap outside. The inline script and `setScheme` update the `theme-color` meta.\n- **`hasShot`:** removed from catalog.ts.\n- **Phone thumbnails:** MiniSurface's `minWidth` keeps index cards at a sane width on phones.\n- **Missing-from-scope items:**\n  - `playwright` is declared in ui-site's package.json.\n  - The dist pages are checked against the built CSP hash.\n  - There is a test that no hidden component reaches the index.\n  - The screenshot script has light, phone and pre-hydration shots.\n\nNot covered by any finding: the fixer deferred two items. Before hydration the Mode switch shows \"Dark\" as pressed (the colours are already right), and the variant gallery has no RTL rendering."
}


## fix:contract (Review)

## Done

**Majors (all four fixed in the TS reference and its fixtures)**
1. **Animation opacity now multiplies on web.** `keyframesCss` animates `--xui-a-opacity` instead of `opacity`. When a set animates opacity, `styleToCss` writes `opacity: calc(<own> * var(--xui-a-opacity, 1))`. New exports: `ANIMATION_PROPERTIES_CSS` (the `@property` rules for `--xui-a-opacity` and `--xui-band`), `paintedOpacity` and `animatesOpacity`. `animations.json` gains `properties` and 5 `opacity` cases with base opacities other than 1. For example, own 0.5 + pulse gives 0.25 at mid, and a finished fade-in keeps the author's 0.6.
2. **Template cycles.** `liftTemplates` now catches a template that would instantiate itself: one naming its own owner, an ancestor of its owner, the root, or looping through other templates (including two templates built separately on the flat path). Each is reported as `template: cycle through this id` and is not lifted. The node stays in place and its owner renders no items. Four new `template-items.json` `reduce` cases cover these.
3. **Reopening a collapsed panel follows the pointer.** It reopens at max(min, delta), clamped like any other drag. `resizePanels([0,100],0,50,…)` now gives `[50,50]`. Three new `resizable.json` cases: first panel, second panel, and clamped by max.
4. **The English fallback uses the surface zone.** `englishFormatter(zone?: ZoneOffset)` takes the UTC offset per instant; helpers are `fixedOffset` and `intlZoneOffset`. A `yyyy-mm-dd` value never shifts. `format.json` gains `zoned` cases (Berlin, New York, Kolkata, Los Angeles, Kiritimati), each with `timeZone` for ICU and `offsetMinutes` for the fallback. The test checks fallback == Intl == the offset.

**Minors**
- **Hand-written decision tests** in `round2.test.ts`, independent of the generator: keys and `#<index>`, suffix escaping, collapse and reopen, keyboard rules, sticky handoff, cycles, opacity, sectioned scroll, numeric strings, zone.
- **npm package:** `prepare-npm.ts` now excludes only the `conformance/fonts/` directory, so `fonts.json` ships.
- **Instance suffixes:** keys are escaped with `instanceSegment` (`~`→`~0`, `.`→`~1`). New `dotted-keys-escaped` case.
- **Locale data:** the patterns, the joiners and the unknown-currency template are now data in `locale.json` `englishFallback` (`datePatterns`, `timePattern`, `dateTimeJoin`, `unknownCurrency`). `format.ts` reads them from there.
- **List performance:** `virtualWindow` and `scrollOffsetForIndex` take optional cached `offsets` and use binary search. `stickyHeader` uses binary search too. Fixture outputs did not change.
- **Format suite:** runs replay through the English fallback and must match exactly. A platform Formatter may run too, comparing after mapping U+202F and U+00A0 to a space. Numeric strings follow a fixed grammar: hex, `Infinity` and `1,000` are not numbers. Seven new cases.
- **Docs:** the Image 16:9 default is in §10. React checklist item 4 now says "eight ids of §3". The case count is now 1642 (it was 1613 before this lane; 1609 was the old report's figure).

**Gaps the review found outside the listed scope**
- `catalog/host.json` is now version 2, with a new `surface` section: `settings` (including `timeZone`), the Formatter's methods, and `commands` (including `scrollToIndex`). It is checked in `host.test.ts`.
- §4 now defines cycle handling.
- §3 now lists the hard-coded SwiftUI labels (doc only).
- New `scrollOffsetForItem` maps a data index to its flat row in a sectioned list, using its own section header as the inset when headers are sticky. Four new `sectionedScrollTo` cases.
- Animation RTL rule: channels are physical and `direction` never mirrors them. This is stated in the `style.json` comment and §2.

Files: everything under `/home/niach/Exponential/repos/Niach/exponential.worktrees/exp-VAPP-99/packages/exponential-ui/`. The generator also rewrote `apps/desktop/crates/exponential-ui/src/generated/catalog.rs` (its `LOCALE_JSON` mirror). I touched nothing else outside the package.

## Contract notes
The Rust core needs these changes before its fixture tests pass again:
- `resizable.rs`: reopen rule.
- `animation.rs`: `--xui-a-opacity` keyframes, `ANIMATION_PROPERTIES_CSS`, `painted_opacity`.
- `host/contract.rs`: version 2 and the `surface` section.
- `list.rs`: `instanceSegment` escape, cached offsets with binary search, `scroll_offset_for_item`.
- `reducer.rs`: cycles.
- `format.rs`: a zone offset per instant; read the patterns from `englishFallback` instead of hand-copied ones; replay `zoned`.

All of this is in the §9 checklists. React must register `ANIMATION_PROPERTIES_CSS` in place of its own `--xui-band` rule in `base-css.ts`. Without it the opacity animation jumps instead of fading.

## Deferred
- **Rust, React, gpui, Compose and Swift ports of these rules:** the renderer lanes own those paths. The smallest next step is the §9 items above.
- **Table part ids** (`cell.<rowKey>.<i>`) keep the raw row key on purpose, so the React and gpui dumps don't change.

## Tests run
- `bun run --filter @exponential-at/ui generate`: run twice. The second run wrote nothing, so the output is stable.
- `bun run --filter @exponential-at/ui test`: 330 pass, 0 fail.
- `bun run --filter @exponential-at/ui typecheck`: exit 0.
- `bun run --filter @exponential-at/ui-react test`: 13 failed, 1183 passed. All 13 are `List/*` snapshots differing by `style="gap: 1px;"` and `data-axis` markup from a concurrent React edit, not from this lane.
- `cargo test -p exponential-ui --no-fail-fast` (CARGO_TARGET_DIR=/tmp/xui-r2-target): expected failures pending the Rust port:
  - `conformance`: resizable 3 failed, animations 18 failed (css), and the virtual-list and animations suites did not run their new cases.
  - `host_fixtures::the_contract_constants_match_catalog_host_json`.
  - `round2_fixtures`: resizable and animations.
  - Everything else passes.


## recheck:contract (Review)

{
 "verdict": "All 4 majors, every minor and every scope gap from the review are closed in the contract package. Nothing is still open in the contract lane.\n\nWhat I ran:\n- `bun run --filter @exponential-at/ui test`: 330 pass, 0 fail (this includes the generate drift gate).\n- `bun run --filter @exponential-at/ui typecheck`: exit 0.\n\nEach major, checked directly against `packages/exponential-ui/src`:\n1. **Resize reopen:** `resizePanels([0,100],0,50,[{collapsible:true,min:20},{}])` now returns `[50,50]`. The mirrored second-panel case (`[100,0]`, delta -50) also returns `[50,50]`.\n2. **Template cycles:** both the ancestor case (root \u2192 col \u2192 l naming col) and the self case (l naming l) now report `template: cycle through this id` and lift nothing. `template-items.json` has cycle-self, cycle-ancestor, cycle-root and cycle-mutual-built cases.\n3. **Animation opacity:** with a theme, `styleToCss({opacity:0.6, animation:'fade-in'})` gives `opacity: calc(0.6 * var(--xui-a-opacity, 1))`. The keyframes animate `--xui-a-opacity`, `ANIMATION_PROPERTIES_CSS` registers it, and `animations.json` has cases with a base opacity other than 1.\n4. **Time zone:** `englishFormatter(fixedOffset(120))` formats `2026-10-14T22:30:00Z` with `HH:mm` as `00:30` and leaves a plain `yyyy-mm-dd` day unshifted. `format.json` has `zoned` cases carrying both `offsetMinutes` and `timeZone`.\n\nEach minor, checked in the files:\n- `prepare-npm.ts` now excludes only the `conformance/fonts/` directory, so `fonts.json` ships.\n- Instance suffixes go through `instanceSegment` (`~`\u2192`~0`, `.`\u2192`~1`).\n- The patterns, joiners and unknown-currency template are read from `locale.json` `englishFallback`.\n- `virtualWindow` and `scrollOffsetForIndex` take cached offsets and use binary search.\n- The format suite replays through the English fallback; a platform formatter may also run after mapping U+202F and U+00A0 to a space. The numeric-string grammar has fixture cases (`0x10`, `Infinity` and others).\n- \u00a710 lists the Image 16:9 default, and React checklist item 4 now says \"eight ids of \u00a73\".\n- `round2.test.ts` has a block of hand-written decision tests that do not come from the generator.\n\nThe scope gaps:\n- `host.json` is at version 2 with a `surface` section (settings including `timeZone`, the Formatter, and commands including `scrollToIndex`).\n- \u00a74 defines cycle handling.\n- \u00a73 lists the hard-coded SwiftUI labels.\n- `scrollOffsetForItem` and its sectioned cases cover `scrollToIndex` on a list with section headers.\n- The `style.json` comment says animation channels are physical and are not mirrored under rtl.\n\nThe remaining work sits in other lanes. The Rust core fixture tests fail until it ports these rules. React must swap its own `--xui-band` rule for `ANIMATION_PROPERTIES_CSS`. Both are listed under missing_from_scope.",
 "findings": [],
 "missing_from_scope": [
  "Rust core port (renderer lane, not the contract): resizable.rs reopen rule, animation.rs --xui-a-opacity / ANIMATION_PROPERTIES_CSS / painted_opacity, host contract version 2 and its surface section, list.rs instanceSegment + binary search + scroll_offset_for_item, reducer.rs cycle detection, format.rs zone offset + englishFallback data + zoned replay. cargo conformance, host_fixtures and round2_fixtures tests fail until this lands.",
  "React renderer (renderer lane): base-css.ts must register ANIMATION_PROPERTIES_CSS in place of its own --xui-band @property rule. Without it the opacity animation jumps instead of fading."
 ]
}


## fix:gpui (Review)

## Done

All 2 majors and 10 of the 11 minors are fixed. I'm disputing one (`Clear search`, evidence below). Every file I touched is in my owned paths: `apps/desktop/crates/exponential-ui-gpui/**` (new: `src/view/pinned.rs`) and `packages/exponential-ui/fixtures/conformance-known.json`. `exponential_ui_screen.rs` needed no change.

- **Sticky / pinned headers keep reading order (major).** The re-sort is gone. A new `PinnedStack` element (`src/view/pinned.rs`) lays out and prepaints children in DOM order, so the AccessKit tree, focus and reading order stay the core's. Only the paint order changes: pinned children paint last.
  - The finding's suggested `gpui::deferred` was not used. At the pinned zed rev, deferred prepaint runs with only the window root on the a11y stack, so the header would become a child of the window root and lose its parent.
  - During prepaint, siblings' hit areas are clipped out of the pinned box. A press on a pinned header no longer reaches the row scrolled under it. This was already a bug before, because a pinned box without listeners has no hitbox of its own.
- **Chart labels use the formatter (major).** `chart::fmt_value(formatter, v)` now calls `formatter.number(v, NumberOptions::default())`, which is exactly what React's `ctx.formatter.number(v)` does. Ticks, value labels, tooltips and the y-label gutter width all use it. `LeafCx` gained a `formatter` field.
- **List a11y.** Section headers get `aria_level(3)`, or the core's `accessibility.level` if it sets one. The core's `posInSet`/`setSize` go to `aria_position_in_set`/`aria_size_of_set`. `AccessibleInfo` has new `level` and `position` fields so tests can check them.
- **Letter-spacing and bidi marks.** A new `measure::is_tracked` skips bidi controls (LRM, RLM, ALM, embeddings, isolates) in `tracking_width` and in the per-glyph `char_at` map.
- **Host scroll x.** `VisibleRegion` gained `left`, and the view now reports `(region.left, region.top)` to `set_surface_scroll`.
- **Resizable hover.** The hover listener and cursor moved from the 1 px line to the 8 px `{id}.hit` area.
- **Shimmer.** The redundant `overflow_hidden` on the node is gone. The band is added after the children, so it paints on top like `::after`.
- **Icon size.** The glyph is now the theme's `Icon/root` recipe width, capped at the box.
- **Minute ticker.** The view wraps its formatter in a `ClockWatch` that notes when `relative_time` runs. The 60 s ticker only re-binds a surface whose last bind formatted a relative time; other surfaces never tick. New public method: `SurfaceView::ticks_minutely()`. This needed no change in the core.
- **Coverage-gap gate.** `tests/conformance.rs` has a new `COVERAGE_GAPS` list (id, side, reason). Any node placed by only one side that isn't on the list fails the test, and so does a stale `coverageGaps` count. The writer records `coverageGaps` in the known file. Today there are 9 gaps: the 8 parts gpui paints inside a leaf, plus `panels.grip.0` (gpui only). The known file was rewritten; `causes`, `rules` and the budgets are unchanged.
- **README.** Corrected:
  - Sticky behaviour, heading level and position in set.
  - The formatter now covers charts.
  - The conditional ticker.
  - The strings claim, limited to labels a surface paints.
  - The conformance state: 48 cases, 0 divergences, 0 origins, 0 survivors, 9 coverage gaps.
  - The suites case count, dropped because it drifts (1642 today); the README now says the suites test writes a pass/fail report.
- **Bench.** `bench_list` now asserts `renderedItems ≤ window + 2 × DEFAULT_OVERSCAN + 1`, so it no longer only prints the numbers.

**Disputed: `Clear search`.** `controls::search_field` is IDE chrome. Its only callers are in `apps/desktop/crates/ui` (`search_sheet.rs`, `diff_pane.rs`, `picker/mod.rs` and others). No surface code calls it, and the surface's Select search is painted from core nodes. I left it alone and corrected the README instead.

## Contract notes
- The core emits `posInSet`/`setSize` only for windowed lists. gpui forwards whatever the core emits, so a non-windowed list item has no position in gpui. a11y.json wants the whole-list position everywhere. The smallest fix is in the core: emit both values for every list item in `layout_tree/containers.rs`, so all renderers forward the same thing.
- Element order is now the a11y order while a header is pinned, which matches a11y.json ("stays first in reading order while pinned").
- Glyph size now follows the recipe. On the web the svg fills 100% of the node's box, so an Icon whose own style makes the box larger than its recipe size draws a larger glyph on the web than in gpui.

## Deferred
- **SwiftUI re-run** against the core layout changes. The MacBook run owns it; I touched nothing on the Swift side.
- **Core perf (not my path):** `call_function` calls `now_ms()` eagerly on every function call. A lazy clock would be cheaper.
- **No test for host scroll x.** The horizontal host-scroll report has no test because the headless window's clip does not scroll on x. Next step: a host pane with a scrolled `overflow_x` wrapper in `round2.rs`.

## Tests run
All with `CARGO_TARGET_DIR=/tmp/xui-r2-target RUSTUP_TOOLCHAIN=1.96.0`, from `apps/desktop`.

- `cargo test -p exponential-ui-gpui --no-fail-fast`: everything passes.
  - lib 56 (includes new unit tests for `pinned::outside`, the bidi mark in tracking, and the chart formatter, including `5000 → "5,000"`)
  - conformance 2
  - host 6
  - interaction 29
  - render 9
  - round2 13 (was 9)
  - suites 1
- **New in round2:**
  - RTL Resizable drag and arrow keys
  - hover on the hit area
  - sticky element order and a press on the pinned box not reaching the row under it
  - section header level 3 and pinned header before its rows
  - a windowed row's position `(1, 200)`
  - host `scroll_to_index` checks `log.scrolled`
  - RTL horizontal windowing
  - minute tick on/off
- **Counterfactuals:** with the old sort, both order assertions failed. Without the hit-area clipping, the press test failed (row r7 took the press).
- `cargo test -p exponential-ui -p exponential-ui-gpui --no-fail-fast` (earlier run): 4 targets failed. All 4 trace to fixtures and catalog files that other lanes rewrote mid-run (00:49–00:51): `resizable.json` (`reopen-*` cases), `animations.json` (`--xui-a-opacity`) and `catalog/host.json` `$.surface`. The 4 targets were core conformance, `host_fixtures`, `round2_fixtures` and gpui `suites`. gpui `suites` passed on the later run.
- `cargo clippy -p exponential-ui-gpui --tests --examples -- -D warnings`: clean.
- `cargo check -p ui`: compiles.
- `EXP_UI_WRITE_FIXTURES=1 cargo test -p exponential-ui-gpui --test conformance`, then a re-run without the variable: passes.
- `cargo run --release -p exponential-ui-gpui --example bench_list`: firstPaintMs 66.6, scrollStepMs 5.087, scrollToIndexMs 6.705, renderedItems 30. The new bound assertion holds.


## recheck:gpui (Review)

{
 "verdict": "Twelve of the 13 findings are fixed. The one dispute holds: `controls::search_field` is IDE chrome, called only from `apps/desktop/crates/ui` (for example `search_sheet.rs`), and no surface code paints it. I ran `cargo test -p exponential-ui-gpui --no-fail-fast` and every target passes: lib 56, conformance 2, host 6, interaction 29, render 9, round2 13, suites 1. I confirmed each fix in the code:\n- Pinned and sticky children go through `PinnedStack` (`src/view/pinned.rs`); the sort is gone.\n- `fmt_value` now uses the surface formatter for ticks, value labels, tooltips and `y_label_w`.\n- `aria_level` and `aria_position_in_set`/`aria_size_of_set` are set in `paint.rs`.\n- `is_tracked` skips bidi marks when counting tracked characters.\n- The host scroll reports `(region.left, region.top)`.\n- The resize hover and cursor sit on the 8 px `{id}.hit` area.\n- The shimmer band paints after the children; `overflow_hidden` now comes only from `clip_x`/`clip_y`.\n- The Icon size comes from the theme's `Icon/root` recipe.\n- `ClockWatch` limits the minute ticker to surfaces that format a relative time.\n- The `COVERAGE_GAPS` gate is in `tests/conformance.rs`.\n- The host `scroll_to_index` test asserts `log.scrolled`.\n\nThree items are still open, all minor. Two are test or scope gaps; the third is a cross-lane re-run.",
 "findings": [
  {
   "file": "apps/desktop/crates/exponential-ui-gpui/src/view/mod.rs",
   "line": 1256,
   "severity": "minor",
   "summary": "The host scroll now reports `(region.left, region.top)`, but no test covers the x component. The fixer deferred it because the headless window's clip does not scroll on x.",
   "failure": "If `region.left` falls back to 0 (for example in the `VisibleRegion` computation), horizontal lists that window against the host stop re-windowing, and the whole suite still passes.",
   "fix": "In round2.rs, add a host pane with a scrolled `overflow_x` wrapper around an unbounded horizontal List, and assert that `set_surface_scroll` receives a nonzero x and that the list re-windows."
  },
  {
   "file": "apps/desktop/crates/exponential-ui/src/layout_tree/containers.rs",
   "line": 176,
   "severity": "minor",
   "summary": "The core emits `posInSet`/`setSize` only for windowed lists (`if let (Some(c), true) = (child, windowed)` at lines 176 and 244). gpui forwards what the core emits, so items in a non-windowed list have no position in the set. a11y.json asks for the whole-list position on every item. This is a core contract gap and lies outside the gpui lane.",
   "failure": "In a short List that is not windowed, a screen reader in gpui announces 'list item' with no 'n of m'.",
   "fix": "Emit both values for every list item in `containers.rs` so that every renderer forwards the same thing, and add a parity assertion for a list that is not windowed."
  }
 ],
 "missing_from_scope": [
  "SwiftUI still has not been re-run against the core layout changes (type inheritance, recipe-under-defaults, one-line x-clip, aspect ratio). It is deferred to the MacBook run and no Swift files changed.",
  "The 'Clear search' tooltip is no longer counted as open: `controls::search_field` is IDE chrome, used only by `apps/desktop/crates/ui`, never by a surface, and the README claim was narrowed to match."
 ]
}


## fix:compose (Review)

## Done

All 5 majors and all 9 minors are fixed, apart from the two noted under Deferred. The unit suite is green: 55 tests, 0 skipped, 0 failed. The conformance report is CONFORMANT with 1642 cases in 24 suites. The three on-device tests also pass.

**Majors**
- **Image gate red.** I rebuilt the facade from the current core and re-recorded the Roborazzi images, after reviewing the diffs first:
  - AppBar's new bottom rule and the sub-pixel Stack shift come from the theme and core lanes.
  - CodeBlock had a real painter bug. The new monospace italic comment's last glyph was clipped by `BasicText`'s default Clip. `CodeLineLeaf` now uses `overflow = TextOverflow.Visible`.
  - A standalone `TreeGuides` is now 0 tall. That comes from the core's round-2 rule that it stretches to its row, so the image test exempts it (`ROW_STRETCHED`).
- **Resizable drag.** The pointer travel is now measured in root coordinates (`onPlaced` + `localToRoot`), so the handle moving under the finger no longer shrinks the delta. The new `Round2GestureTest` drags it with `performTouchInput` in 12 steps of 5 dp, and the panel follows by 60 dp both mid-drag and on release.
- **Chart and Slider numbers.** Value labels, the tooltip, the chart and sparkline summary, and the Slider `stateDescription` now go through `ChartModel.format(v, model.formatter)`. The summary fills the same way as the TS reference's `chartSummaryParams`. Tested: de-DE gives `1.234,5` and en-US gives `1,234.5`.
- **Per-step scroll pass.** `scrollTo(id, x, y, fromView = true)` skips the layout pass for a plain container. It still runs one when a windowed list or a sticky node sits inside. When the core moves an offset (a key, `scrollIntoView`, a clamp), the view follows through `scrollEpoch`. The visible height now applies at once only when it grows by 48 dp or more; smaller changes wait for 150 ms of rest. Tested: four view scrolls cause no pass, Home causes one pass and moves the view, and a sticky node inside forces a pass.
- **Swift parity check.** The `assumeTrue` skip is gone. Every case both snapshots have is compared, and cases only one side has are listed. The 46 shared cases that drifted (because the Swift snapshot predates round 2) are listed in the new `src/test/snapshots/swift-parity-pending.json`. Any drift outside that list fails, and so does a listed case that matches again.

**Minors**
- **README.** Corrected to the shipped rules: fractional widths plus `TEXT_SLACK_PX`, nowrap min-content for one-line text (and that the Swift measurer still uses the 0 rule), the measurer ceiling the wrap width, the scroll cost table, chart behaviour, toasts, `visibility`, the ratchet, and a table showing which suites run through the painter and which through ports or bindings.
- **KeyboardTest.** It now tabs to the tabs instead of seeding focus, and sends a real `KEYCODE_ESCAPE`. It passes on the `Medium_Phone_API_34` emulator.
- **Assertive announcements.** One announcement event now carries the text; `assertive` first calls `AccessibilityManager.interrupt()`.
- **Field errors.** They are now joined one per line, with no Latin punctuation.
- **Font ratchet.** An improvement now fails until its budget is lowered. The total coverage gap is printed (174 web-placed nodes Compose does not emit).
- **Chart parity with React.** Cartesian kinds mirror under RTL (pie and donut do not). The plot is focusable: ArrowLeft/ArrowRight move the tooltip, Home/End jump, Escape hides it. A missing value is skipped when drawn and reads `—` in the tooltip.
- **Toast timers.** A hold keeps the remaining time instead of restarting the full duration. Hover and press are counted separately, and gone toasts are forgotten. The test checks that 2 s were left after the hold.
- **`visibility: hidden`.** It is now inherited and takes no taps or focus. This covers pressables, focusables, hover, text fields and the Resizable handle, and an invisible box swallows the pointer for self-handling controls under it. Tested by tapping a hidden button and a button inside a hidden box.
- **Conformance suites.** New fixture sections landed during this run, so I updated the test-side ports to match the TS reference: `instanceSegment` escaping, `--xui-a-opacity` in the keyframes CSS, `zoned` (through both the English fallback and the painter's `IcuFormatter`), `sectionedScrollTo`, and the `opacity` cases (own × frame, using the painter's `AnimFrame`).

## Contract notes
- **Disputed: list separators in the chart summary.** I kept `", "`. The TS reference `chart.ts` `chartSummaryParams` joins series names with `` `, ` `` and the fixture locks it ("Chart of Runs, values from -1 to 8"), so `ListFormatter` would diverge from the reference. Field errors did change: they are now joined by newline.
- **Partly disputed: missing chart points.** React does not draw a gap: it filters out non-finite points and joins the line through them (`chart.tsx`). Compose now matches that rather than drawing a gap.
- **Swift drift for the iOS lane.** `TextMeasurer.swift` still shrinks a `lines: 1` text to 0 at min-content, while gpui and Compose treat it as nowrap. 46 cases also wait for a Swift snapshot re-record (see `swift-parity-pending.json`).
- **TreeGuides.** A standalone `TreeGuides` lays out 0 tall in the current core, because it stretches to its row.

## Deferred
- **Generator `$` escaping.** `packages/exponential-ui/scripts/generate.ts` `renderKotlin` still writes `\\$breakpoint` into `ExponentialUICatalog.generated.kt` (line 14), which Kotlin reads as a string template. That file is not mine. Smallest fix: emit Kotlin strings with `JSON.stringify(v).replace(/\$/g, "\\$")`, then replace `CatalogConstants` with the generated object.
- **Re-records may go stale again.** The images and `components.json` match the core as of about 01:05. The core and theme lanes were still editing during this run, so the image and snapshot gates may need another `EXPONENTIAL_UI_RECORD=1` pass after the last core lane lands. Review the PNG diffs first.
- **Device tests and later commits.** The device tests ran against an x86_64 `.so` built before the FFI lane regenerated the Kotlin bindings at 00:57. I did not rerun them after that.

## Files
Everything is under `packages/exponential-ui-compose`. New files: `ui-compose/src/test/kotlin/at/exponential/ui/Round2GestureTest.kt` and `ui-compose/src/test/snapshots/swift-parity-pending.json`. I also edited the README, KeyboardTest, ChartLeaf, ChartModel, NodeView, TextFields, TextLeaves, Semantics, ExponentialSurface, Containers, SurfaceModel and SurfaceInteraction, plus the tests Round2InteractionTest, Round2Suites, SnapshotTest, RealFontConformanceTest and ComponentSnapshotImagesTest, and the re-recorded images.

Outside my path I touched only build outputs: `/tmp/xui-r2-target`, the gitignored `apps/desktop/crates/exponential-ui-ffi/out/jniLibs/x86_64/libexponential_ui_ffi.so` (`cargo ndk -t x86_64 --profile mobile`), and the conformance report `.conformance/exponential-ui-compose.json`.

## Tests run
- `cargo build -p exponential-ui-ffi --release --features cli` with `CARGO_TARGET_DIR=/tmp/xui-r2-target`: ok.
- `EXPONENTIAL_UI_RECORD=1 ./gradlew :ui-compose:testDebugUnitTest --tests '*ComponentSnapshotImagesTest*'`, then `--tests '*SnapshotTest.matchesTheSwiftSnapshot'`: recorded.
- `./gradlew :ui-compose:testDebugUnitTest` (ANDROID_HOME, CARGO_TARGET_DIR, LANG/LC_ALL set): **55 tests, 0 skipped, 0 failed**.
- `bun run --filter @exponential-at/ui conformance:check .conformance/exponential-ui-compose.json`: **CONFORMANT, 1642 cases in 24 suites**.
- `./gradlew :example:connectedDebugAndroidTest` on the `Medium_Phone_API_34` emulator: **3/3 passed** (AccessibilityOrderTest, KeyboardTest with the real Escape key, TypingTest).
- `./gradlew :example:assembleDebug :example:assembleDebugAndroidTest`: ok.
- `wc -m CLAUDE.md`: 39986, not edited.
