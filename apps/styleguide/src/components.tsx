/**
 * The Components group (EXP-698) — the canonical glass control set: the REAL
 * `@exp/ui` component wherever one owns the form (an ISLAND, EXP-887), and
 * HTML/CSS driven by `@exp/design-tokens` for the compositions that have no
 * single owner.
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
 * EXP-887 turned every entry whose web symbol moved into `@exp/ui` into an
 * ISLAND: the REAL component, rendered to static markup inside a declarative
 * shadow root and painted by the package's own compiled stylesheet
 * (`@exp/ui/island`). A lookalike could disagree with the product silently;
 * the component cannot disagree with itself. What stays hand-written is the
 * COMPOSITIONS — the app shell, a comment card, the token swatches — plus the
 * two Radix portals that render nothing at rest (`PORTAL_ONLY_IDS`).
 *
 * `components.test.tsx` gates the parts that rot: every named file exists,
 * every platform is accounted for, the web/packages-ui split matches the
 * island split, and the stylesheet contains no colour literal.
 */

import type { ReactElement } from "react"

import { designTokens } from "@exp/design-tokens"
import {
  Button,
  GlassCard,
  GlassGroup,
  GlassInputRow,
  GlassPickerRow,
  GlassRow,
  GlassSectionHeader,
  GlassTabsRow,
  GlassToggleRow,
  ICON_DISC_TONES,
  IconDisc,
  IconPicker,
  IconSwatchGrid,
  Input,
  IssueChip,
  ListRow,
  Pill,
  RichTab,
  Separator,
  Tabs,
  TabsList,
  TabsTrigger,
  Textarea,
  UserAvatar,
  conceptIcon,
  type StatusGlyphProps,
} from "@exp/ui"

