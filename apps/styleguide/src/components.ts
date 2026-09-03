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
  svgCheck,
  svgChevronRight,
  svgCircleHelp,
  svgCircleUser,
  svgEllipsis,
  svgFlag,
  svgGitMerge,
  svgHash,
  svgImage,
  svgInbox,
  svgListTodo,
  svgPaperclip,
  svgPlay,
  svgPlus,
  svgSend,
  svgSmile,
  svgTag,
  svgTerminal,
  svgTrash,
  svgX,
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

/**
 * The selection bar. `labelled` is the desktop arm (icon + text on every
 * button); the phone arm keeps only the glyphs, and only Start coding keeps
 * its words.
 */
function bulkBar(labelled: boolean): string {
  const button = (glyph: string, label: string, destructive = false): string =>
    [
      `<span class="item${destructive ? ` destructive` : ``}">`,
      glyph,
      labelled ? `<span class="label">${escapeHtml(label)}</span>` : ``,
      `</span>`,
    ].join(``)
  return [
    `<div class="cmp-bulk-bar">`,
    `<span class="item">${svgX}</span>`,
    `<span class="value">3</span>`,
    button(svgListTodo, `Status`),
    button(svgFlag, `Priority`),
    button(svgCircleUser, `Assignee`),
    button(svgTag, `Labels`),
    pill(`Start coding`, { size: `md`, glyph: svgPlay, primary: true }),
    button(svgTrash, `Delete`, true),
    `</div>`,
  ].join(``)
}

function iconButton(glyph: string): string {
  return `<button class="cmp-icon-button" type="button">${glyph}</button>`
}

/**
 * The ONE capsule. `readonly` is what used to be a chip, `sm` + `action` what
 * used to be a "header button" — both were the same chrome wearing a second
 * name, so EXP-698 kept the chrome and dropped the names.
 */
interface PillOptions {
  size?: `md` | `sm`
  mode?: `action` | `select` | `readonly`
  selected?: boolean
  glyph?: string
  dot?: boolean
  /** Accent paint (EXP-698 r4) — orthogonal to size and mode. */
  primary?: boolean
}

function pill(label: string, options: PillOptions = {}): string {
  const {
    size = `sm`,
    mode = `action`,
    selected = false,
    glyph,
    dot = false,
    primary = false,
  } = options
  const tag = mode === `readonly` ? `span` : `button`
  return [
    `<${tag} class="cmp-pill${selected ? ` selected` : ``}"`,
    primary ? ` data-primary` : ``,
    ` data-size="${size}" data-mode="${mode}"${mode === `readonly` ? `` : ` type="button"`}>`,
    dot ? `<span class="dot"></span>` : ``,
    glyph ?? ``,
    `<span class="label">${escapeHtml(label)}</span>`,
    `</${tag}>`,
  ].join(``)
}

/**
 * No `userId` here — the demo names the hue directly, because the hash that
 * picks it is the CLIENTS' contract (each pins the same 8-id fixture), not
 * something a static page can re-derive. Omitting the label draws the picture
 * arm: a filled circle, which is what a real photo occupies.
 */
function avatar(initials?: string, hue?: number): string {
  const attr = initials === undefined ? ` data-photo` : ` data-hue="${hue ?? 0}"`
  return `<span class="cmp-avatar"${attr}>${initials === undefined ? `` : escapeHtml(initials)}</span>`
}

interface RichTabOptions {
  title: string
  /** The mono run identifier — a terminal index, an issue key, a branch. */
  id?: string
  glyph?: string
  dot?: boolean
  badge?: string
  active?: boolean
}

function richTab(options: RichTabOptions): string {
  const { title, id, glyph, dot = false, badge, active = false } = options
  return [
    `<span class="cmp-rich-tab${active ? ` active` : ``}">`,
    dot ? `<span class="dot"></span>` : ``,
    glyph ?? ``,
    `<span class="title">${escapeHtml(title)}</span>`,
    id === undefined ? `` : `<span class="id">${escapeHtml(id)}</span>`,
    badge === undefined ? `` : `<span class="badge">${escapeHtml(badge)}</span>`,
    `<span class="close">${svgX}</span>`,
    `</span>`,
  ].join(``)
}

interface ComposerOptions {
  placeholder: string
  tools: string[]
  /** One attachment, to show the strip; the real one wraps. */
  attachment?: string
  submit?: string
  opaque?: boolean
}

