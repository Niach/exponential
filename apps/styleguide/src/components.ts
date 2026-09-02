/**
 * The Components group (EXP-698) — the canonical glass control set, written
 * ONCE, in HTML and CSS driven by `@exp/design-tokens`.
 *
 * Screens are screenshots because a screen is a composition nobody can
 * reproduce from a spec. A CONTROL is the opposite: it is a handful of numbers,
 * and a photograph of one is a worse reference than the control itself — it
 * ages, it needs a capture lane, and it cannot be measured. So this group holds
 * no shots at all. Each entry renders the agreed form live and then names, per
 * platform, the ONE symbol and file that is supposed to match it:
 *
 *   - `ok`       that platform's implementation IS this form
 *   - `leftover` it exists, but still disagrees — the `note` says how
 *   - `n/a`      the platform deliberately has no such control
 *
 * The group is SYNTHETIC: `packages/view-catalog`'s `GroupId` union is
 * hand-typed and drift-gated against `views.json`, and adding a group with no
 * views and no shots to it would mean teaching the capture pipeline about a
 * section it can never photograph. It lives here instead, and `--check` never
 * sees it.
 *
 * `components.test.ts` gates the parts that rot: every named file exists, every
 * platform is accounted for, and the stylesheet contains no colour literal.
 */

import { designTokens } from "@exp/design-tokens"

import {
  escapeHtml,
  svgBell,
  svgChevronRight,
  svgEllipsis,
  svgGitMerge,
  svgInbox,
  svgPlay,
  svgPlus,
  svgTrash,
} from "./html.ts"

export type ComponentPlatform = `web` | `desktop` | `ios` | `android`

export const COMPONENT_PLATFORMS: readonly ComponentPlatform[] = [
  `web`,
  `desktop`,
  `ios`,
  `android`,
]

/**
 * `leftover` is the whole point of the table: a control that exists on a
 * platform but has not been brought onto the canonical form yet. Silence would
 * read as agreement.
 */
export type ComponentState = `ok` | `leftover` | `n/a`

export interface ComponentStatus {
  state: ComponentState
  /** The component / function / modifier that IS this control on that platform. */
  symbol?: string
  /** Repo-relative path holding it. Gated for existence by the test. */
  file?: string
  /** One line, ≤120 chars: why it is a leftover, or why the platform has none. */
  note?: string
}

export interface ComponentSpec {
  id: string
  title: string
  kind: `Grouped list` | `Controls` | `Surfaces` | `Tokens`
  blurb: string
  status: Record<ComponentPlatform, ComponentStatus>
  /** Pure HTML using only `.cmp-*` classes — never a `style=` attribute. */
  render: () => string
}

export const COMPONENTS_GROUP = {
  id: `components`,
  label: `Components`,
  blurb: `The glass control set, rendered from the design tokens — not photographed.`,
} as const

const { glass, radius, size, motion } = designTokens

/* ------------------------------------------------------------------ parts */

function sectionHeader(title: string, trailing?: string): string {
  return [
    `<div class="cmp-section-header">`,
    `<span class="title">${escapeHtml(title)}</span>`,
    trailing === undefined ? `` : `<span class="trailing">${trailing}</span>`,
    `</div>`,
  ].join(``)
}

function group(...children: string[]): string {
  return `<div class="cmp-group">${children.join(``)}</div>`
}

function pickerRow(label: string, value: string): string {
  return [
    `<div class="cmp-row-shell">`,
    `<span class="label">${escapeHtml(label)}</span>`,
    `<span class="value">${escapeHtml(value)}</span>`,
    `<span class="chevron">${svgChevronRight}</span>`,
    `</div>`,
  ].join(``)
}

function inputRow(label: string, placeholder: string): string {
  return [
    `<div class="cmp-row-shell">`,
    `<span class="label">${escapeHtml(label)}</span>`,
    `<input class="value" type="text" placeholder="${escapeHtml(placeholder)}" readonly>`,
    `</div>`,
  ].join(``)
}