import {
  escapeHtml,
  svgBell,
  svgBuilding,
  svgCheck,
  svgChevronRight,
  svgCircleHelp,
  svgCircleUser,
  svgExternalLink,
  svgFlag,
  svgGitMerge,
  svgGithub,
  svgHash,
  svgImage,
  svgInbox,
  svgListTodo,
  svgLock,
  svgMessageCircle,
  svgPaperclip,
  svgPlay,
  svgPlus,
  svgRefresh,
  svgSend,
  svgSmile,
  svgTag,
  svgTerminal,
  svgTrash,
  svgTriangleAlert,
  svgUser,
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

interface ComponentSpecBase {
  id: string
  title: string
  kind: `Grouped list` | `Controls` | `Surfaces` | `Tokens`
  blurb: string
  status: Record<ComponentPlatform, ComponentStatus>
}

/**
 * A demo is one of two things (EXP-887):
 *
 *   `island`  the REAL `@exp/ui` component, rendered to static markup inside
 *             a declarative shadow root and painted by the package stylesheet
 *             — the only kind that cannot drift, and the only kind allowed
 *             once the web symbol lives in `packages/ui`
 *   `render`  hand-written HTML using only `.cmp-*` classes, for the
 *             COMPOSITIONS no single component owns (the app shell, a comment
 *             card, the token swatches) and for the two Radix surfaces that
 *             render nothing at rest (see `PORTAL_ONLY_IDS`)
 */
export type ComponentSpec = ComponentSpecBase &
  (
    | { render: () => string; island?: never }
    | { island: () => ReactElement; render?: never }
  )

export function isIsland(
  spec: ComponentSpec
): spec is ComponentSpecBase & { island: () => ReactElement } {
  return spec.island !== undefined
}

/**
 * The two entries whose web symbol DOES live in `packages/ui` and still keeps
 * a hand-written demo: a closed Radix portal renders nothing at all to static
 * markup, so an island of either would be an empty box. Anything else under
 * `packages/ui/` must be an island — `components.test.tsx` gates both
 * directions, and exempts only these and the `Tokens` entries, which document
 * a VALUE rather than a control.
 */
export const PORTAL_ONLY_IDS: readonly string[] = [`sheet`, `menu`]

export const COMPONENTS_GROUP = {
  id: `components`,
  label: `Components`,
  blurb: `The glass control set, rendered live from @exp/ui and the design tokens — not photographed.`,
} as const

const { glass, radius, size, motion } = designTokens

/* ------------------------------------------------------------- fixtures */
/* What an island is handed. Every callback is a no-op and every value is
   already resolved: this page has no router, no team and no live query behind
   it, and an island is the component's RESTING state. Glyphs come through
   `conceptIcon` for the same reason the product does — a multi-client surface
   names a CONCEPT, never a raw lucide import. */

const noop = (): void => {}

const PlusGlyph = conceptIcon(`ui-add`)
const SendGlyph = conceptIcon(`ui-send`)
const PlayGlyph = conceptIcon(`action-run`)
const MoreGlyph = conceptIcon(`ui-more`)
const ChevronDownGlyph = conceptIcon(`ui-chevron-down`)
const CloseGlyph = conceptIcon(`ui-close`)
const ShellGlyph = conceptIcon(`session-shell`)
const MergeGlyph = conceptIcon(`pr-merged`)

/* One glyph per disc tone — the four heads the product actually opens with: a
   team/invite page, a saved connection, a refused one, an empty inbox. */
const DISC_GLYPH = {
  primary: conceptIcon(`ui-team`),
  success: conceptIcon(`ui-success`),
  danger: conceptIcon(`ui-warning`),
  muted: conceptIcon(`nav-inbox`),
} as const

/* The chip's status arrives ALREADY resolved (`StatusGlyph` knows nothing
   about teams), which is exactly what a fixture can supply: the builtin
   backlog and done rows, with the token classes `BUILTIN_STATUS_COLOR_CLASS`
   gives them. */
const BACKLOG_GLYPH: StatusGlyphProps = {
  icon: `circle-dashed`,
  colorClass: `text-muted-foreground`,
}
const DONE_GLYPH: StatusGlyphProps = {
  icon: `circle-check`,
  colorClass: `text-blue-500`,
}

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

function relationRow(label: string, id: string, title: string): string {
  return [
    `<div class="cmp-relation-row">`,
    `<span class="dot"></span>`,
    `<span class="caption">${escapeHtml(label)}</span>`,
    `<span class="id">${escapeHtml(id)}</span>`,
    `<span class="title">${escapeHtml(title)}</span>`,
    `<span class="trailing">${iconButton(svgX)}</span>`,
    `</div>`,
  ].join(``)
}

function iconButton(glyph: string): string {
  return `<button class="cmp-icon-button" type="button">${glyph}</button>`
}

/**
 * The GHOST (EXP-862). Same 32px box, same 16px glyph, but no circle, no fill
 * and no border at rest: a SECONDARY action is a glyph the row owns, and the
 * hover wash is the only paint it ever draws.
 */
function ghostIconButton(glyph: string): string {
  return `<button class="cmp-ghost-icon-button" type="button">${glyph}</button>`
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

/* ------------------------------------------------- GitHub connect (FEED-42) */

/** One text line of the status block: leading glyph or dot, sentence, pills. */
function githubLine(lead: string, text: string, pills: string[] = [], warn = false): string {
  return [
    `<div class="cmp-github-line${warn ? ` cmp-github-warn` : ``}">`,
    lead,
    `<span class="cmp-github-text">${escapeHtml(text)}</span>`,
    ...pills,
    `</div>`,
  ].join(``)
}

/** An account row: org/user glyph, login, the Configure link, the ✕ that confirms. */
function githubAccount(glyph: string, login: string): string {
  return [
    `<div class="cmp-github-account">`,
    glyph,
    `<span class="cmp-github-login">${escapeHtml(login)}</span>`,
    githubLink(`Configure`),
    ghostIconButton(svgX),
    `</div>`,
  ].join(``)
}

/** A text link with the trailing external glyph (Configure, account links). */
function githubLink(label: string): string {
  return `<a class="cmp-github-link" href="#">${escapeHtml(label)}${svgExternalLink}</a>`
}

function repoPickerRow(name: string, locked = false): string {
  return [
    `<div class="cmp-repo-picker-row">`,
    svgGithub,
    `<span class="cmp-repo-picker-name">${escapeHtml(name)}</span>`,
    locked ? `<span class="cmp-repo-picker-lock">${svgLock}</span>` : ``,
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

const WEB_GLASS_ROWS = `packages/ui/src/glass-rows.tsx`
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
    title: `Group band`,
    kind: `Grouped list`,
    blurb: `EXP-818: the Linear group header — a full-width strip on the section fill, radius 10, padding 6/12, 14/20 at 85% foreground, a trailing slot, 4px over its flat rows. No count. Never uppercase and never a divider.`,
    status: {
      web: ok(`GlassSectionHeader`, WEB_GLASS_ROWS, HEADER_EXCEPTION),
      desktop: ok(`surface::glass_section_header`, DESKTOP_SURFACE, HEADER_EXCEPTION),
      ios: ok(`GlassSectionBand`, IOS_THEME, HEADER_EXCEPTION),
      android: ok(
        `Modifier.glassSectionBand()`,
        ANDROID_GLASS,
        `SectionHeader (Scaffolding.kt) wraps it. ${HEADER_EXCEPTION}`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div>
          <GlassSectionHeader
            label="Boards"
            trailing={
              <Pill mode="action" leading={<PlusGlyph />}>
                New
              </Pill>
            }
          />
          <ListRow interactive>
            <span className="min-w-0 flex-1 truncate">Mobile app</span>
            <span className="text-xs text-muted-foreground">24 issues</span>
          </ListRow>
          <ListRow interactive>
            <span className="min-w-0 flex-1 truncate">Website</span>
            <span className="text-xs text-muted-foreground">9 issues</span>
          </ListRow>
        </div>
        <GlassSectionHeader label="Danger zone" />
      </div>
    ),
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
    island: () => (
      <GlassGroup>
        <GlassPickerRow
          label="Repository"
          value="exp"
          onValueChange={noop}
          options={[{ value: `exp`, label: `niach/exponential` }]}
          renderValue={(option) => option?.label}
        />
        <GlassInputRow id="demo-group-slug" label="Slug" defaultValue="mobile-app" />
        <GlassToggleRow
          id="demo-group-archived"
          label="Archived"
          checked={false}
          onCheckedChange={noop}
        />
      </GlassGroup>
    ),
  },
  {
    id: `row`,
    title: `Glass row`,
    kind: `Grouped list`,
    blurb: `The GAPPED card item: radius 10, row fill, its own hairline border, padding 12. EXP-818 keeps it for the few real cards (a transcript's tool output, a diff); every LIST wears the flat list row below.`,
    status: {
      web: ok(`GlassRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_row_card`, DESKTOP_SURFACE),
      ios: ok(`GlassRow`, IOS_THEME),
      android: ok(`Modifier.glassRow()`, ANDROID_GLASS),
    },
    island: () => (
      <div className="grid gap-3">
        <GlassRow interactive>
          <span className="min-w-0 flex-1 truncate">APP-14 · Fix the merge queue</span>
          <Pill>in review</Pill>
        </GlassRow>
        <GlassRow interactive>
          <span className="min-w-0 flex-1 truncate">APP-15 · Ship the usage sheet</span>
          <Pill>backlog</Pill>
        </GlassRow>
      </div>
    ),
  },
  {
    id: `list-row`,
    title: `List row`,
    kind: `Grouped list`,
    blurb: `EXP-818: the flat list item every list wears — no stroke, no fill, radius 10, padding 12, NO gap between rows under a group band; hover takes the row fill, the selected row the active fill. Rows read as a table, not as cards.`,
    status: {
      web: ok(`ListRow`, WEB_GLASS_ROWS),
      desktop: ok(`surface::flat_row`, DESKTOP_SURFACE),
      ios: ok(`FlatRow / .flatRow()`, IOS_THEME),
      android: ok(`Modifier.flatRow()`, ANDROID_GLASS),
    },
    island: () => (
      <div>
        <GlassSectionHeader label="Running" />
        <ListRow interactive active>
          <span className="min-w-0 flex-1 truncate">APP-14 · Fix the merge queue</span>
          <span className="text-xs text-muted-foreground">macbook</span>
        </ListRow>
        <ListRow interactive>
          <span className="min-w-0 flex-1 truncate">APP-15 · Ship the usage sheet</span>
          <span className="text-xs text-muted-foreground">server</span>
        </ListRow>
      </div>
    ),
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
    island: () => (
      <GlassGroup>
        <GlassPickerRow
          label="Agent"
          value="claude"
          onValueChange={noop}
          options={[{ value: `claude`, label: `claude` }]}
          renderValue={(option) => option?.label}
        />
        <GlassInputRow id="demo-shell-prefix" label="Branch prefix" defaultValue="exp/" />
        <GlassToggleRow
          id="demo-shell-plan"
          label="Plan first"
          description="Ask before it writes"
          checked
          onCheckedChange={noop}
        />
      </GlassGroup>
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
    island: () => (
      <GlassGroup>
        <GlassPickerRow
          label="Status"
          value="in_review"
          onValueChange={noop}
          options={[{ value: `in_review`, label: `In review` }]}
          renderValue={(option) => option?.label}
        />
        <GlassPickerRow
          label="Assignee"
          value="danny"
          onValueChange={noop}
          options={[{ value: `danny`, label: `Danny` }]}
          renderValue={(option) => option?.label}
        />
        <GlassPickerRow
          label="Due date"
          value=""
          onValueChange={noop}
          options={[]}
          placeholder="No date"
        />
      </GlassGroup>
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
    island: () => (
      <GlassGroup>
        <GlassInputRow id="demo-input-name" label="Name" defaultValue="Mobile app" />
        <GlassInputRow id="demo-input-prefix" label="Prefix" defaultValue="APP" />
      </GlassGroup>
    ),
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
    island: () => (
      <GlassGroup>
        <GlassToggleRow
          id="demo-toggle-helpdesk"
          label="Helpdesk"
          description="Reporters can reply by email"
          checked
          onCheckedChange={noop}
        />
        <GlassToggleRow
          id="demo-toggle-widget"
          label="Widget"
          checked={false}
          onCheckedChange={noop}
        />
      </GlassGroup>
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
    island: () => (
      <GlassGroup>
        <GlassTabsRow value="open" onValueChange={noop}>
          <TabsTrigger value="open">Open</TabsTrigger>
          <TabsTrigger value="merged">Merged</TabsTrigger>
          <TabsTrigger value="all">All</TabsTrigger>
        </GlassTabsRow>
        <GlassPickerRow
          label="APP-14"
          value="two"
          onValueChange={noop}
          options={[{ value: `two`, label: `2 files` }]}
          renderValue={(option) => option?.label}
        />
        <GlassPickerRow
          label="APP-21"
          value="seven"
          onValueChange={noop}
          options={[{ value: `seven`, label: `7 files` }]}
          renderValue={(option) => option?.label}
        />
      </GlassGroup>
    ),
  },
  {
    id: `segmented`,
    title: `Segmented control`,
    kind: `Controls`,
    blurb: `The standalone capsule: 36 tall, padding 3, the section fill under a section stroke. Segments share the embedded row's geometry.`,
    status: {
      web: ok(`TabsList`, `packages/ui/src/tabs.tsx`),
      desktop: ok(`controls::segmented`, DESKTOP_CONTROLS),
      ios: ok(`GlassSegmentedControl`, IOS_SEGMENTED),
      android: ok(`GlassSegmentedControl`, `${ANDROID_COMPONENTS}/GlassSegmentedControl.kt`),
    },
    island: () => (
      <Tabs value="issues">
        <TabsList>
          <TabsTrigger value="issues">Issues</TabsTrigger>
          <TabsTrigger value="actions">Actions</TabsTrigger>
          <TabsTrigger value="automations">Automations</TabsTrigger>
        </TabsList>
      </Tabs>
    ),
  },
  {
    id: `rich-tab`,
    title: `Rich tab`,
    kind: `Controls`,
    blurb: `The STRIP tab — the desktop's top tab strip and session bar, the web agent dock. 26 tall, radius 10, padding 0/10, no chrome at rest: hover takes the row fill, active the active fill and full foreground. A 16px status glyph or a 6px dot leads, the title truncates at 180, a mono identifier sits at 50%, an exit code rides a small badge, and the close is a ghost 20px X. Never a pill: pills carry a label, this carries a state.`,
    status: {
      web: ok(`RichTab`, `packages/ui/src/rich-tab.tsx`),
      desktop: ok(`surface::rich_tab`, DESKTOP_SURFACE),
      ios: na(`no terminal or top tab strips`),
      android: na(`no terminal or top tab strips`),
    },
    island: () => (
      <div className="flex flex-wrap items-center gap-2">
        <RichTab
          active
          icon={<ShellGlyph className="size-4" />}
          identifier="1"
          title="zsh"
          onSelect={noop}
          onClose={noop}
        />
        <RichTab
          status="bg-emerald-500"
          identifier="APP-14"
          title="Fix the merge queue"
          onSelect={noop}
          onClose={noop}
        />
        <RichTab
          icon={<ShellGlyph className="size-4" />}
          title="bun run typecheck"
          badge={<Pill>exit 1</Pill>}
          onSelect={noop}
          onClose={noop}
        />
      </div>
    ),
  },
  {
    id: `session-bar`,
    title: `Session bar`,
    kind: `Controls`,
    blurb: `The bottom strip of coding tabs (EXP-769). It sits OUTSIDE the content card, on the bare page ground below it, the mirror of the desktop's head toolbar above (EXP-771): the card stops 6px short and the band takes the last 36 down to the window bottom, with no fill and no border of its own and its chips inset 8. Give it a fill and the ground reads as a second card. It holds the rich tabs of the user's running sessions and, on the desktop, the open terminals — a session tab leads with its 6px liveness dot and carries the issue's mono identifier, the title and a muted " · machine" caption; a terminal tab leads with the terminal glyph and wears its exit code as a badge. Right after the last tab sit two ghost 24px glyph buttons: Chat (a promptless chat run on the default agent) and add (a plain terminal, desktop only). Nothing else: no header, no collapse, no label when empty — the strip is always there so Chat is always one click away. Selecting a tab opens that session or terminal FULLSCREEN in the content area; the web navigates to the session route, the desktop shows the screen. The × on a live session kills it (confirmed), on an ended one closes the transcript tab, on a terminal closes the terminal.`,
    status: {
      web: na(`EXP-818: no bottom band on the web — sessions are the sidebar's Sessions group and the Agent page's list.`),
      desktop: ok(
        `session_bar::SessionBar`,
        `apps/desktop/crates/ui/src/session_bar.rs`,
        `the tabs are ScreensPanel::render_session_bar_tabs; terminals are Screen::Terminal center screens`
      ),
      ios: na(`no session bar: sessions open full-screen from Agents`),
      android: na(`no session bar: sessions open full-screen from Agents`),
    },
    render: () =>
      [
        // The ground is part of the specimen: the point of the band is what it
        // sits ON, so the demo draws the card it hangs below.
        `<div class="cmp-session-ground">`,
        `<div class="card"><span class="line"></span><span class="line"></span></div>`,
        `<div class="cmp-session-bar">`,
        richTab({ dot: true, title: `Fix the merge queue`, id: `APP-14`, active: true }),
        richTab({ glyph: svgTerminal, title: `zsh`, id: `1` }),
        richTab({ glyph: svgTerminal, title: `bun run typecheck`, badge: `exit 1` }),
        `<button class="tool" type="button">${svgMessageCircle}</button>`,
        `<button class="tool" type="button">${svgPlus}</button>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `icon-button`,
    title: `Primary icon button`,
    kind: `Controls`,
    blurb: `A 32px circle of card fill under a card stroke, glyph 16px at 70% foreground; hover fills to active and the glyph goes full strength. The SHAPE is the meaning (EXP-771, narrowed by EXP-862): a circle marks the PRIMARY action and nothing else wears one. That is play / start, send, the rail's New issue and Search, a mobile FAB, and the "+" that adds. Every other icon-only control is the ghost icon button. The remaining exception is a picker TRIGGER, which is a rounded square: see icon picker.`,
    status: {
      web: ok(`buttonVariants variant="glass" size="icon-sm"`, `packages/ui/src/button.tsx`),
      desktop: ok(`controls::glass_icon_button`, DESKTOP_CONTROLS),
      ios: ok(`CircleIconButton`, IOS_CONTROLS),
      android: ok(`CircleIconButton`, `${ANDROID_COMPONENTS}/CircleIconButton.kt`),
    },
    island: () => (
      <div className="flex items-center gap-2">
        <Button variant="glass" size="icon-sm" aria-label="Start coding">
          <PlayGlyph />
        </Button>
        <Button variant="glass" size="icon-sm" aria-label="Send">
          <SendGlyph />
        </Button>
        <Button variant="glass" size="icon-sm" aria-label="New issue">
          <PlusGlyph />
        </Button>
      </div>
    ),
  },
  {
    id: `ghost-icon-button`,
    title: `Ghost icon button`,
    kind: `Controls`,
    blurb: `The SECONDARY icon button (EXP-862): the same 32px box and the same 16px glyph at 70% foreground, with no circle, no fill and no border at rest. Hover is the only paint it carries, the row wash under the MD corner, and the glyph goes full strength. Everything that is not the primary action wears this one: the "…" overflow, close, the folder and file-list toggles, the chevrons (back, fold, reorder), trash and remove, refresh. Put a circle here and the surface ends up with three things asking to be pressed and no way to tell which one it wants.`,
    status: {
      web: ok(`buttonVariants variant="ghost" size="icon-sm"`, `packages/ui/src/button.tsx`),
      desktop: ok(`controls::ghost_icon_button`, DESKTOP_CONTROLS),
      ios: ok(`GhostIconButton`, IOS_CONTROLS),
      android: ok(
        `CircleIconButton(borderless = true)`,
        `${ANDROID_COMPONENTS}/CircleIconButton.kt`,
        `one composable, two shapes: borderless drops the circle and the stroke and keeps the hover fill`
      ),
    },
    island: () => (
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="icon-sm" aria-label="More">
          <MoreGlyph />
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Fold">
          <ChevronDownGlyph />
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Close">
          <CloseGlyph />
        </Button>
      </div>
    ),
  },
  {
    id: `icon-picker`,
    title: `Icon picker`,
    kind: `Controls`,
    blurb: `The one surface that picks a glyph, and the one exception to the circle (EXP-771): a circle is the primary ACTION, ROUNDED SQUARE is a picker. The trigger is a square at the radius ladder's MD step over card fill, sized to the field it sits beside (web h-9, desktop and the natives the 32px control rung) — a card hairline once something is picked, a DASHED one under the placeholder glyph while it is empty — and the cells of the 60-glyph grid it opens wear that same corner, the picked one taking the active fill under the active stroke. EXP-862 gave the colour picker the SAME trigger, so the board form reads as one control repeated; the swatches inside it are the counter-example: a colour has no shape to read, so those stay circles.`,
    status: {
      web: ok(
        `IconPicker`,
        `packages/ui/src/icon-picker.tsx`,
        `the grid is icon-swatch-grid.tsx, rendered inside the trigger's popover`
      ),
      desktop: ok(
        `board_form::icon_picker`,
        `apps/desktop/crates/ui/src/board_form.rs`,
        `icon_swatch_grid is the grid in the same file`
      ),
      ios: ok(
        `IconPicker`,
        `apps/ios/ExpUI/Sources/IconPicker.swift`,
        `the grid is ExpUI/Sources/IconSwatchGrid.swift`
      ),
      android: ok(
        `IconPicker`,
        `${ANDROID_COMPONENTS}/IconPicker.kt`,
        `the grid is ui/components/IconSwatchGrid.kt`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex items-center gap-2">
          <IconPicker value="" onChange={noop} allowsNone />
          <IconPicker value="flag" onChange={noop} />
          <Button variant="glass" size="icon-sm" aria-label="New board">
            <PlusGlyph />
          </Button>
        </div>
        {/* 8 × 28px cells + 7 × 6px gaps — the popover's own column count. */}
        <div className="w-[266px]">
          <IconSwatchGrid value="flag" onChange={noop} />
        </div>
      </div>
    ),
  },
  {
    id: `button-primary`,
    title: `Primary submit`,
    kind: `Controls`,
    blurb: `Full width, padding 14/16, radius 10, solid primary. Disabled drops to card fill with a card stroke and 50% foreground. The specimen is the web's own Button, which is a CAPSULE — the radius-10 rectangle is the mobile sheet submit, as the web row below says.`,
    status: {
      web: ok(
        `Button (variant default)`,
        `packages/ui/src/button.tsx`,
        `web/desktop primaries stay capsules; the radius-10 full-width form is the mobile sheet submit`
      ),
      desktop: ok(`Button::primary()`, DESKTOP_CONTROLS),
      ios: ok(`GlassSubmitButton`, IOS_CONTROLS),
      android: ok(`GlassSubmitButton`, `${ANDROID_COMPONENTS}/GlassSubmitButton.kt`),
    },
    island: () => (
      <div className="grid gap-3">
        <Button className="w-full">Create issue</Button>
        <Button className="w-full" disabled>
          Create issue
        </Button>
      </div>
    ),
  },
  {
    id: `pill`,
    title: `Pill`,
    kind: `Controls`,
    blurb: `The ONE capsule, a 2×3 matrix: size md 32 or sm 24, mode action / select / readonly, plus a primary PAINT flag that crosses all six. Card fill under a card stroke, label at 70% — action and select go active on hover, a selected one also takes the active stroke, readonly is metadata and never a target. There is no chip and no header button: those WERE this, under a second name. A conversation or subagent tab is sm select; a members-list role chip is sm readonly, 12px from its neighbours in a row.`,
    status: {
      web: ok(`Pill`, `packages/ui/src/pill.tsx`),
      desktop: ok(`surface::glass_pill`, DESKTOP_SURFACE),
      ios: ok(`GlassPill`, `apps/ios/ExpUI/Sources/GlassPill.swift`),
      android: ok(`GlassPill`, `${ANDROID_COMPONENTS}/GlassPill.kt`),
    },
    island: () => (
      <div className="grid gap-3">
        <div className="flex flex-wrap items-center gap-2.5">
          <Pill size="md" mode="action" leading={<PlusGlyph />}>
            New
          </Pill>
          <Pill size="md" mode="select" selected>
            All
          </Pill>
          <Pill size="md" mode="select">
            Mine
          </Pill>
          <Pill size="md" mode="readonly">
            in review
          </Pill>
        </div>
        <div className="flex flex-wrap items-center gap-2.5">
          <Pill size="sm" mode="action" leading={<PlusGlyph />}>
            New
          </Pill>
          <Pill size="sm" mode="select" selected>
            Open
          </Pill>
          <Pill size="sm" mode="select">
            Merged
          </Pill>
          <Pill size="sm" mode="readonly">
            Owner
          </Pill>
          <Pill size="sm" mode="readonly" leading={<MergeGlyph />}>
            APP-14
          </Pill>
          <Pill size="sm" mode="readonly" dot="var(--color-emerald-500)">
            running
          </Pill>
        </div>
        <div className="flex flex-wrap items-center gap-2.5">
          <Pill size="md" mode="action" primary>
            Create issue
          </Pill>
          <Pill size="sm" mode="action" primary leading={<PlayGlyph />}>
            Watch
          </Pill>
        </div>
      </div>
    ),
  },
  {
    id: `issue-chip`,
    title: `Issue chip`,
    kind: `Controls`,
    blurb: `The ONE badge that names an issue inline. A small rounded RECT — 6px on web, 4 on desktop, 5 on iOS, 5dp on Android — and never a capsule: a capsule is the pill, which carries a label, not a subject. A hairline border over the accent fill (the fill barely clears the surface, so the border is what makes the chip legible), then status glyph · mono muted identifier · the title in the foreground at medium weight, truncated. The left padding is tighter than the right because the glyph carries its own gap. The ✕ sits INSIDE the chip and exists for COMPOSERS only — nothing else hands a badge a control — and it takes the trailing padding down to its own hit box. Inside a markdown editor the identical box is a DECORATION painted over the bare \`#IDENT\` token, so the document text round-trips untouched and the two cannot drift.`,
    status: {
      web: ok(`IssueChip`, `packages/ui/src/issue-chip.tsx`, `The app's components/issue-chip.tsx is the binding that resolves the status and adds the hover preview.`),
      desktop: ok(`issue_chip`, `apps/desktop/crates/ui/src/issue_chip.rs`),
      ios: ok(`IssueChip`, `apps/ios/ExpUI/Sources/IssueChip.swift`),
      android: ok(`IssueChip`, `${ANDROID_COMPONENTS}/IssueChip.kt`),
    },
    island: () => (
      <div className="flex flex-wrap items-center gap-2.5">
        <IssueChip
          identifier="EXP-885"
          title="One issue chip per platform"
          status={BACKLOG_GLYPH}
          onClick={noop}
        />
        <IssueChip
          identifier="EXP-469"
          title="Status glyph padding"
          status={BACKLOG_GLYPH}
          onRemove={noop}
        />
        <IssueChip
          identifier="EXP-423"
          title="The chip is a rounded rect on every client, and a long title truncates"
          status={DONE_GLYPH}
        />
      </div>
    ),
  },
  {
    id: `avatar`,
    title: `Avatar`,
    kind: `Controls`,
    blurb: `Picture first: a circle filled edge to edge by the person's image. Without one the initials sit on THEIR hue — one of eight token colours picked by fnv1a32(utf8(userId)) % 8 — as a 20% fill under the glyph at full strength, no stroke. The hash is byte-identical on all four clients, so one person is one colour everywhere; a subject with no id at all (a bot, an unresolved reporter) keeps the muted fallback.`,
    status: {
      web: ok(`UserAvatar`, `packages/ui/src/user-avatar.tsx`, `AvatarFallback (./avatar.tsx) paints the hue; UserAvatar is the composition every site renders.`),
      desktop: ok(`user_avatar::avatar_element`, `apps/desktop/crates/ui/src/user_avatar.rs`),
      ios: ok(`UserAvatar`, `apps/ios/ExpUI/Sources/UserAvatar.swift`),
      android: ok(`UserAvatar`, `${ANDROID_COMPONENTS}/Avatars.kt`),
    },
    island: () => (
      <div className="flex items-center gap-2.5">
        {/* No id at all — the muted fallback a bot or an unresolved reporter
            keeps. A real photo cannot appear here: `AvatarImage` only paints
            once the browser has DECODED the image, so it renders nothing at
            all statically. */}
        <UserAvatar user={{ name: `Exponential` }} size={32} />
        <UserAvatar user={{ id: `user-mk`, name: `Mina Kay` }} size={32} />
        <UserAvatar user={{ id: `user-js`, name: `Jonas Stern` }} size={32} />
        <UserAvatar user={{ id: `user-sl`, email: `sam.lee@example.com` }} size={32} />
      </div>
    ),
  },
  {
    id: `icon-disc`,
    title: `Icon disc`,
    kind: `Controls`,
    blurb: `A 48px circle holding a 24px glyph. It is a HEADING, not a control — it never takes a click and it has no smaller size: it opens an empty state, a wizard step card, an invite page or a full-page outcome, and nothing else. The TONE owns both the wash and the glyph colour, so a call site never restates a text-* on the icon: primary at 10% for a neutral head, emerald, red and muted at 15% for an outcome. Only placement stays outside — the mx-auto that centres it in a card and whatever margin the copy under it needs. EXP-903: eight web surfaces drew this circle by hand before it became one primitive.`,
    status: {
      web: ok(`IconDisc`, `packages/ui/src/icon-disc.tsx`),
      desktop: ok(
        `OnboardingView::render`,
        `apps/desktop/crates/ui/src/onboarding.rs`,
        `the wizard card head draws the 48px primary disc inline — the only disc on the IDE, so it has no shared helper`
      ),
      ios: na(`the wizard step header is a title over a subtitle; no native screen opens with a disc`),
      android: na(`the onboarding step head is a 56dp glassCard box around a primary glyph, not a tinted circle`),
    },
    island: () => (
      <div className="flex items-center gap-4">
        {ICON_DISC_TONES.map((tone) => (
          <IconDisc key={tone} icon={DISC_GLYPH[tone]} tone={tone} />
        ))}
      </div>
    ),
  },
  {
    id: `text-field`,
    title: `Text field`,
    kind: `Controls`,
    blurb: `36 tall, padding 0/12, radius 12, card fill under a card stroke; focus swaps the stroke to active — no ring. Placeholder at 50%.`,
    status: {
      web: ok(`Input`, `packages/ui/src/input.tsx`),
      desktop: {
        state: `leftover`,
        symbol: `controls::glass_input`,
        file: DESKTOP_CONTROLS,
        note: `focus swaps the stroke to strokeActive, no ring (EXP-720); the radius is still theme-wide (10, not 12)`,
      },
      ios: ok(`GlassTextField`, IOS_CONTROLS),
      android: ok(`GlassTextField`, `${ANDROID_COMPONENTS}/GlassTextField.kt`),
    },
    island: () => (
      <div className="grid gap-3">
        <Input defaultValue="Fix the merge queue" />
        <Input placeholder="Search issues" />
      </div>
    ),
  },
  {
    id: `textarea`,
    title: `Text area`,
    kind: `Controls`,
    blurb: `The field's own recipe, grown: radius 12, card fill under a card stroke, focus swaps the stroke to active — no ring. Padding 8/12, three rows tall, and it GROWS with content; the drag handle is off everywhere. Inside a group it goes borderless, because the row is already the chrome.`,
    status: {
      web: ok(`Textarea`, `packages/ui/src/textarea.tsx`),
      desktop: ok(`controls::web_textarea`, DESKTOP_CONTROLS),
      ios: ok(`GlassTextField(lines:)`, IOS_CONTROLS),
      android: ok(`GlassTextField(minLines/maxLines)`, `${ANDROID_COMPONENTS}/GlassTextField.kt`),
    },
    island: () => (
      <div className="grid gap-3">
        <Textarea rows={3} defaultValue="Rebase onto origin/master and force-push, then merge." />
        <Textarea rows={3} placeholder="Describe the issue" />
        <GlassGroup>
          <GlassInputRow id="demo-textarea-title" label="Title" defaultValue="Fix the merge queue" />
          <div className="px-4 py-3">
            <Textarea
              rows={3}
              placeholder="Description"
              className="rounded-none border-0 bg-transparent p-0 shadow-none focus-visible:border-0"
            />
          </div>
        </GlassGroup>
      </div>
    ),
  },
  {
    id: `app-shell`,
    title: `App shell`,
    kind: `Surfaces`,
    blurb: `The CUTOUT (EXP-723). The window is the page gradient; the navigation column sits directly on it with no fill of its own; the content is a card inset 10 on the sides and top, and 6 at the bottom while a session band renders (10 otherwise) — radius 12, a card hairline, the panel wash, overflow hidden. Chrome that is not content sits on the bare ground AROUND the card, symmetrically (EXP-771): the desktop's title strip above it and the session band below on both clients, each 36 tall with no fill and no border, their chips inset 8, and the band running to the window bottom. The wash is translucent on purpose: the ground darkens down the page and a solid fill would drift away from it. Phones drop the card entirely and run full-bleed under the tab bar.`,
    status: {
      web: ok(`mainPanelClass`, `apps/web/src/components/team/app-shell.ts`),
      desktop: ok(
        `Shell::render`,
        `apps/desktop/crates/ui/src/shell.rs`,
        `cutout panel painted by Shell::render, FILL_PANEL over the content ramp`
      ),
      ios: na(`phones are full-bleed under the tab bar; no cutout`),
      android: na(`phones are full-bleed under the tab bar; no cutout`),
    },
    render: () =>
      [
        `<div class="cmp-app-shell">`,
        // Both bands are children of the SHELL, never of the panel: that is the
        // whole claim the demo is making.
        `<div class="header"><span class="title">Mobile app</span></div>`,
        `<div class="content">`,
        `<div class="nav">`,
        `<span class="item"><span class="label">Inbox</span></span>`,
        `<span class="item active"><span class="label">Mobile app</span></span>`,
        `<span class="item"><span class="label">Reviews</span></span>`,
        `</div>`,
        `<div class="panel"><span class="line"></span><span class="line"></span><span class="line"></span></div>`,
        `</div>`,
        `<div class="dock">`,
        richTab({ glyph: svgTerminal, title: `zsh`, id: `1`, active: true }),
        richTab({ dot: true, title: `Fix the merge queue`, id: `APP-14` }),
        `<button class="add" type="button">${svgPlus}</button>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `page-header`,
    title: `Settings page header`,
    kind: `Surfaces`,
    blurb: `Every settings page on web and desktop opens identically (EXP-771): the title Settings at 2xl bold, the subtitle "Manage {team name} and your account" at sm muted, then a hairline divider. All three ride a centred column capped at 56rem (896px at a 16px root; the desktop pins 896 outright) with 1.5rem padding, while the SCROLL region is the full width of the pane, so the scrollbar rides the viewport edge instead of the text column. The nav beside it lists every page: web gains Helpdesk as its own entry next to Feedback widget, and the desktop carries both under a Features group.`,
    status: {
      web: ok(`SettingsLayout`, `apps/web/src/routes/t/$teamSlug/settings/route.tsx`),
      desktop: ok(
        `settings::detail_column`,
        `apps/desktop/crates/ui/src/settings/mod.rs`,
        `the column and the header the SettingsView panes render into`
      ),
      ios: na(`the native settings root is a grouped list, no page title band`),
      android: na(`the native settings root is a grouped list, no page title band`),
    },
    render: () =>
      [
        `<div class="cmp-page-header">`,
        `<div class="content">`,
        `<div class="title">Settings</div>`,
        `<div class="desc">Manage Mobile Ltd and your account</div>`,
        `<div class="cmp-divider"></div>`,
        group(pickerRow(`Team name`, `Mobile Ltd`), toggleRow(`Helpdesk`, undefined, true)),
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `glass-card`,
    title: `Glass card`,
    kind: `Surfaces`,
    blurb: `The ONE translucent card, and only its box: radius XL, the card hairline, the glass card fill. Everything a card owns on its own stays at the call site, because those genuinely differ — its padding, a blur, a shadow, and the divide-y + overflow hidden that turns the same box into a GROUP of rows. EXP-903: five surfaces painted it by copy-paste (the comment row, the agent AskCard, the usage card, the mobile issue properties sheet and the repo picker, which had drifted to the MD corner and the bare hairline). The shadcn Card derives its recipe from the same constant, so the two cannot disagree; a group of list ROWS is the group container instead, on the row fill with no outer stroke.`,
    status: {
      web: ok(
        `GlassCard / GLASS_CARD_CLASS`,
        `packages/ui/src/glass-card.tsx`,
        `Card (./card.tsx) derives its own recipe from the same constant.`
      ),
      desktop: ok(`surface::glass_card`, DESKTOP_SURFACE),
      ios: ok(`GlassCard`, IOS_THEME),
      android: ok(`Modifier.glassCard()`, ANDROID_GLASS),
    },
    island: () => (
      <div className="grid gap-3">
        <GlassCard className="p-3">
          <div className="text-sm font-medium">Pull request opened</div>
          <div className="text-xs text-muted-foreground">exp/EXP-903 → master</div>
        </GlassCard>
        {/* The same box as a GROUP: the dividers and the overflow rule are the
            call site's, which is the whole point of the primitive. */}
        <GlassCard className="divide-y divide-glass-stroke overflow-hidden">
          <div className="flex items-center justify-between px-4 py-3 text-sm">
            <span className="text-muted-foreground">Status</span>
            <span>In progress</span>
          </div>
          <div className="flex items-center justify-between px-4 py-3 text-sm">
            <span className="text-muted-foreground">Assignee</span>
            <span>Mina Kay</span>
          </div>
          <div className="flex items-center justify-between px-4 py-3 text-sm">
            <span className="text-muted-foreground">Board</span>
            <span>Mobile app</span>
          </div>
        </GlassCard>
      </div>
    ),
  },
  {
    id: `comment-card`,
    title: `Comment card`,
    kind: `Surfaces`,
    blurb: `One comment in the activity feed: the avatar rides the timeline gutter, everything else lives in a radius-16 card of card fill under a card hairline. The header is the author at body size and medium weight, then a muted caption carrying the relative time and, when it applies, "edited" and "via MCP" for a comment an agent posted. Images attached to the comment are LARGE tiles stacked under the body — full width, capped at 480 tall, radius 12, hairline, reserving their probed aspect ratio — and any other file stays a read-only pill. The card is the thread: its replies sit under the body behind one hairline, each with a 20px avatar and the same header, and every top-level card closes with a muted "Leave a reply…" row — on web and desktop the composer opens in its place, on mobile it hands the docked composer a "Replying to" target.`,
    status: {
      web: ok(`RegularCommentRow`, `apps/web/src/components/comment-rows/regular.tsx`),
      desktop: ok(`comments::comment_row`, `apps/desktop/crates/ui/src/comments.rs`),
      ios: ok(
        `RegularCommentRow`,
        `apps/ios/Exponential/UI/Issue/CommentThreadView.swift`
      ),
      android: ok(
        `RegularCommentRow`,
        `apps/android/app/src/main/java/com/exponential/app/ui/issue/RegularCommentRow.kt`
      ),
    },
    render: () =>
      [
        `<div class="cmp-comment">`,
        avatar(`AL`, 3),
        `<div class="card">`,
        `<div class="header"><span class="name">Ada Lovelace</span><span class="caption">2 days ago · edited</span></div>`,
        `<div class="text">Pushed the fix. The strip only reserved height once the image had decoded.</div>`,
        `<div class="image"></div>`,
        pill(`trace.txt`, { mode: `readonly`, glyph: svgPaperclip }),
        `<div class="replies">`,
        `<div class="reply">`,
        avatar(`JK`, 2),
        `<div class="reply-body">`,
        `<div class="header"><span class="name">Jonas Klein</span><span class="caption">1 day ago · via MCP</span></div>`,
        `<div class="text">Confirmed on the device farm, cold start is under target now.</div>`,
        `</div>`,
        `</div>`,
        `<div class="reply-row">Leave a reply…</div>`,
        `</div>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `sheet`,
    title: `Sheet shell`,
    kind: `Surfaces`,
    blurb: `Top radius 24 over the page's bottom gradient, a card hairline, a 36×4 grabber. Header gutter 20, content gutter 16. Dismissal is the grabber drag or the backdrop — the header's trailing slot holds an optional ACTION, never a Cancel — and the bottom carries exactly one primary.`,
    status: {
      web: ok(`SheetContent side="bottom"`, `packages/ui/src/sheet.tsx`),
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
    blurb: `The chat-sized set the steer feed is built from. Narration is bare text at 90% behind a 12px glyph at 50% — no bubble, because a wall of them is unreadable. The person's turn IS a bubble: radius 12, active fill, strong hairline. Plan and question share ONE neutral radius-16 card; only the header line is tinted, primary for a plan and yellow for a question. Its options are full-width rows: the promoted one wears the primary fill, a pick the glass active fill, never blue; a free-text row opens the composer card inline under itself. A tool line is a 12px label with a truncated mono detail at 50%, and any long block clamps at 160 behind Show more. Inline code is tinted in chat feeds only — the issue and comment renderers keep the neutral chip.`,
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
      web: ok(
        `DropdownMenuContent`,
        `packages/ui/src/dropdown-menu.tsx`,
        `MENU_SURFACE_CLASS paints the three panels too: mention-textarea, markdown-editor's #/@ list, steer-command-menu.`
      ),
      desktop: ok(
        `theme::exponential_dark (accent = glass fillActive)`,
        `apps/desktop/crates/theme/src/lib.rs`,
        `PopupMenu reads theme.accent; EXP-811 points it at the glass active fill.`
      ),
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
    blurb: `A floating capsule: padding 4 inside a strong hairline, over the OPAQUE card fill. Items are 44px circles; the active one takes the active fill. On a board the detached slot is one 52px capsule with two arms, Start chat | New issue, split by a hairline; elsewhere a single circle.`,
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
      web: ok(`Separator`, `packages/ui/src/separator.tsx`, `Inside a group the same hairline comes from GlassGroup's divide-y, not from a Separator element.`),
      desktop: ok(`surface::glass_row_divider`, DESKTOP_SURFACE),
      ios: ok(`GlassDivider`, IOS_THEME),
      android: ok(`GroupDivider`, ANDROID_SHEET_ROWS),
    },
    island: () => (
      <div className="grid justify-items-start gap-3">
        <Pill>above</Pill>
        <Separator className="bg-glass-stroke" />
        <Pill>below</Pill>
      </div>
    ),
  },
  {
    id: `tokens-fills`,
    title: `Fills`,
    kind: `Tokens`,
    blurb: `The four white-alpha fills, on the page gradient they are designed against. Section under row under card under active — never a fifth step.`,
    status: {
      web: ok(`--glass-fill-*`, `packages/ui/src/styles.css`),
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
      web: ok(`--glass-stroke-*`, `packages/ui/src/styles.css`),
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
    blurb: `Six steps. Row 10, group and field 12, card 16, sheet 24 — anything else is a mistake, and capsules use 9999 rather than a step. MD does double duty as the PICKER corner (EXP-771): an icon or colour picker trigger and every cell of the glyph grid take it, which is what keeps a picker from reading as a circular action button.`,
    status: {
      web: ok(`--radius`, `packages/ui/src/styles.css`),
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
        `packages/ui/src/button.tsx`,
        `32 and 24 are the pill's md and sm, in packages/ui/src/pill.tsx`
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
      web: ok(`--motion-*`, `packages/ui/src/styles.css`),
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
  {
    id: `relations-card`,
    title: `Relations card`,
    kind: `Grouped list`,
    blurb: `EXP-736: the issue's relations beneath the properties card (web + desktop) or inside the properties sheet (phones). Section header with the Add relation capsule, then one row per link: status glyph, the per-side label, identifier, title, trailing remove.`,
    status: {
      web: ok(`IssueRelationsCard`, `apps/web/src/components/issue-relations-card.tsx`),
      desktop: ok(`issue_relations::render_relations_card`, `apps/desktop/crates/ui/src/issue_relations.rs`),
      ios: ok(`IssueRelationsSection`, `apps/ios/Exponential/UI/Issue/Sheets/IssueRelationsSection.swift`, `Lives in the properties sheet, not on the detail page.`),
      android: ok(`RelationsSection`, `apps/android/app/src/main/java/com/exponential/app/ui/issue/RelationsSection.kt`, `Lives in the properties sheet, not on the detail page.`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        sectionHeader(`Relations`, pill(`Add relation`, { glyph: svgPlus })),
        group(
          relationRow(`blocked by`, `EXP-612`, `ACP: agent client protocol for the steer channel`),
          relationRow(`sub-issue of`, `EXP-723`, `desktop to web approaching`),
          relationRow(`related to`, `EXP-736`, `issue relation`)
        ),
        `</div>`,
      ].join(``),
  },
  {
    id: `github-connection`,
    title: `GitHub connection`,
    kind: `Grouped list`,
    blurb: `FEED-42: the same block on all four clients. The status sits BEFORE the repo list, every inline action is an sm action pill, the ✕ is always visible and confirms, and accounts + stale marks come from integrations.github.status.`,
    status: {
      web: ok(`GithubStatusLine`, `apps/web/src/components/team/repositories-section.tsx`),
      desktop: ok(`RepositoriesPane::github_status_line`, `apps/desktop/crates/ui/src/settings/repositories.rs`),
      ios: ok(`TeamRepositoriesSection`, `apps/ios/Exponential/UI/Settings/TeamRepositoriesSection.swift`, `Lives in the native Team settings screen.`),
      android: ok(`RepositoriesSection`, `apps/android/app/src/main/java/com/exponential/app/ui/settings/TeamSettingsScreen.kt`, `Lives in the native Team settings screen.`),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<div>`,
        sectionHeader(`Repositories`, pill(`Add repository`, { glyph: svgGithub })),
        `<p class="cmp-github-caption">Connect a GitHub account or organization first, then add its repositories to share them with the team — everyone can code on a shared repo. Point a board at one to make it the clone target for “Start coding”.</p>`,
        `</div>`,
        `<div class="cmp-github-status">`,
        githubLine(`<span class="cmp-github-dot"></span>`, `GitHub accounts connected to this team`),
        `<div class="cmp-github-accounts">`,
        githubAccount(svgBuilding, `acme`),
        githubAccount(svgUser, `octocat`),
        `</div>`,
        `<p class="cmp-github-caption cmp-github-indent">An installation is per GitHub account or organization. Repositories come from the accounts listed here.</p>`,
        `<div class="cmp-github-actions">`,
        pill(`Connect another account`, { glyph: svgPlus }),
        pill(`Refresh access`, { glyph: svgRefresh }),
        `</div>`,
        githubLine(
          svgTriangleAlert,
          `Reconnect GitHub to refresh which repositories you can access from octocat.`,
          [pill(`Reconnect`)]
        ),
        githubLine(
          svgTriangleAlert,
          `No one’s GitHub connection covers installation 42 anymore — reconnecting can’t refresh it.`,
          [pill(`Disconnect account`)],
          true
        ),
        `</div>`,
        `<div class="cmp-github-status">`,
        githubLine(svgGithub, `No GitHub account connected`, [
          pill(`Connect GitHub`, { glyph: svgGithub }),
          pill(`Install on an account`, { glyph: svgPlus }),
        ]),
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `repo-picker`,
    title: `Add-repository picker`,
    kind: `Grouped list`,
    blurb: `FEED-42: tapping a row or a successful Add by name adds at once. The suspended and re-auth banners are independent, and add errors show inline while the picker stays open.`,
    status: {
      web: ok(`GithubRepoPicker`, `apps/web/src/components/github-repo-picker.tsx`),
      desktop: ok(`add_repository_dialog::footer`, `apps/desktop/crates/ui/src/settings/add_repository_dialog.rs`),
      ios: ok(`GithubRepoPicker`, `apps/ios/Exponential/UI/Settings/GithubRepoPicker.swift`, `A sheet; a plan-limit add shows no web upgrade pointer (EXP-216).`),
      android: ok(`GithubRepoPickerSheet`, `apps/android/app/src/main/java/com/exponential/app/ui/onboarding/GithubRepoPickerSheet.kt`, `A sheet; a plan-limit add shows no web upgrade pointer (EXP-216).`),
    },
    render: () =>
      [
        `<div class="cmp-stack cmp-repo-picker">`,
        `<div class="cmp-repo-picker-banner">`,
        svgTriangleAlert,
        `<span class="cmp-github-text">Reconnect GitHub (octocat) to refresh. Repos created or shared with you since your last connect won’t appear until you do.</span>`,
        pill(`Reconnect GitHub`, { glyph: svgRefresh }),
        `</div>`,
        `<input class="cmp-text-field" type="text" placeholder="Search repositories…" readonly>`,
        group(
          repoPickerRow(`acme/web`),
          repoPickerRow(`acme/billing`, true),
          repoPickerRow(`octocat/dotfiles`)
        ),
        `<div class="cmp-repo-picker-footer">`,
        `<p class="cmp-github-caption">Only repositories your GitHub installation grants appear here. Missing one? Grant it on GitHub, then refresh. `,
        githubLink(`acme`),
        `, `,
        githubLink(`octocat`),
        `</p>`,
        `<p class="cmp-github-caption">Showing the first 500 repositories per account — use the field below for the rest.</p>`,
        `<div class="cmp-github-actions">`,
        pill(`Refresh`, { glyph: svgRefresh }),
        pill(`Install on another account`, { glyph: svgPlus }),
        `</div>`,
        `<div class="cmp-repo-picker-lookup">`,
        `<input class="cmp-text-field" type="text" placeholder="owner/name" aria-label="Add repository by name" readonly>`,
        pill(`Look up`),
        `</div>`,
        `<p class="cmp-repo-picker-error">Repository not found, or no connected installation grants it.</p>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
]
