# Shared UI inventory (SLOP-18)

The primitives every Exponential client draws, mapped per platform and onto the
planned `exp` A2UI catalog (VAPP-5). Per EXP-1174 round 3 this table **is the
catalog list** (VAPP-82, VAPP-83): every row becomes one `exp` catalog
component, rendered on web WITH the `@exp/ui` component in the web column.

Snapshot of 2026-10-04 (master `574bf1023b`). The per-platform `status` tables
in `apps/styleguide/src/components.tsx` stay the live record; this file is the
cross-cut by primitive.

Paths: web `packages/ui/src/` (`@exp/ui`) unless prefixed `web:` (=
`apps/web/src/`); iOS `apps/ios/ExpUI/Sources/` unless prefixed `app:` (=
`apps/ios/Exponential/UI/`); Android `apps/android/app/src/main/java/com/exponential/app/ui/`;
desktop `apps/desktop/crates/ui/src/`.

## Legend

- **GAP-N**: no named primitive in that platform's shared layer (built inline in
  a screen, private to a screen file, or living in the app layer instead of
  `@exp/ui` / ExpUI / `ui/components` / `crates/ui` surface modules).
- **GAP-C**: no catalog entry yet. VAPP-5 as written specifies only `Box`
  (+ style whitelist), the basic-catalog map and `Mascot` (EXP-1174 r1, since
  parked); **every** name in the catalog column is a new entry VAPP-5 must add.
  The column is therefore GAP-C throughout and the mark is omitted per row.
- n/a: the platform deliberately does not draw it.

## Inventory