function composer(options: ComposerOptions): string {
  const { placeholder, tools, attachment, submit = svgSend, opaque = false } = options
  const strip =
    attachment === undefined
      ? ``
      : `<div class="strip"><span class="item">${svgImage}<span class="label">${escapeHtml(attachment)}</span></span></div>`
  return [
    `<div class="cmp-composer${opaque ? ` opaque` : ``}">`,
    strip,
    `<textarea class="field" rows="1" placeholder="${escapeHtml(placeholder)}"></textarea>`,
    `<div class="tools">`,
    tools.map((glyph) => `<button class="tool" type="button">${glyph}</button>`).join(``),
    `<button class="submit" type="button">${submit}</button>`,
    `</div>`,
    `</div>`,
  ].join(``)
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

/**
 * There is deliberately no `leftover` helper: as of EXP-698 every control on
 * this page agrees on all four platforms. A control that drifts again gets
 * `{ state: `leftover`, symbol, file, note }` back — the state, the render and
 * the yellow note styling all still exist for it.
 */

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
        sectionHeader(`Boards`, pill(`New`, { glyph: svgPlus })),
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
        row(`APP-14 · Fix the merge queue`, pill(`in review`, { mode: `readonly` }), true),
        row(`APP-15 · Ship the usage sheet`, pill(`backlog`, { mode: `readonly` }), true),
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
    id: `rich-tab`,
    title: `Rich tab`,
    kind: `Controls`,
    blurb: `The STRIP tab — the desktop's top tab strip and terminal dock, the web agent dock. 26 tall, radius 10, padding 0/10, no chrome at rest: hover takes the row fill, active the active fill and full foreground. A 16px status glyph or a 6px dot leads, the title truncates at 180, a mono identifier sits at 50%, an exit code rides a small badge, and the close is a ghost 20px X. Never a pill: pills carry a label, this carries a state.`,
    status: {
      web: ok(`RichTab`, `apps/web/src/components/rich-tab.tsx`),
      desktop: ok(`surface::rich_tab`, DESKTOP_SURFACE),
      ios: na(`no terminal or top tab strips`),
      android: na(`no terminal or top tab strips`),
    },
    render: () =>
      [
        `<div class="cmp-inline">`,
        richTab({ glyph: svgTerminal, title: `zsh`, id: `1`, active: true }),
        richTab({ dot: true, title: `Fix the merge queue`, id: `APP-14` }),
        richTab({ glyph: svgTerminal, title: `bun run typecheck`, badge: `exit 1` }),
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
      android: ok(`GlassSubmitButton`, `${ANDROID_COMPONENTS}/GlassSubmitButton.kt`),
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
    blurb: `The ONE capsule, a 2×3 matrix: size md 32 or sm 24, mode action / select / readonly, plus a primary PAINT flag that crosses all six. Card fill under a card stroke, label at 70% — action and select go active on hover, a selected one also takes the active stroke, readonly is metadata and never a target. There is no chip and no header button: those WERE this, under a second name. A conversation or subagent tab is sm select; a members-list role chip is sm readonly, 12px from its neighbours in a row.`,
    status: {
      web: ok(`Pill`, `apps/web/src/components/ui/pill.tsx`),
      desktop: ok(`surface::glass_pill`, DESKTOP_SURFACE),
      ios: ok(`GlassPill`, `apps/ios/ExpUI/Sources/GlassPill.swift`),
      android: ok(`GlassPill`, `${ANDROID_COMPONENTS}/GlassPill.kt`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<div class="cmp-inline">`,
        pill(`New`, { size: `md`, glyph: svgPlus }),
        pill(`All`, { size: `md`, mode: `select`, selected: true }),
        pill(`Mine`, { size: `md`, mode: `select` }),
        pill(`in review`, { size: `md`, mode: `readonly` }),
        `</div>`,
        `<div class="cmp-inline">`,
        pill(`New`, { glyph: svgPlus }),
        pill(`Open`, { mode: `select`, selected: true }),
        pill(`Merged`, { mode: `select` }),
        pill(`Owner`, { mode: `readonly` }),
        pill(`APP-14`, { mode: `readonly`, glyph: svgGitMerge }),
        pill(`running`, { mode: `readonly`, dot: true }),
        `</div>`,
        `<div class="cmp-inline">`,
        pill(`Create issue`, { size: `md`, primary: true }),
        pill(`Watch`, { primary: true, glyph: svgPlay }),
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `avatar`,
    title: `Avatar`,
    kind: `Controls`,
    blurb: `Picture first: a circle filled edge to edge by the person's image. Without one the initials sit on THEIR hue — one of eight token colours picked by fnv1a32(utf8(userId)) % 8 — as a 20% fill under the glyph at full strength, no stroke. The hash is byte-identical on all four clients, so one person is one colour everywhere; a subject with no id at all (a bot, an unresolved reporter) keeps the muted fallback.`,
    status: {
      web: ok(`AvatarFallback`, `apps/web/src/components/ui/avatar.tsx`),
      desktop: ok(`user_avatar::avatar_element`, `apps/desktop/crates/ui/src/user_avatar.rs`),
      ios: ok(`UserAvatar`, `apps/ios/ExpUI/Sources/UserAvatar.swift`),
      android: ok(`UserAvatar`, `${ANDROID_COMPONENTS}/Avatars.kt`),
    },
    render: () =>
      [
        `<div class="cmp-inline">`,
        avatar(),
        avatar(`MK`, 1),
        avatar(`JS`, 4),
        avatar(`SL`, 6),
        `</div>`,
      ].join(``),
  },
  {
    id: `text-field`,
    title: `Text field`,
    kind: `Controls`,
    blurb: `36 tall, padding 0/12, radius 12, card fill under a card stroke; focus swaps the stroke to active — no ring. Placeholder at 50%.`,
    status: {
      web: ok(`Input`, `apps/web/src/components/ui/input.tsx`),
      desktop: {
        state: `leftover`,
        symbol: `WebControl::web_input`,
        file: DESKTOP_CONTROLS,
        note: `stroke is strokeCard via theme.input; radius and focus ring are theme-wide (10 / neutral ring), not 12 / strokeActive`,
      },
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
    id: `textarea`,
    title: `Text area`,
    kind: `Controls`,
    blurb: `The field's own recipe, grown: radius 12, card fill under a card stroke, focus swaps the stroke to active — no ring. Padding 8/12, three rows tall, and it GROWS with content; the drag handle is off everywhere. Inside a group it goes borderless, because the row is already the chrome.`,
    status: {
      web: ok(`Textarea`, `apps/web/src/components/ui/textarea.tsx`),
      desktop: ok(`controls::web_textarea`, DESKTOP_CONTROLS),
      ios: ok(`GlassTextField(lines:)`, IOS_CONTROLS),
      android: ok(`GlassTextField(minLines/maxLines)`, `${ANDROID_COMPONENTS}/GlassTextField.kt`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<textarea class="cmp-textarea" rows="3">Rebase onto origin/master and force-push, then merge.</textarea>`,
        `<textarea class="cmp-textarea" rows="3" placeholder="Describe the issue"></textarea>`,
        group(
          inputRow(`Title`, `Fix the merge queue`),
          `<div class="cmp-row-shell"><textarea class="cmp-textarea borderless" rows="3" placeholder="Description"></textarea></div>`
        ),
        `</div>`,
      ].join(``),
  },
  {
    id: `sheet`,
    title: `Sheet shell`,
    kind: `Surfaces`,
    blurb: `Top radius 24 over the page's bottom gradient, a card hairline, a 36×4 grabber. Header gutter 20, content gutter 16. Dismissal is the grabber drag or the backdrop — the header's trailing slot holds an optional ACTION, never a Cancel — and the bottom carries exactly one primary.`,
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
        `<div class="header"><span class="title">New issue</span><span class="trailing">${pill(`Clear all`)}</span></div>`,
        `<div class="content">`,
        group(pickerRow(`Board`, `Mobile app`), pickerRow(`Status`, `Backlog`)),
        `<button class="cmp-button-primary" type="button">Create</button>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `composer`,
    title: `Composer`,
    kind: `Surfaces`,
    blurb: `ONE composer for comments, steering and support replies: a radius-16 card of card fill under a card hairline, holding an optional attachment strip, a borderless 36-min field and a tool row of 24px ghost glyph buttons with a right-aligned submit whose glyph is the primary tint. The steer variant carries only attach and send. The opaque variant swaps to the opaque card fill and the strong stroke — it floats over a feed on mobile, and an alpha fill there shows the conversation through it.`,
    status: {
      web: ok(`Composer`, `apps/web/src/components/composer.tsx`),
      desktop: ok(`composer::glass_composer`, `apps/desktop/crates/ui/src/composer.rs`),
      ios: ok(`GlassComposer`, `apps/ios/ExpUI/Sources/GlassComposer.swift`),
      android: ok(`GlassComposer`, `${ANDROID_COMPONENTS}/GlassComposer.kt`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        composer({
          placeholder: `Leave a comment`,
          attachment: `screenshot.png`,
          tools: [svgImage, svgPaperclip, svgHash, svgSmile],
        }),
        composer({ placeholder: `Steer the run`, tools: [svgPlus], submit: svgSend }),
        composer({ placeholder: `Reply`, tools: [svgPaperclip], opaque: true }),
        `</div>`,
      ].join(``),
  },
  {
    id: `markdown`,
    title: `Markdown blocks`,
    kind: `Surfaces`,
    blurb: `The chat-sized set the steer feed is built from. Narration is bare text at 90% behind a 12px glyph at 50% — no bubble, because a wall of them is unreadable. The person's turn IS a bubble: radius 12, active fill, strong hairline. Plan and question share ONE neutral radius-16 card; only the header line is tinted, primary for a plan and yellow for a question. A tool line is a 12px label with a truncated mono detail at 50%, and any long block clamps at 160 behind Show more. Inline code is tinted in chat feeds only — the issue and comment renderers keep the neutral chip.`,
    status: {
      web: ok(`QuestionCard / NarrationBubble`, `apps/web/src/components/agent-session.tsx`),
      desktop: ok(`steer_viewer`, `apps/desktop/crates/ui/src/steer_viewer.rs`),
      ios: ok(`QuestionCard`, `apps/ios/Exponential/UI/Session/AgentSessionView.swift`),
      android: ok(
        `QuestionCard`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt`
      ),
    },
    render: () =>
      [
        `<div class="cmp-markdown">`,
        `<div class="narration">${svgTerminal}<span class="label">Reading the merge queue, then the <code>applyPrMergeState</code> webhook that feeds it.</span></div>`,
        `<div class="tool-row"><span class="label">Read</span><span class="value">apps/web/src/lib/trpc/coding-sessions.ts</span></div>`,
        `<div class="bubble">Rebase onto <code>master</code> first, then open the PR.</div>`,
        `<div class="card">`,
        `<div class="card-head">${svgCheck}<span class="label">Plan ready</span></div>`,
        `<div class="fold">Move the merge-state fan-out into applyPrMergeState, end every live session on the merged branch, and leave the run that merged its own PR alone. Then re-point the webhook and the poller at the same helper so the two paths cannot drift again, and cover both with one test that merges a batch PR and asserts every linked issue lands on the team's merge target, on the webhook path and the polling one.</div>`,
        `<span class="show-more">Show more</span>`,
        `</div>`,
        `<div class="card warn">`,
        `<div class="card-head">${svgCircleHelp}<span class="label">Needs input</span></div>`,
        `<div>Should a batch PR merge close every linked issue, or only the ones whose branch matches?</div>`,
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
      web: ok(`DropdownMenuContent`, `apps/web/src/components/ui/dropdown-menu.tsx`),
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
    id: `bulk-bar`,
    title: `Bulk action bar`,
    kind: `Surfaces`,
    blurb: `The selection's own bar: the tab bar's opaque card at radius XL3, padding 10x8, holding the clear cross, the count, one ghost button per property, the accent Start coding pill and a destructive trash. On a phone it REPLACES the tab bar and the labels drop away.`,
    status: {
      web: ok(`BulkActionBar`, `apps/web/src/components/bulk-action-bar.tsx`),
      desktop: ok(`render_bulk_bar`, `apps/desktop/crates/ui/src/issue_list.rs`),
      ios: ok(`selectionBar`, `apps/ios/Exponential/UI/Issue/IssueListView.swift`),
      android: ok(
        `SelectionBar`,
        `apps/android/app/src/main/java/com/exponential/app/ui/issue/IssueListScreen.kt`
      ),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        bulkBar(true),
        bulkBar(false),
        `</div>`,
      ].join(``),
  },
  {
    id: `usage-bar`,
    title: `Usage bar`,
    kind: `Surfaces`,
    blurb: `A label/amount line above a 6px capsule track in the strong stroke; the fill is foreground at 30%, or the yellow semantic once it is nearly spent.`,
    status: {
      web: ok(`AgentUsageCards`, `apps/web/src/components/agent-usage-bar.tsx`),
      desktop: ok(`render_usage_cards`, `apps/desktop/crates/ui/src/usage_bar.rs`),
      ios: ok(`AgentUsageCardRow`, `apps/ios/Exponential/UI/Session/AgentUsageCards.swift`),
      android: ok(
        `UsageTrack`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentUsageBar.kt`
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
        pill(`above`, { mode: `readonly` }),
        `<div class="cmp-divider"></div>`,
        pill(`below`, { mode: `readonly` }),
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
      web: ok(
        `buttonVariants size-9 / Pill size md|sm`,
        `apps/web/src/components/ui/button.tsx`,
        `32 and 24 are the pill's md and sm, in ui/pill.tsx`
      ),
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