function toggleRow(label: string, desc: string | undefined, on: boolean): string {
  const text =
    desc === undefined
      ? `<span class="label">${escapeHtml(label)}</span>`
      : `<span class="text"><span class="label">${escapeHtml(label)}</span><span class="desc">${escapeHtml(desc)}</span></span>`
  return [
    `<div class="cmp-row-shell">`,
    text,
    `<span class="cmp-switch${on ? ` on` : ``}"></span>`,
    `</div>`,
  ].join(``)
}

function tabsRow(labels: string[], activeIndex: number): string {
  const tabs = labels
    .map(
      (label, at) =>
        `<span class="tab${at === activeIndex ? ` active` : ``}">${escapeHtml(label)}</span>`
    )
    .join(``)
  return `<div class="cmp-tabs-row">${tabs}</div>`
}

function iconButton(glyph: string): string {
  return `<button class="cmp-icon-button" type="button">${glyph}</button>`
}

function buttonXs(label: string, glyph?: string): string {
  return `<button class="cmp-button-xs" type="button">${glyph ?? ``}${escapeHtml(label)}</button>`
}

function row(label: string, trailing?: string, interactive = false): string {
  return [
    `<div class="cmp-row${interactive ? ` interactive` : ``}">`,
    `<span class="label">${escapeHtml(label)}</span>`,
    trailing === undefined ? `` : `<span class="trailing">${trailing}</span>`,
    `</div>`,
  ].join(``)
}

function swatch(name: string, value: string, box: string): string {
  return [
    `<div class="cmp-swatch">`,
    `<span class="box ${box}"></span>`,
    `<span class="name">${escapeHtml(name)}</span>`,
    `<span class="value">${escapeHtml(value)}</span>`,
    `</div>`,
  ].join(``)
}

function motionLine(label: string, box: string): string {
  return [
    `<div class="line">`,
    `<span class="label">${escapeHtml(label)}</span>`,
    `<span class="track"><span class="box ${box}"></span></span>`,
    `</div>`,
  ].join(``)
}

/* ---------------------------------------------------------------- statuses */

function ok(symbol: string, file: string, note?: string): ComponentStatus {
  return { state: `ok`, symbol, file, note }
}

function leftover(symbol: string, file: string, note: string): ComponentStatus {
  return { state: `leftover`, symbol, file, note }
}

function na(note: string): ComponentStatus {
  return { state: `n/a`, note }
}

const WEB_GLASS_ROWS = `apps/web/src/components/ui/glass-rows.tsx`
const DESKTOP_SURFACE = `apps/desktop/crates/ui/src/surface.rs`
const DESKTOP_CONTROLS = `apps/desktop/crates/ui/src/controls.rs`
const IOS_THEME = `apps/ios/ExpUI/Sources/GlassTheme.swift`
const IOS_CONTROLS = `apps/ios/ExpUI/Sources/GlassControls.swift`
const IOS_OPTION_ROWS = `apps/ios/ExpUI/Sources/GlassOptionRows.swift`
const IOS_SEGMENTED = `apps/ios/ExpUI/Sources/GlassSegmentedControl.swift`
const ANDROID_GLASS = `apps/android/app/src/main/java/com/exponential/app/ui/theme/Glass.kt`
const ANDROID_SHEET_ROWS = `apps/android/app/src/main/java/com/exponential/app/ui/components/SheetOptionRows.kt`
const ANDROID_COMPONENTS = `apps/android/app/src/main/java/com/exponential/app/ui/components`

const HEADER_EXCEPTION = `Emoji picker category headers stay uppercase on purpose (shared exception).`

/* -------------------------------------------------------------- the specs */