| # | Primitive | web `@exp/ui` | iOS (ExpUI) | Android | desktop (`crates/ui`) | `exp` catalog | Styleguide id | Gaps |
|---|---|---|---|---|---|---|---|---|
| 1 | Group band (header strip over rows) | `GlassSectionHeader` `glass-rows.tsx` | `GlassSectionBand` `GlassTheme.swift` | `Modifier.glassSectionBand()` `theme/Glass.kt`; `SectionHeader` `components/Scaffolding.kt` | `surface::glass_section_band` / `glass_section_header` | `Band` | `section-header` | — |
| 2 | Hairline-divided flat list (rows under a band) | `SETTINGS_LIST_CLASS` `glass-rows.tsx` | GAP-N: `VStack(spacing: 0)` + `GlassDivider` per site | GAP-N: `GroupDivider` `components/SheetOptionRows.kt` placed by hand | `surface::glass_group_rows_bare` | `RowList` | `divider` | iOS, Android |
| 3 | Group (form fields container) | `GlassGroup` `glass-rows.tsx` | `GlassSection` `GlassTheme.swift` | `Modifier.glassGroup()` `Glass.kt`; `OptionGroup` `SheetOptionRows.kt` | `surface::glass_group` / `glass_group_rows` | `Group` | `group` | — |
| 4 | List row (flat) | `ListRow` `glass-rows.tsx` | `FlatRow` / `.flatRow()` `GlassTheme.swift` | `Modifier.flatRow()` `Glass.kt` | `surface::flat_row` / `flat_row_compact` | `ListRow` | `list-row` | — |
| 5 | Card row (gapped) | `GlassRow` `glass-rows.tsx` | `GlassRow` `GlassTheme.swift` | `Modifier.glassRow()` `Glass.kt` | `surface::glass_row_card` | `CardRow` | `row` | — |
| 6 | Issue row | GAP-N: `web:components/issue-list.tsx` `IssueRow`, `issue-relations-card.tsx` `RelationIssueRow` | GAP-N: private `issueRow()` in `app:Issue/IssueListView.swift`, `app:MyIssues/MyIssuesView.swift`; `RelatedIssueRow` `app:Work/PrGraphBadge.swift` | GAP-N: internal `IssueRow` `issue/IssueListScreen.kt`, `LongPressIssueRow`, `RelationIssueRow` | GAP-N: `issue_list.rs` `render_issue_row`, `issue_relations.rs` `issue_row` | `IssueRow` | none | all four + styleguide |
| 7 | Running row (live run in a list) | GAP-N: `web:components/session-list-rows.tsx` `RunningSessionRow` | GAP-N: `app:Session/RunningSessionRow.swift` (ended twin `EndedRunRow` is in ExpUI) | GAP-N: internal `session/RunningSessionRow.kt` | GAP-N: `run_rows::render_running_run_row` (`pub(crate)`) | `RunRow` | none | all four + styleguide |
| 8 | Pill | `Pill` `pill.tsx` | `GlassPill` `GlassPill.swift` | `GlassPill` `components/GlassPill.kt` | `surface::glass_pill` / `glass_pill_button` / `pill_dot` | `Pill` | `pill` | — |
| 9 | Issue chip (+ stack) | `IssueChip`, `IssueChipStack` `issue-chip.tsx` | `IssueChip` `IssueChip.swift`, `IssueChipStack.swift` | `IssueChip` / `IssueChipStack` `components/IssueChip.kt` | `issue_chip` | `IssueChip` | `issue-chip` | — |
| 10 | Entity chip | `EntityChip` `entity-chip.tsx` | `EntityChip` `EntityChip.swift` | `EntityChip` `components/EntityChip.kt` | `entity_chip` | `EntityChip` | `entity-chip` | — |
| 11 | Label chip | GAP-N: `<Pill dot>` inline (`web:components/issue-list.tsx`) | GAP-N: inline `Circle()` (`app:Issue/IssueListView.swift`), `GlassPill(dot:)` elsewhere | `LabelChip` / `LabelDot` `components/IssueVisuals.kt` | GAP-N: private `label_chip` `issue_list.rs` | `LabelChip` (or `Pill` with `dot`) | covered by `pill` | web, iOS, desktop |
| 12 | Composer subject chips / headline | GAP-N: `web:components/launch-dialog/subject-chips.tsx`, `launch-headline.tsx` | GAP-N (leftover): `AgentComposerHeadline` `app:Agent/AgentComposerCard.swift` | GAP-N (leftover): `AgentComposerHeadline` `agent/AgentComposer.kt` | GAP-N: `chat_launch::headline` (text only) | `ComposerHeadline` | `composer-dialog` | all four |
| 13 | LiveDot | `LiveDot` `live-dot.tsx` | GAP-N: `SessionStateDot` `app:Session/SessionStateDot.swift` (app layer) | GAP-N: internal `LiveDot` `issue/AgentPrCard.kt` | `surface::live_dot` | `LiveDot` | `live-dot` | iOS, Android |
| 14 | StatusGlyph | `StatusGlyph` `status-glyph.tsx` | GAP-N: no view; `AppIcon(status.iconName)` + `IssueColorExtensions.swift` per site | `StatusIcon` / `PriorityIcon` `components/IssueVisuals.kt` | `icons::resolved_status_icon` | `StatusGlyph` | none (shown inside `live-dot`) | iOS + styleguide |
| 15 | IconDisc | `IconDisc` `icon-disc.tsx` | n/a | n/a (onboarding: a 56dp `glassCard` box) | GAP-N: inline in `onboarding.rs` | `IconDisc` | `icon-disc` | desktop (natives n/a by decision) |
| 16 | Glass card | `GlassCard` `glass-card.tsx` | `GlassCard` `GlassTheme.swift` | `Modifier.glassCard()` `Glass.kt` | `surface::glass_card` | `Card` (maps basic `Card`) | `glass-card` | — |
| 17 | Composer box | `Composer` / `ComposerTool` / `ComposerSubmit` `composer.tsx` | `GlassComposer` `GlassComposer.swift` | `GlassComposer` `components/GlassComposer.kt` | `composer::glass_composer` | `Composer` | `composer` | — |
| 18 | Results tiles | `SessionResultsView` `session-results-view.tsx` (tile private: `ResultTile`) | GAP-N: `app:Work/SessionResultsFace.swift` (private `SessionResultTile`) | GAP-N: `work/ResultsFace.kt` (private `ResultTile`) | GAP-N: `session_results::tile` (module-private) | `ResultTile` (+ `ResultGroup` per topic) | `session-results` | tile private ×4; iOS/Android face in app layer |
| 19 | Question / decision card (agent ask, plan) | GAP-N: private `AskCard` / `QuestionCard` `web:components/agent-session.tsx` | GAP-N: private `QuestionCard` `app:Session/AgentSessionView.swift` | GAP-N: private `QuestionCard` / `AnsweredAskCard` `session/AgentSessionScreen.kt` | GAP-N: `steer_viewer.rs` `render_ask` | `DecisionCard` | `markdown` (misfiled) | all four |
| 20 | Choice dialog (blocked start, stack merge) | GAP-N: `web:components/blocked-start-dialog.tsx`, `stack-merge-choice-dialog.tsx` on `Dialog` | GAP-N: `app:Agent/BlockedStartSheet.swift`, `app:Work/WorkMergePill.swift` | GAP-N: in `agent/AgentScreen.kt`, `issue/ChangesScreen.kt` | GAP-N: `chat_screen.rs`, `pr_merge.rs` on `native_dialog::DialogShell` | `ChoiceDialog` (fixture-driven: `blocked-start.json`, `stack-merge-choice.json`) | `blocked-start-dialog`, `stack-merge-choice-dialog` | all four (shared spec = fixtures only) |
| 21 | Toast | `Toaster`, `ToastKindIcon` `toast.tsx` | `Toaster` / `ToastCard` / `ToastHost` `Toast.swift` | `Toaster` / `ToastCard` / `ToastHost` `components/Toast.kt` | `toast::Toast` / `toast::show` | bridge `harness.toast` (not a tree node) | `toast` | — |
| 22 | Picker (core list + trigger) | `Picker`, `PickerList`, `PickerTrigger`, `PickerItemBody` `picker/` | `GlassPicker`, `PickerItem` `Picker/Picker.swift` | `Picker`, `PickerItemBody` `components/picker/Picker.kt` | `picker::Picker`, `picker_item_body` | `Picker` (options inline or bound) | `picker` | — |
| 23 | Typed pickers (status, priority, label, assignee, board, device, account, action, issue, icon, MCP) | `picker/*-picker.tsx` (11) | `Picker/*Picker.swift` (8 + shared account/icon; no MCP) | `components/picker/*Picker.kt` (9; no icon/MCP) | `picker/*_picker.rs` (11) | `Picker` + `source` binding (`exp:statuses`, …) | `picker-*` | iOS MCP; Android icon, MCP |
| 24 | Picker row (closed picker in a form) | `Combobox triggerVariant="row"`, `GLASS_PICKER_ROW` | `GlassPickerRow` `GlassOptionRows.swift` | `PickerRow` `SheetOptionRows.kt` | `surface::glass_picker_row` | `PickerRow` | `picker-row`, `row-shell` | — |
| 25 | Property row | `PropertyRow` `property-row.tsx` | `GlassMetaRow` `GlassMetaRow.swift` | `MetaRow` `SheetOptionRows.kt` | n/a (properties = header chips) | `PropertyRow` | `property-row` | — |
| 26 | Drawer / nav rows | `SidebarMenuButton` `sidebar.tsx` (compositions `web:components/team/list-nav.tsx`, `sidebar.tsx`) | GAP-N: private `boardRow` / `plainActionRow` `app:Home/BoardSwitcherSheet.swift` | `BoardRow` `components/BoardRow.kt`; private `MutedActionRow` | GAP-N: private `rail_row` / `rail_row_lead` `sidebar.rs` | `NavRow` | `list-row` (compact) | iOS, desktop; no own styleguide id |