export const COMPONENTS: readonly ComponentSpec[] = [
  {
    id: `section-header`,
    title: `Section header`,
    kind: `Grouped list`,
    blurb: `Sentence case, 14/20 at 70% foreground, with a trailing slot. No count — EXP-698 retired header counts on every client. Never uppercase and never a divider.`,
    status: {
      web: ok(`GlassSectionHeader`, WEB_GLASS_ROWS, HEADER_EXCEPTION),
      desktop: ok(`surface::glass_section_header`, DESKTOP_SURFACE, HEADER_EXCEPTION),
      ios: ok(`GlassSectionHeader`, `apps/ios/ExpUI/Sources/GlassTheme.swift`, HEADER_EXCEPTION),
      android: ok(`SectionHeader`, `${ANDROID_COMPONENTS}/Scaffolding.kt`, HEADER_EXCEPTION),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        sectionHeader(`Boards`, buttonXs(`New`, svgPlus)),
        group(pickerRow(`Mobile app`, `24 issues`), pickerRow(`Website`, `9 issues`)),
        sectionHeader(`Danger zone`),
        `</div>`,
      ].join(``),
  },
  {
    id: `group`,
    title: `Group container`,
    kind: `Grouped list`,
    blurb: `Borderless: radius 12, the row fill, hairline separators between children, overflow hidden. The fill is the edge — no outer stroke.`,
    status: {
      web: ok(`GlassGroup`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_group / glass_group_rows`, DESKTOP_SURFACE),
      ios: ok(`GlassSection`, IOS_THEME),
      android: ok(`Modifier.glassGroup()`, ANDROID_GLASS, `OptionGroup in ui/components/SheetOptionRows.kt is the list wrapper around it.`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        sectionHeader(`Board`),
        group(
          pickerRow(`Repository`, `niach/exponential`),
          inputRow(`Slug`, `mobile-app`),
          toggleRow(`Archived`, undefined, false)
        ),
        `</div>`,
      ].join(``),
  },
  {
    id: `row`,
    title: `Glass row`,
    kind: `Grouped list`,
    blurb: `The GAPPED list item: radius 10, row fill, its own hairline border, padding 12. Interactive rows lighten to half the active fill on hover.`,
    status: {
      web: ok(`GlassRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_row_card`, DESKTOP_SURFACE),
      ios: ok(`GlassRow`, IOS_THEME),
      android: ok(`Modifier.glassRow()`, ANDROID_GLASS),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        row(`APP-14 · Fix the merge queue`, `<span class="cmp-chip">in review</span>`, true),
        row(`APP-15 · Ship the usage sheet`, `<span class="cmp-chip">backlog</span>`, true),
        `</div>`,
      ].join(``),
  },
  {
    id: `row-shell`,
    title: `Row shell`,
    kind: `Grouped list`,
    blurb: `The rhythm every grouped row inherits: padding 12/16, gap 12, 14px text. The shell never draws a stroke — the group's hairlines do.`,
    status: {
      web: ok(`GlassInputRow / GlassToggleRow / GlassPickerRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_row_shell`, DESKTOP_SURFACE),
      ios: ok(`GlassPickerRow`, IOS_OPTION_ROWS),
      android: ok(`PickerRow`, ANDROID_SHEET_ROWS),
    },
    render: () =>
      group(
        pickerRow(`Agent`, `claude`),
        inputRow(`Branch prefix`, `exp/`),
        toggleRow(`Plan first`, `Ask before it writes`, true)
      ),
  },
  {
    id: `picker-row`,
    title: `Picker row`,
    kind: `Grouped list`,
    blurb: `Label left, value right-aligned at 70% foreground, a 14px chevron at 50%. The whole row is the target, never just the value.`,
    status: {
      web: ok(`GlassPickerRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_picker_row`, DESKTOP_SURFACE),
      ios: ok(`GlassPickerRow`, IOS_OPTION_ROWS),
      android: ok(`PickerRow`, ANDROID_SHEET_ROWS),
    },
    render: () =>
      group(
        pickerRow(`Status`, `In review`),
        pickerRow(`Assignee`, `Danny`),
        pickerRow(`Due date`, `No date`)
      ),
  },
  {
    id: `input-row`,
    title: `Input row`,
    kind: `Grouped list`,
    blurb: `A bare right-aligned field at 70% foreground inside the shell — no box, no border. The row is the field's chrome.`,
    status: {
      web: ok(`GlassInputRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_input_row`, DESKTOP_SURFACE),
      ios: na(`Form text rows use the system field inside GlassSection`),
      android: ok(`GlassTextField(bordered = false)`, `${ANDROID_COMPONENTS}/GlassTextField.kt`),
    },
    render: () => group(inputRow(`Name`, `Mobile app`), inputRow(`Prefix`, `APP`)),
  },
  {
    id: `toggle-row`,
    title: `Toggle row`,
    kind: `Grouped list`,
    blurb: `Label, an optional 12px description at 50%, and a 36×20 switch: on is the primary track with a primary-foreground thumb, off the active fill.`,
    status: {
      web: ok(`GlassToggleRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_toggle_row`, DESKTOP_SURFACE),
      ios: ok(`GlassToggleStyle`, IOS_CONTROLS),
      android: ok(`SwitchRow`, ANDROID_SHEET_ROWS),
    },
    render: () =>
      group(
        toggleRow(`Helpdesk`, `Reporters can reply by email`, true),
        toggleRow(`Widget`, undefined, false)
      ),
  },
  {
    id: `tabs-row`,
    title: `Embedded tabs row`,
    kind: `Grouped list`,
    blurb: `The segmented control as the FIRST row of a group: padding 8, full width, no fill and no stroke of its own.`,
    status: {
      web: ok(`GlassTabsRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_tabs_row`, DESKTOP_SURFACE),
      ios: ok(`GlassSegmentedControl(style: .embedded)`, IOS_SEGMENTED),
      android: ok(`GlassSegmentedControl(embedded = true)`, `${ANDROID_COMPONENTS}/GlassSegmentedControl.kt`),
    },
    render: () =>
      group(
        tabsRow([`Open`, `Merged`, `All`], 0),
        pickerRow(`APP-14`, `2 files`),
        pickerRow(`APP-21`, `7 files`)
      ),
  },
  {
    id: `segmented`,
    title: `Segmented control`,
    kind: `Controls`,
    blurb: `The standalone capsule: 36 tall, padding 3, the section fill under a section stroke. Segments share the embedded row's geometry.`,
    status: {
      web: ok(`TabsList`, `apps/web/src/components/ui/tabs.tsx`),
      desktop: ok(`controls::segmented`, DESKTOP_CONTROLS),
      ios: ok(`GlassSegmentedControl`, IOS_SEGMENTED),
      android: ok(`GlassSegmentedControl`, `${ANDROID_COMPONENTS}/GlassSegmentedControl.kt`),
    },
    render: () =>
      [
        `<div class="cmp-segmented">`,
        `<span class="tab active">Issues</span>`,
        `<span class="tab">Actions</span>`,
        `<span class="tab">Automations</span>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `icon-button`,
    title: `Glass icon button`,
    kind: `Controls`,
    blurb: `A 32px circle of card fill under a card stroke, glyph 16px at 70% foreground; hover fills to active and the glyph goes full strength.`,
    status: {
      web: ok(`buttonVariants variant="glass" size="icon-sm"`, `apps/web/src/components/ui/button.tsx`),
      desktop: ok(`controls::glass_icon_button`, DESKTOP_CONTROLS),
      ios: ok(`CircleIconButton`, IOS_CONTROLS),
      android: ok(`CircleIconButton`, `${ANDROID_COMPONENTS}/CircleIconButton.kt`),
    },
    render: () =>
      row(`Nightly changelog`, `${iconButton(svgPlay)}${iconButton(svgEllipsis)}`),
  },
  {
    id: `button-xs`,
    title: `Header button`,
    kind: `Controls`,
    blurb: `The 24-tall capsule that sits in a section header's trailing slot: 12/16 medium at 70% foreground, a card hairline over the card fill; hover goes active.`,
    status: {
      web: ok(`buttonVariants variant="glass" size="xs"`, `apps/web/src/components/ui/button.tsx`),
      desktop: ok(`WebControl::web_xs`, DESKTOP_CONTROLS),
      ios: na(`sheet header actions are GlassPillButton`),
      android: na(`sheet header actions are GlassSheetHeaderAction`),
    },
    render: () =>
      [
        `<div class="cmp-inline">`,
        buttonXs(`New board`, svgPlus),
        buttonXs(`Merge`, svgGitMerge),
        buttonXs(`Manage`),
        `</div>`,
      ].join(``),
  },
  {
    id: `button-primary`,
    title: `Primary submit`,
    kind: `Controls`,
    blurb: `Full width, padding 14/16, radius 10, solid primary. Disabled drops to card fill with a card stroke and 50% foreground.`,
    status: {
      web: ok(
        `buttonVariants (default)`,
        `apps/web/src/components/ui/button.tsx`,
        `web/desktop primaries stay capsules; the radius-10 full-width form is the mobile sheet submit`
      ),
      desktop: ok(`Button::primary()`, DESKTOP_CONTROLS),
      ios: ok(`GlassSubmitButton`, IOS_CONTROLS),
      android: ok(`GlassSubmitButton`, `${ANDROID_COMPONENTS}/GlassPillButton.kt`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<button class="cmp-button-primary" type="button">Create issue</button>`,
        `<button class="cmp-button-primary disabled" type="button">Create issue</button>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `pill`,
    title: `Pill`,
    kind: `Controls`,
    blurb: `The selectable capsule: card fill, card stroke, 12px medium. Selected swaps to the active fill and the active stroke.`,
    status: {
      web: ok(`buttonVariants variant="glass" size="sm"`, `apps/web/src/components/ui/button.tsx`),
      desktop: ok(`surface::glass_pill`, DESKTOP_SURFACE),
      ios: ok(`GlassPillButton`, IOS_CONTROLS),
      android: ok(`GlassPillButton`, `${ANDROID_COMPONENTS}/GlassPillButton.kt`),
    },
    render: () =>
      [
        `<div class="cmp-inline">`,
        `<button class="cmp-pill active" type="button">All</button>`,
        `<button class="cmp-pill" type="button">Assigned to me</button>`,
        `<button class="cmp-pill" type="button">${svgPlus}Filter</button>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `chip`,
    title: `Chip`,
    kind: `Controls`,
    blurb: `Static metadata, never a target: radius 8, card fill, NO stroke, padding 4/8 at 12px medium.`,
    status: {
      web: leftover(
        `Badge`,
        `apps/web/src/components/ui/badge.tsx`,
        `capsule, not radius 8`
      ),
      desktop: ok(`surface::glass_chip`, DESKTOP_SURFACE),
      ios: ok(`GlassChip`, IOS_CONTROLS),
      android: ok(`Modifier.glassChip()`, ANDROID_GLASS),
    },
    render: () =>
      [
        `<div class="cmp-inline">`,
        `<span class="cmp-chip">in review</span>`,
        `<span class="cmp-chip">bug</span>`,
        `<span class="cmp-chip">APP-14</span>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `text-field`,
    title: `Text field`,
    kind: `Controls`,
    blurb: `36 tall, padding 0/12, radius 12, card fill under a card stroke; focus swaps the stroke to active — no ring. Placeholder at 50%.`,
    status: {
      web: leftover(
        `Input`,
        `apps/web/src/components/ui/input.tsx`,
        `radius 10 / fillRow / ring focus vs 12 / fillCard / strokeActive`
      ),
      desktop: leftover(`WebControl::web_input`, DESKTOP_CONTROLS, `theme input chrome`),
      ios: ok(`GlassTextField`, IOS_CONTROLS),
      android: ok(`GlassTextField`, `${ANDROID_COMPONENTS}/GlassTextField.kt`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<input class="cmp-text-field" type="text" value="Fix the merge queue">`,
        `<input class="cmp-text-field" type="text" placeholder="Search issues">`,
        `</div>`,
      ].join(``),
  },
  {
    id: `sheet`,
    title: `Sheet shell`,
    kind: `Surfaces`,
    blurb: `Top radius 24 over the page's bottom gradient, a card hairline, a 36×4 grabber. Header gutter 20, content gutter 16.`,
    status: {
      web: ok(`SheetContent side="bottom"`, `apps/web/src/components/ui/sheet.tsx`),
      desktop: na(`dialogs are OS windows`),
      ios: ok(`GlassSheetChrome + GlassSheetTokens`, `apps/ios/ExpUI/Sources/GlassSheet.swift`),
      android: ok(`GlassSheet + GlassSheetDefaults`, `${ANDROID_COMPONENTS}/GlassSheet.kt`),
    },
    render: () =>
      [
        `<div class="cmp-sheet">`,
        `<div class="grabber"></div>`,
        `<div class="header"><span class="title">New issue</span><span class="trailing">${buttonXs(`Cancel`)}</span></div>`,
        `<div class="content">`,
        group(pickerRow(`Board`, `Mobile app`), pickerRow(`Status`, `Backlog`)),
        `<button class="cmp-button-primary" type="button">Create</button>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `menu`,
    title: `Menu surface`,
    kind: `Surfaces`,
    blurb: `180–280 wide, padding 4, radius 12. Opaque by construction: the card fill is composited over the popover solid so nothing shows through.`,
    status: {
      web: leftover(
        `DropdownMenuContent`,
        `apps/web/src/components/ui/dropdown-menu.tsx`,
        `rounded-xl + glass-panel, items ~32px`
      ),
      desktop: na(`gpui-component PopupMenu, theme-driven`),
      ios: ok(`GlassMenu + GlassMenuTokens`, `apps/ios/ExpUI/Sources/GlassMenu.swift`),
      android: ok(`GlassDropdownMenu + GlassMenuDefaults`, `${ANDROID_COMPONENTS}/GlassMenu.kt`),
    },
    render: () =>
      [
        `<div class="cmp-menu">`,
        `<div class="item">${svgPlay}Start coding</div>`,
        `<div class="item">${svgGitMerge}Open pull request</div>`,
        `<div class="divider"></div>`,
        `<div class="item destructive">${svgTrash}Delete issue</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `tab-bar`,
    title: `Bottom tab bar`,
    kind: `Surfaces`,
    blurb: `A floating capsule: padding 4 inside a strong hairline, over the OPAQUE card fill. Items are 44px circles; the active one takes the active fill.`,
    status: {
      web: ok(`MobileTabBar`, `apps/web/src/components/team/mobile-tab-bar.tsx`),
      desktop: na(`no bottom bar`),
      ios: ok(`MobileTabBar`, `apps/ios/Exponential/UI/Navigation/MobileTabBar.swift`),
      android: ok(`BottomNavBar`, `${ANDROID_COMPONENTS}/BottomNavBar.kt`),
    },
    render: () =>
      [
        `<div class="cmp-tab-bar">`,
        `<span class="item active">${svgInbox}</span>`,
        `<span class="item">${svgPlus}</span>`,
        `<span class="item">${svgGitMerge}</span>`,
        `<span class="item">${svgBell}</span>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `usage-bar`,
    title: `Usage bar`,
    kind: `Surfaces`,
    blurb: `A label/amount line above a 6px capsule track in the strong stroke; the fill is foreground at 30%, or the yellow semantic once it is nearly spent.`,
    status: {
      web: leftover(
        `AgentUsageCards`,
        `apps/web/src/components/agent-usage-bar.tsx`,
        `track border/60 + fill muted/60, not strokeStrong + fg 30%`
      ),
      desktop: leftover(
        `render_usage_cards`,
        `apps/desktop/crates/ui/src/usage_bar.rs`,
        `fill is 35% fg, untokenized (canonical 30%)`
      ),
      ios: leftover(
        `AgentUsageCardRow`,
        `apps/ios/Exponential/UI/Session/AgentUsageCards.swift`,
        `fill is 35% fg, untokenized (canonical 30%)`
      ),
      android: leftover(
        `UsageTrack`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentUsageBar.kt`,
        `fill is 35% fg, untokenized (canonical 30%)`
      ),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<div class="cmp-usage-bar">`,
        `<div class="line"><span class="label">Session</span><span class="amount">62%</span></div>`,
        `<div class="track"><div class="fill"></div></div>`,
        `</div>`,
        `<div class="cmp-usage-bar warn">`,
        `<div class="line"><span class="label">Weekly</span><span class="amount">88%</span></div>`,
        `<div class="track"><div class="fill"></div></div>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `divider`,
    title: `Hairline divider`,
    kind: `Surfaces`,
    blurb: `One pixel of the row stroke. The only rule allowed inside a group, and the only one anywhere in the glass set.`,
    status: {
      web: ok(`GlassGroup (divide-glass-stroke)`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_row_divider`, DESKTOP_SURFACE),
      ios: ok(`GlassDivider`, IOS_THEME),
      android: ok(`GroupDivider`, ANDROID_SHEET_ROWS),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<span class="cmp-chip">above</span>`,
        `<div class="cmp-divider"></div>`,
        `<span class="cmp-chip">below</span>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-fills`,
    title: `Fills`,
    kind: `Tokens`,
    blurb: `The four white-alpha fills, on the page gradient they are designed against. Section under row under card under active — never a fifth step.`,
    status: {
      web: ok(`--glass-fill-*`, `apps/web/src/styles.css`),
      desktop: ok(`theme::glass::FILL_*`, `apps/desktop/crates/theme/src/tokens.generated.rs`),
      ios: ok(`GlassTokens`, `apps/ios/ExpUI/Sources/GlassTokens.swift`),
      android: ok(`GlassTokens`, ANDROID_GLASS, `aliases of DesignTokens.generated.kt`),
    },
    render: () =>
      [
        `<div class="cmp-swatches">`,
        swatch(`fillSection`, glass.fillSection, `fill-section`),
        swatch(`fillRow`, glass.fillRow, `fill-row`),
        swatch(`fillCard`, glass.fillCard, `fill-card`),
        swatch(`fillActive`, glass.fillActive, `fill-active`),
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-strokes`,
    title: `Strokes`,
    kind: `Tokens`,
    blurb: `Five hairlines. Row separates, section and card enclose, strong floats, active marks a selection — pick by JOB, never by contrast.`,
    status: {
      web: ok(`--glass-stroke-*`, `apps/web/src/styles.css`),
      desktop: ok(`theme::glass::STROKE_*`, `apps/desktop/crates/theme/src/tokens.generated.rs`),
      ios: ok(`GlassTokens`, `apps/ios/ExpUI/Sources/GlassTokens.swift`),
      android: ok(`GlassTokens`, ANDROID_GLASS, `aliases of DesignTokens.generated.kt`),
    },
    render: () =>
      [
        `<div class="cmp-swatches">`,
        swatch(`strokeRow`, glass.strokeRow, `stroke-row`),
        swatch(`strokeSection`, glass.strokeSection, `stroke-section`),
        swatch(`strokeCard`, glass.strokeCard, `stroke-card`),
        swatch(`strokeStrong`, glass.strokeStrong, `stroke-strong`),
        swatch(`strokeActive`, glass.strokeActive, `stroke-active`),
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-radius`,
    title: `Radius ladder`,
    kind: `Tokens`,
    blurb: `Six steps. Row 10, group and field 12, card 16, sheet 24 — anything else is a mistake, and capsules use 9999 rather than a step.`,
    status: {
      web: ok(`--radius`, `apps/web/src/styles.css`),
      desktop: ok(`theme::radius::*`, `apps/desktop/crates/theme/src/tokens.generated.rs`),
      ios: ok(`GlassTokens`, `apps/ios/ExpUI/Sources/GlassTokens.swift`),
      android: ok(`GlassTokens`, ANDROID_GLASS, `aliases of DesignTokens.generated.kt`),
    },
    render: () =>
      [
        `<div class="cmp-radius">`,
        ...(
          [
            [`sm`, radius.sm],
            [`md`, radius.md],
            [`lg`, radius.lg],
            [`xl`, radius.xl],
            [`xl2`, radius.xl2],
            [`xl3`, radius.xl3],
          ] as const
        ).map(
          ([name, value]) =>
            `<div class="step"><span class="box r-${name}"></span><span class="label">${name} · ${value}</span></div>`
        ),
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-size`,
    title: `Control heights`,
    kind: `Tokens`,
    blurb: `Three control heights plus the field and the list row. A control that is none of these is a control nobody agreed to.`,
    status: {
      web: ok(`buttonVariants size-9 / h-8 / h-6`, `apps/web/src/components/ui/button.tsx`),
      desktop: ok(`theme::size::*`, `apps/desktop/crates/theme/src/tokens.generated.rs`),
      ios: ok(`GlassTokens`, `apps/ios/ExpUI/Sources/GlassTokens.swift`),
      android: ok(`GlassTokens`, ANDROID_GLASS, `aliases of DesignTokens.generated.kt`),
    },
    render: () =>
      [
        `<div class="cmp-size">`,
        ...(
          [
            [`controlLg`, size.controlLg, `size-ctl-lg`],
            [`controlMd`, size.controlMd, `size-ctl-md`],
            [`controlSm`, size.controlSm, `size-ctl-sm`],
            [`inputHeight`, size.inputHeight, `size-input`],
            [`rowHeight`, size.rowHeight, `size-row`],
          ] as const
        ).map(
          ([name, value, cls]) =>
            `<div class="line"><span class="label">${name}</span><span class="bar ${cls}"></span><span class="value">${value}</span></div>`
        ),
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-motion`,
    title: `Motion`,
    kind: `Tokens`,
    blurb: `Three durations against three easings — hover a line to run it. Fast for micro-feedback, standard for most transitions, slow for whole surfaces.`,
    status: {
      web: ok(`--motion-*`, `apps/web/src/styles.css`),
      desktop: ok(`theme::motion::duration / ease`, `apps/desktop/crates/theme/src/tokens.generated.rs`),
      ios: ok(`GlassTokens`, `apps/ios/ExpUI/Sources/GlassTokens.swift`),
      android: ok(`GlassTokens`, ANDROID_GLASS, `aliases of DesignTokens.generated.kt`),
    },
    render: () => {
      const durations = [
        [`fast`, motion.duration.fast, `dur-fast`],
        [`standard`, motion.duration.standard, `dur-standard`],
        [`slow`, motion.duration.slow, `dur-slow`],
      ] as const
      const eases = [
        [`standard`, `ease-standard`],
        [`decelerate`, `ease-decelerate`],
        [`accelerate`, `ease-accelerate`],
      ] as const
      const lines: string[] = []
      for (const [durName, ms, durClass] of durations) {
        for (const [easeName, easeClass] of eases) {
          lines.push(motionLine(`${durName} ${ms} · ${easeName}`, `${durClass} ${easeClass}`))
        }
      }
      return `<div class="cmp-motion">${lines.join(``)}</div>`
    },
  },
]