### Further shared primitives (already named on every platform unless marked)

| Primitive | web | iOS | Android | desktop | `exp` catalog | Gaps |
|---|---|---|---|---|---|---|
| Buttons (icon, ghost icon, primary, text) | `button.tsx` | `CircleIconButton`, `GhostIconButton`, `GlassSubmitButton` `GlassControls.swift` | `CircleIconButton.kt`, `GlassSubmitButton.kt` | `controls::glass_icon_button`, `ghost_icon_button`, `text_button`, `surface::glass_pill_button_primary` | `Button` (`variant`) | iOS text button (leftover) |
| Text field / textarea / search | `input.tsx`, `textarea.tsx`, `GlassInputRow`, `search-field.tsx` | `GlassTextField`, `GlassSheetSearchField` | `GlassTextField.kt`, `GlassSheetSearchField` | `controls::glass_input`, `web_textarea`, `search_field` | `TextField` (`multiline`, `search`) | — |
| Switch / toggle row | `switch.tsx`, `GlassToggleRow` | `GlassToggleStyle` | `SwitchRow`, `glassSwitchColors` | `controls::web_switch`, `surface::glass_toggle_row` | `Toggle` | — |
| Segmented / tabs | `segmented-control.tsx`, `GlassTabsRow`, `work-face-strip.tsx`, `rich-tab.tsx` | `GlassSegmentedControl` | `GlassSegmentedControl` | `controls::segmented`, `surface::glass_tabs_row` | `Segmented`, `Tabs` | — |
| Avatars | `UserAvatar`, `TeamAvatar` | same | same (`Avatars.kt`) | `user_avatar.rs` | `Avatar` | — |
| Menu | `menu-surface.ts` | `GlassMenuTokens` | `GlassMenuDefaults` | GAP-N (gpui-component `PopupMenu`) | `Menu` | desktop |
| Sheet | `sheet.tsx` | `GlassSheetChrome` | `GlassSheet` | n/a | `Sheet` (surface) | — |
| Empty state | `EmptyState`, `ListEmpty`, `EmptyCta` `empty-state.tsx` | GAP-N (leftover `InboxView.emptyState`) | GAP-N (leftover `Scaffolding.kt` `EmptyState`) | `controls::empty_state` | `EmptyState` | iOS, Android |
| Meters / rings | `meter.tsx`, `segmented-bar.tsx`, `context-ring.tsx`, `progress-ring.tsx` | `UsageTrack`, `SegmentedTrack`, `ContextRing`, `ProgressRing` | `UsageTrack`, `SegmentedTrack`, `ContextRing`; `SubIssueProgressRing` private in `issue/RelationsSection.kt` | `usage_bar::meter`, `usage_sheet::context_ring`, progress inside `issue_relations::render_sub_issues` | `Meter`, `Ring` | Android progress ring, desktop rings not shared |
| Badge | `badge.tsx` | GAP-N (inlined in segmented) | GAP-N (inlined in segmented) | `surface::count_badge` | `Badge` | iOS, Android |
| Issue group band (status group header) | `IssueGroupBand` `issue-group-band.tsx` | GAP-N (`IssueListView.statusHeader`) | GAP-N (`MyIssuesScreen.kt` `GroupHeader`) | `IssueListView::render_group_header` | `Band` (`variant: group`) | iOS, Android |
| Disclosure header | `DisclosureHeader` `disclosure-header.tsx` | GAP-N (private `ToolGroupRow`) | GAP-N (private `ToolGroupRow`) | `controls::disclosure_header` | `Disclosure` | iOS, Android |
| Tree guides | `tree-guides-view.tsx` | `TreeGuidesOverlay` | `TreeGuidesOverlay.kt` | `tree_guides.rs` | `TreeGuides` (on `RowList`) | — |
| Diff card / file tree | `file-diff-card.tsx`, `edited-files-card.tsx`, `file-diff-tree.tsx` | `DiffFileCard`, `DiffFileTree`, `EditedFilesCard` (app layer) | same (`ui/issue`, `ui/session`) | `diff::render_file_card`, `diff_pane::file_tree` | `Diff` (wrapped native) | iOS/Android in app layer |
| Markdown view | TipTap / renderer | cmark-gfm | commonmark-java | comrak + `gpui-markdown-editor` | `Markdown` | — |

## Gap summary

- **No shared primitive anywhere (×4):** issue row, running row, question/decision
  card, choice dialog body, composer headline. These are exactly the surfaces
  SLOP-16 found composed ad hoc; each gets a named spec in the extraction step.
- **Private tiles:** the results tile exists ×4 but is private in every file.
- **Native-only gaps:** iOS StatusGlyph view, LiveDot in the iOS/Android shared
  layers, the flat hairline list container on iOS/Android, empty state, badge,
  issue group band, disclosure header on iOS/Android, label chip on
  web/iOS/desktop, drawer rows on iOS/desktop, desktop menu.
- **Styleguide gaps:** no specimen id for issue row, running row, StatusGlyph,
  nav row; the ask/plan card sits under `markdown`.
- **Catalog:** VAPP-5 names none of these yet (GAP-C for every row).

## Tokens (SLOP-18 step 2)

`packages/design-tokens/tokens.json` now carries the scales the catalog's `Box`
styles and our surfaces resolve against:

- `spacing`: xxs 2 · xs 4 · sm 8 · md 12 · lg 16 · xl 24 · xl2 32 (px ≡ pt ≡ dp).
- `type.size` / `type.lineHeight`: xs 12/16 · sm 14/20 · base 16/24 · lg 18/28 · xl 20/28 · xl2 24/32.
- `type.weight`: regular 400 · medium 500 · semibold 600 · bold 700.
- `type.emphasis`: primary 1 · secondary 0.7 · tertiary 0.5 · quaternary 0.3 (the iOS `TextOpacity` ladder).

Generated as `DesignTokens.Spacing` / `DesignTokens.Typography` (Swift, Kotlin)
and `tokens::spacing` / `tokens::typography` (Rust); web mirrors them as
`--space-*` / `--type-*` in `packages/ui/src/styles.css`, parity-tested.
Wired without restyling: iOS `TextOpacity`, Android `TextEmphasis` and the
desktop `FONT_SIZE_PX` read the tokens.

Known mismatches for the extraction step to settle:

- 6 and 10 are heavily used on all four clients but are off the scale. They are
  usually the inside of a pill or band, and the row padding on Android.
- iOS sets type in Dynamic Type styles (`.caption` ≈ 12, `.subheadline` ≈ 15,
  `.body` = 17), which do not land on 12/14/16.
- Android uses the stock M3 scale; its body and label sizes already land on
  12/14/16.
- The desktop rem is 14, so gpui helpers scale with it (`.p_2()` = 7px,
  `.text_xs()` = 10.5px). Literal token px are needed where numbers must match
  the other clients.
- Web at md+ scales every rem by 1.156 (the 18.5px root). The tokens are the
  16px-root values.
