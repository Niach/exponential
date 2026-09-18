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
// The concept → glyph map itself. `@exp/ui` re-exports the RESOLVER
// (`conceptIcon`) but not the table, and this entry documents the table; the
// package rides in with `@exp/ui`, which owns the generated file shown here.
import { SEMANTIC_ICONS } from "@exp/icons"
import { contract } from "@exp/domain-contract"
import { parseDiff } from "@exp/domain-contract/diff"
import { editCard } from "@exp/domain-contract/edit-card"
import {
  AgentBrandMark,
  AgentPicker,
  AgentPickerTabs,
  Alert,
  AlertDescription,
  AlertTitle,
  AttachmentThumb,
  AuthFormShell,
  Badge,
  BoardGlyph,
  Button,
  Calendar,
  ChangesFileSheet,
  ClaudeIcon,
  CodexIcon,
  Checkbox,
  Combobox,
  ComboboxList,
  ColorPicker,
  ColorSwatchGrid,
  Composer,
  ComposerSubmit,
  ComposerTool,
  ContextRing,
  CursorIcon,
  DatePicker,
  DisclosureHeader,
  EditedFilesCard,
  EmojiPicker,
  EmptyCta,
  ExponentialLogo,
  FAB_CHROME_CLASS,
  FabButton,
  FileDiffCard,
  FileDiffTree,
  GlassCard,
  GlassGroup,
  GlassInputRow,
  GlassRow,
  GlassSectionHeader,
  GlassTabsRow,
  GlassToggleRow,
  EmptyState,
  ICON_DISC_TONES,
  IconDisc,
  DEVICE_ICON_OPTIONS,
  IconPicker,
  IconSwatchGrid,
  Input,
  IssueGroupBand,
  IssueChip,
  Label,
  LiveDot,
  ListRow,
  ListEmpty,
  Meter,
  MobileWorkCapsule,
  OpenAiIcon,
  PasswordInput,
  Pill,
  PrGithubButton,
  PreviewMedia,
  RUN_TITLE_CLASS,
  RichTab,
  SearchField,
  SegmentedControl,
  SessionResultsView,
  Select,
  SelectTrigger,
  SelectValue,
  Separator,
  Skeleton,
  StatusGlyph,
  Switch,
  TabsTrigger,
  TeamAvatar,
  Textarea,
  TypeaheadMenu,
  TypeaheadRow,
  UserAvatar,
  WORK_COLUMN_CLASS,
  WorkHeader,
  conceptIcon,
  indexEmojiData,
  type EmojiDataset,
  type PickerOption,
  type SessionResultEntry,
  type StatusGlyphProps,
} from "@exp/ui"

import { tokenSlug } from "./component-styles.ts"
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
  svgInbox,
  svgListTodo,
  svgLock,
  svgMessageCircle,
  svgPaperclip,
  svgPlay,
  svgPlus,
  svgRefresh,
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

/**
 * EXP-941: the page has THREE modes, and `kind` is the sub-group INSIDE one.
 * Views are the photographed catalog; Components are the controls; Style is
 * the values every control is built from. A spec carries no mode field —
 * `modeOf` derives it from the kind, so a mis-sorted entry is impossible
 * rather than merely gated.
 */
export type ComponentKind =
  | `Inputs & pickers`
  | `Buttons & chips`
  | `Lists & rows`
  | `Surfaces`
  | `Feedback`
  | `Colour`
  | `Shape & size`
  | `Type`
  | `Motion`
  | `Icons`

export type Mode = `views` | `components` | `style`

export interface ModeInfo {
  id: Mode
  label: string
  blurb: string
}

export const MODES: Record<Mode, ModeInfo> = {
  views: {
    id: `views`,
    label: `Views`,
    blurb: `Every screen in the catalog, photographed on all five platforms.`,
  },
  components: {
    id: `components`,
    label: `Components`,
    blurb: `The glass control set, rendered live from @exp/ui and the design tokens — not photographed.`,
  },
  style: {
    id: `style`,
    label: `Style`,
    blurb: `The values the controls are made of: colour, shape, type, motion and the icon registry.`,
  },
}

/** Kept as the name the renderer and the tests already use for the mode. */
export const COMPONENTS_GROUP = MODES.components

/** The kinds that live under Style; every other kind is a Component. */
export const STYLE_KINDS: readonly ComponentKind[] = [
  `Colour`,
  `Shape & size`,
  `Type`,
  `Motion`,
  `Icons`,
]

/** Nav order within a mode. The union of both lists IS `ComponentKind`. */
export const KIND_ORDER: Record<Exclude<Mode, `views`>, readonly ComponentKind[]> = {
  components: [`Inputs & pickers`, `Buttons & chips`, `Lists & rows`, `Surfaces`, `Feedback`],
  style: STYLE_KINDS,
}

export function modeOf(spec: { kind: ComponentKind }): Exclude<Mode, `views`> {
  return STYLE_KINDS.includes(spec.kind) ? `style` : `components`
}

interface ComponentSpecBase {
  id: string
  title: string
  kind: ComponentKind
  blurb: string
  status: Record<ComponentPlatform, ComponentStatus>
  /**
   * Web call sites that still draw this semantic BY HAND (EXP-941). The status
   * table says whether a platform HAS the control; this says where the product
   * ignores it. Rendered under the table as "Still drawn by hand" and counted
   * as one extra yellow dot in the nav, so the page keeps naming what is wrong
   * instead of quietly showing the happy path. Gated: the file exists, the
   * note is one line of ≤120 chars.
   */
  leftovers?: { file: string; note: string }[]
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
 * The three entries whose web symbol DOES live in `packages/ui` and still keep
 * a hand-written demo: a closed Radix portal renders nothing at all to static
 * markup, so an island of any of them would be an empty box. Anything else
 * under `packages/ui/` must be an island — `components.test.tsx` gates both
 * directions, and exempts only these and the Style entries, which document a
 * VALUE rather than a control.
 */
export const PORTAL_ONLY_IDS: readonly string[] = [`sheet`, `menu`, `dialog`]

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
const BoldGlyph = conceptIcon(`editor-bold`)
const WarningGlyph = conceptIcon(`ui-warning`)
const ShellGlyph = conceptIcon(`session-shell`)
const MergeGlyph = conceptIcon(`pr-merged`)
const EditorImageGlyph = conceptIcon(`editor-image`)
const AttachGlyph = conceptIcon(`ui-attach`)
const IssueRefGlyph = conceptIcon(`editor-issue-ref`)
const EmojiGlyph = conceptIcon(`editor-emoji`)
const PropertiesGlyph = conceptIcon(`ui-properties`)
const CommentGlyph = conceptIcon(`notification-issue-comment`)
const WorkFacesGlyph = conceptIcon(`work-faces`)
const DraftsGlyph = conceptIcon(`nav-drafts`)
const PinGlyph = conceptIcon(`ui-pin`)
const ToolGlyph = conceptIcon(`coding-tool`)
const ActionCreateGlyph = conceptIcon(`action-create`)

/* A neutral stand-in for a picked screenshot: the island loads no network
   image, so the thumb's crop and hairline read against a flat data-URI tile. */
/* The two option shapes the app really hands a picker (EXP-941): a person,
   whose email is the SEARCH text and not the label, and a label, whose colour
   is a dot. `value` is the identity — two boards may share a name — so a
   duplicate label is never a bug. */
const ASSIGNEE_OPTIONS: PickerOption[] = [
  { value: `mina`, label: `Mina Kay`, keywords: [`Mina Kay`, `mina@example.com`], hint: `you` },
  { value: `jonas`, label: `Jonas Stern`, keywords: [`Jonas Stern`, `jonas@example.com`] },
  { value: `sam`, label: `Sam Lee`, keywords: [`Sam Lee`, `sam@example.com`] },
]

const LABEL_OPTIONS: PickerOption[] = [
  { value: `bug`, label: `bug`, dot: `#ef4444` },
  { value: `mobile`, label: `mobile`, dot: `#3b82f6` },
  { value: `design`, label: `design`, dot: `#a855f7` },
  { value: `docs`, label: `docs`, dot: `#22c55e` },
]

/* The same labels mid bulk-edit (EXP-957): `checked` overrides membership in
   `value` for the glyph, so "design" reads as on-some without being picked. */
const BULK_LABEL_OPTIONS: PickerOption[] = LABEL_OPTIONS.map((option) =>
  option.value === `design` ? { ...option, checked: `indeterminate` } : option
)

/* Local midnight, the way `parseDateValue` reads the wire format — `new
   Date("2026-03-08")` alone is UTC and renders the 7th west of Greenwich. */
const DUE_DATE_FIXTURE = new Date(`2026-03-08T00:00:00`)

/* The whole registry, in the generator's own order: concept id beside the
   Lucide glyph it resolves to. Read from the map rather than transcribed —
   a hand-written table is exactly the drift this entry exists to catch. */
const ICON_CONCEPTS = Object.entries(SEMANTIC_ICONS) as [
  Parameters<typeof conceptIcon>[0],
  string,
][]

const THUMB_FIXTURE_SRC = `data:image/svg+xml,${encodeURIComponent(`<svg xmlns="http://www.w3.org/2000/svg" width="96" height="96"><rect width="96" height="96" fill="gray"/></svg>`)}`

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

function swatch(name: string, value: string, box: string): string {
  return [
    `<div class="cmp-swatch">`,
    `<span class="box ${box}"></span>`,
    `<span class="name">${escapeHtml(name)}</span>`,
    `<span class="value">${escapeHtml(value)}</span>`,
    `</div>`,
  ].join(``)
}

/**
 * One `.cmp-swatches` table for a whole token colour GROUP (EXP-941). The box
 * class is `fill-<prefix>-<key>`, whose rule and var are BOTH generated from
 * the same object in `component-styles.ts` / `styles.ts` — so a new token
 * appears here, in the stylesheet and in `:root` at once, and a demo still
 * carries no colour literal.
 */
function swatchGroup(prefix: string, group: Record<string, string>): string {
  const rows = Object.entries(group)
    .filter(([key]) => !key.startsWith(`$`))
    .map(([key, value]) => swatch(key, value, `fill-${prefix}-${tokenSlug(key)}`))
    .join(``)
  return `<div class="cmp-swatches">${rows}</div>`
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
 * The control EXISTS on that platform and still disagrees with this form. The
 * note says HOW, in one line, because a table that only ever says `ok` is a
 * table nobody reads.
 */
function leftover(symbol: string, file: string, note: string): ComponentStatus {
  return { state: `leftover`, symbol, file, note }
}

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

/* ----------------------------------------------------- EXP-916 diff files */

const WEB_DIFF_CARD = `packages/ui/src/file-diff-card.tsx`
const WEB_EDIT_CARD = `packages/ui/src/edited-files-card.tsx`
const WEB_DIFF_TREE = `packages/ui/src/file-diff-tree.tsx`
const DESKTOP_DIFF = `apps/desktop/crates/ui/src/diff.rs`
const DESKTOP_SESSION_EXTRAS = `apps/desktop/crates/ui/src/session_extras.rs`
const DESKTOP_DIFF_PANE = `apps/desktop/crates/ui/src/diff_pane.rs`
const IOS_DIFF_CARD = `apps/ios/Exponential/UI/Issue/DiffFileCard.swift`
const IOS_EDIT_CARD = `apps/ios/Exponential/UI/Session/EditedFilesCard.swift`
const IOS_DIFF_TREE = `apps/ios/Exponential/UI/Issue/DiffFileTree.swift`
const ANDROID_DIFF_CARD = `apps/android/app/src/main/java/com/exponential/app/ui/issue/DiffFileCard.kt`
const ANDROID_EDIT_CARD = `apps/android/app/src/main/java/com/exponential/app/ui/session/EditedFilesCard.kt`
const ANDROID_DIFF_TREE = `apps/android/app/src/main/java/com/exponential/app/ui/issue/DiffFileTree.kt`

/** A tiny two-hunk patch — the resting state of one file card. */
const DIFF_CARD_PATCH = [
  `--- a/packages/ui/src/file-diff-card.tsx`,
  `+++ b/packages/ui/src/file-diff-card.tsx`,
  `@@ -12,4 +12,5 @@`,
  ` const COLLAPSE_THRESHOLD = contract.diffUi.collapseThresholdLines`,
  `-const LINE_CHUNK = 500`,
  `+const LINE_CHUNK = contract.diffUi.lineChunk`,
  `+const HIGHLIGHT_LIMIT = 1500`,
  ` `,
  `@@ -48,3 +49,3 @@`,
  ` export function FileDiffCard({`,
  `-  file,`,
  `+  file, state = \`ready\`,`,
  ` }) {`,
].join(`\n`)

const FIXTURE_FILE = parseDiff(DIFF_CARD_PATCH).files[0]

/** Three edited files, the last one still open — an agent mid-run. */
const EDIT_ITEMS = [
  `packages/ui/src/file-diff-card.tsx`,
  `packages/ui/src/file-diff-tree.tsx`,
  `apps/web/src/components/agent-session.tsx`,
].map((path, index) => ({
  id: index + 1,
  kind: `tool`,
  toolKind: `edit`,
  detail: path,
  settled: true,
  diff: [
    `--- a/${path}`,
    `+++ b/${path}`,
    `@@ -1,3 +1,4 @@`,
    ` import { cn } from "./cn"`,
    `-const LINE_CHUNK = 500`,
    `+const LINE_CHUNK = contract.diffUi.lineChunk`,
    `+`,
  ].join(`\n`),
}))

const EDIT_CARD_VIEW = editCard(EDIT_ITEMS, 3)

/** Six files across three directories — one of them a compacted chain. */
const TREE_FILES = [
  `packages/ui/src/file-diff-card.tsx`,
  `packages/ui/src/file-diff-tree.tsx`,
  `packages/ui/src/edited-files-card.tsx`,
  `apps/web/src/components/agent-session.tsx`,
  `apps/web/src/lib/agent-feed.ts`,
  `README.md`,
].map((path, index) =>
  parseDiff(
    [
      `--- a/${path}`,
      `+++ b/${path}`,
      `@@ -1,2 +1,${index + 2} @@`,
      ` kept`,
      ...Array.from({ length: index + 1 }, (_, i) => `+added ${i}`),
    ].join(`\n`)
  ).files[0]
)

/* EXP-551: a six-emoji stand-in for the generated dataset. Only the groups
   that carry an emoji are listed — the picker renders one band per group, and
   nine empty bands would be a specimen of nothing. The labels are the
   generator's own (`EMOJI_GROUP_LABELS`), spelled out here so the styleguide
   keeps no dependency on the dataset package. */
const EMOJI_FIXTURE: EmojiDataset = {
  version: `16.0.0`,
  groups: [`Smileys & emotion`, `Animals & nature`],
  emojis: [
    { u: `\u{1F600}`, l: `grinning face`, g: 0, s: [`grinning`], t: [`smile`] },
    { u: `\u{1F602}`, l: `face with tears of joy`, g: 0, s: [`joy`], t: [`laugh`] },
    { u: `\u{1F389}`, l: `party popper`, g: 0, s: [`tada`], t: [`celebration`] },
    { u: `\u{1F436}`, l: `dog face`, g: 1, s: [`dog`], t: [`pet`] },
    { u: `\u{1F431}`, l: `cat face`, g: 1, s: [`cat`], t: [`pet`] },
    { u: `\u{1F984}`, l: `unicorn`, g: 1, s: [`unicorn`], t: [`fantasy`] },
  ],
}

/* EXP-879: two topics, two labels each — the shape a run publishes when it
   shot one screen on two platforms. The probed size gives each tile its
   aspect; the src is the flat data-URI tile, because an island loads nothing. */
const SESSION_RESULTS_FIXTURE: SessionResultEntry[] = [
  { topic: `Issue header`, label: `web`, attachmentId: `a1`, width: 1440, height: 900 },
  { topic: `Issue header`, label: `ios`, attachmentId: `a2`, width: 390, height: 844 },
  { topic: `Emoji picker`, label: `web`, attachmentId: `a3`, width: 1440, height: 900 },
  { topic: `Emoji picker`, label: `android`, attachmentId: `a4`, width: 412, height: 915 },
]

/* -------------------------------------------------------------- the specs */

export const COMPONENTS: readonly ComponentSpec[] = [
  {
    id: `section-header`,
    title: `Group band`,
    kind: `Lists & rows`,
    blurb: `EXP-818: the Linear group header — a full-width strip on the section fill, radius 10, padding 6/12, 14/20 at 85% foreground, a trailing slot, 4px over its flat rows. No count. Never uppercase and never a divider. A band heads a LIST; the bare fold INSIDE a row is the disclosure header below, which draws no strip at all.`,
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
    id: `disclosure-header`,
    title: `Disclosure header`,
    kind: `Lists & rows`,
    blurb: `EXP-962: the fold toggle INSIDE a row, and the whole of it is one line of bare text — a 12px chevron pointing right folded and down open, the label muted and brightening under the pointer, \`aria-expanded\` stating the fold, the entire line the target. The steer feed's tool groups, its Exponential calls, its subagent lanes and its long bodies, the workflow card's agents and the issue group's own header each drew this by hand before it was one component. \`chevron="trailing"\` parks the glyph at the far edge instead, for a row whose siblings carry none and must not indent out of line with them. It is NOT the group band above: that is a filled strip heading a LIST. And it may not contain another button — a fold's own action renders beside it, because a button inside a button is invalid markup.`,
    status: {
      web: ok(`DisclosureHeader`, `packages/ui/src/disclosure-header.tsx`),
      desktop: ok(
        `controls::disclosure_header`,
        DESKTOP_CONTROLS,
        `EXP-963: the steer feed's tool groups, Exponential runs, subagent lanes and workflow agents fold on it`
      ),
      ios: leftover(
        `ToolGroupRow / ExpToolGroupRow / SubagentGroupRow`,
        `apps/ios/Exponential/UI/Session/AgentSessionView.swift`,
        `three private structs repeat the 11pt chevron row, each with its own @State expanded`
      ),
      android: leftover(
        `ToolGroupRow / ExpToolGroupRow / SubagentGroupRow`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt`,
        `the same three private composables, each rebuilding the chevron row`
      ),
    },
    island: () => (
      <div className="grid gap-4 text-xs">
        <DisclosureHeader open={false} onToggle={noop}>
          <ToolGlyph className="size-3 shrink-0" />
          <span className="min-w-0 truncate">Ran 3 commands · edited 2 files</span>
        </DisclosureHeader>
        <div>
          <DisclosureHeader open onToggle={noop}>
            <ToolGlyph className="size-3 shrink-0" />
            <span className="min-w-0 truncate">Ran 3 commands · edited 2 files</span>
          </DisclosureHeader>
          <div className="mt-1 pl-5 font-mono text-muted-foreground">bun run test:shots</div>
        </div>
        {/* The trailing arm: the label keeps the row's own left edge. */}
        <DisclosureHeader open={false} onToggle={noop} chevron="trailing">
          <span className="min-w-0 truncate">Output · 128 lines</span>
        </DisclosureHeader>
      </div>
    ),
  },
  {
    id: `group`,
    title: `Group container`,
    kind: `Lists & rows`,
    blurb: `Borderless: radius 12, the row fill, hairline separators between children, overflow hidden. The fill is the edge — no outer stroke.`,
    status: {
      web: ok(`GlassGroup`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_group / glass_group_rows`, DESKTOP_SURFACE),
      ios: ok(`GlassSection`, IOS_THEME),
      android: ok(`Modifier.glassGroup()`, ANDROID_GLASS, `OptionGroup in ui/components/SheetOptionRows.kt is the list wrapper around it.`),
    },
    island: () => (
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Repository"
          value="exp"
          onChange={noop}
          options={[{ value: `exp`, label: `niach/exponential` }]}
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
    kind: `Lists & rows`,
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
    kind: `Lists & rows`,
    blurb: `EXP-818: the flat list item every list wears — no stroke, no fill, radius 10, padding 12, NO gap between rows under a group band; hover takes the row fill, the selected row the active fill. Rows read as a table, not as cards. EXP-962 gave it a second density: \`compact\` is the 28px one-line row the narrow column runs at (the sidebar's pinned and draft arms, the compact inbox) — the same 14px type, 8px of side padding, 8px to the glyph — and \`SidebarMenuButton density="compact"\` is its exact twin, so a nav entry and a list row sitting in the same 17rem slot are the same height.`,
    status: {
      web: ok(`ListRow`, WEB_GLASS_ROWS),
      desktop: ok(
        `surface::flat_row / flat_row_compact`,
        DESKTOP_SURFACE,
        `EXP-963: flat_row_compact is the 28px density the rail's entries run at`
      ),
      ios: ok(`FlatRow / .flatRow()`, IOS_THEME),
      android: ok(`Modifier.flatRow()`, ANDROID_GLASS),
    },
    leftovers: [
      { file: `apps/web/src/components/team/board-switcher-sheet.tsx`, note: `PLAIN_ROW re-derives the mobile picker row` },
    ],
    island: () => (
      <div className="grid gap-4">
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
        {/* The sidebar's rung: 28px, one line, the same type. */}
        <div>
          <GlassSectionHeader label="Pinned" />
          <ListRow density="compact" interactive>
            <PinGlyph className="size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">APP-14 · Fix the merge queue</span>
          </ListRow>
          <ListRow density="compact" interactive>
            <PinGlyph className="size-4 shrink-0 text-muted-foreground" />
            <span className="min-w-0 flex-1 truncate">APP-15 · Ship the usage sheet</span>
          </ListRow>
        </div>
      </div>
    ),
  },
  {
    id: `row-shell`,
    title: `Row shell`,
    kind: `Lists & rows`,
    blurb: `The rhythm every grouped row inherits: padding 12/16, gap 12, 14px text. The shell never draws a stroke — the group's hairlines do.`,
    status: {
      web: ok(`GlassInputRow / GlassToggleRow / Combobox triggerVariant="row"`, WEB_GLASS_ROWS),
      desktop: ok(`surface::glass_row_shell`, DESKTOP_SURFACE),
      ios: ok(`GlassPickerRow`, IOS_OPTION_ROWS),
      android: ok(`PickerRow`, ANDROID_SHEET_ROWS),
    },
    island: () => (
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Agent"
          value="claude"
          onChange={noop}
          options={[{ value: `claude`, label: `claude` }]}
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
    kind: `Lists & rows`,
    blurb: `Label left, value right-aligned at 70% foreground, a 14px chevron at 50%. The whole row is the target, never just the value.`,
    status: {
      web: ok(
        `Combobox triggerVariant="row"`,
        `packages/ui/src/combobox.tsx`,
        `EXP-958: the row IS the picker — its own Select and sheet are gone`
      ),
      desktop: ok(`surface::glass_picker_row`, DESKTOP_SURFACE),
      ios: ok(`GlassPickerRow`, IOS_OPTION_ROWS),
      android: ok(`PickerRow`, ANDROID_SHEET_ROWS),
    },
    island: () => (
      <GlassGroup>
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Status"
          value="in_review"
          onChange={noop}
          options={[{ value: `in_review`, label: `In review` }]}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Assignee"
          value="danny"
          onChange={noop}
          options={[{ value: `danny`, label: `Danny` }]}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="Due date"
          value={null}
          onChange={noop}
          options={[]}
          triggerLabel="No date"
        />
      </GlassGroup>
    ),
  },
  {
    id: `input-row`,
    title: `Input row`,
    kind: `Lists & rows`,
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
    kind: `Lists & rows`,
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
    kind: `Lists & rows`,
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
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="APP-14"
          value="two"
          onChange={noop}
          options={[{ value: `two`, label: `2 files` }]}
        />
        <Combobox
          triggerVariant="row"
          searchable={false}
          mobileTitle="APP-21"
          value="seven"
          onChange={noop}
          options={[{ value: `seven`, label: `7 files` }]}
        />
      </GlassGroup>
    ),
  },
  {
    id: `segmented`,
    title: `Segmented control`,
    kind: `Inputs & pickers`,
    blurb: `The standalone capsule: 36 tall, padding 3, the section fill under a section stroke. Segments share the embedded row's geometry — EXP-941 made that second form a prop rather than a second component, so the settings strips and the free-floating ones are one control. Eight strips wired their own Tabs + TabsList + N triggers by hand before, and drifted in padding and in whether a segment carried a glyph; the class recipe still lives in tabs.tsx, which other surfaces read directly.`,
    status: {
      web: ok(
        `SegmentedControl`,
        `packages/ui/src/segmented-control.tsx`,
        `the SEGMENTED_* class constants stay in tabs.tsx byte-identical; this renders the strip from an option array`
      ),
      desktop: ok(`controls::segmented`, DESKTOP_CONTROLS),
      ios: ok(`GlassSegmentedControl`, IOS_SEGMENTED),
      android: ok(`GlassSegmentedControl`, `${ANDROID_COMPONENTS}/GlassSegmentedControl.kt`),
    },
    island: () => (
      <div className="grid gap-4">
        <SegmentedControl
          value="issues"
          onValueChange={noop}
          options={[
            { value: `issues`, label: `Issues` },
            { value: `actions`, label: `Actions` },
            { value: `automations`, label: `Automations` },
          ]}
        />
        {/* `embedded` is the SAME control as a glass group's first row — see
            embedded tabs row, which is this arm inside its group. */}
        <GlassGroup>
          <SegmentedControl
            embedded
            value="open"
            onValueChange={noop}
            options={[
              { value: `open`, label: `Open` },
              { value: `merged`, label: `Merged` },
            ]}
          />
          <GlassRow interactive>
            <span className="min-w-0 flex-1 truncate">EXP-941 · Styleguide foundation</span>
            <Pill>in review</Pill>
          </GlassRow>
        </GlassGroup>
      </div>
    ),
  },
  {
    id: `rich-tab`,
    title: `Rich tab`,
    kind: `Buttons & chips`,
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
    kind: `Surfaces`,
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
    kind: `Buttons & chips`,
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
    kind: `Buttons & chips`,
    blurb: `The SECONDARY icon button (EXP-862): the same 32px box and the same 16px glyph at 70% foreground, with no circle, no fill and no border at rest. Hover is the only paint it carries, the row wash under the MD corner, and the glyph goes full strength; a toggle that is ON says so with aria-pressed and keeps that wash (the editor rail's marks, EXP-960). Everything that is not the primary action wears this one: the "…" overflow, close, the folder and file-list toggles, the chevrons (back, fold, reorder), trash and remove, refresh. Put a circle here and the surface ends up with three things asking to be pressed and no way to tell which one it wants.`,
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
      // The last one is a TOGGLE held down: `aria-pressed` is the pressed
      // state (EXP-960), the hover wash kept on — the editor rail's marks.
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
        <Button variant="ghost" size="icon-sm" aria-label="Bold" aria-pressed>
          <BoldGlyph />
        </Button>
      </div>
    ),
  },
  {
    id: `fab-button`,
    title: `Floating circle`,
    kind: `Buttons & chips`,
    blurb: `The third circle, and the only one that FLOATS: 52px of the floating chrome around a 20px glyph, pressed down to the active wash. EXP-962 made it a component — three files had restated the circle, one of them spelling 52px as 3.25rem — so the tab bar's FAB, the issue bar's coding circle and every slot of the Work bar are now one button. Its glyph rides at the SECONDARY emphasis, 70% white, because a bar of equal circles has no hierarchy to spend; the one slot that IS the bar's call to action says \`emphasis="primary"\` and goes full white. The 32px icon button above lives in a row; this one hangs over the content, and only a phone has one. \`FAB_CIRCLE_CLASS\` is the same shape for the two slots that are not buttons (the usage ring, the capsule).`,
    status: {
      web: ok(
        `FabButton / FAB_CIRCLE_CLASS`,
        `packages/ui/src/fab-chrome.tsx`,
        `MOBILE_WORK_CIRCLE_CLASS in mobile-work-bar.tsx is an alias of the class`
      ),
      desktop: na(`no floating phone bar: the IDE's bottom edge is the terminal session bar`),
      ios: ok(
        `FloatingBarCircle`,
        `apps/ios/ExpUI/Sources/FloatingBottomBar.swift`,
        `the glyph's emphasis rides the caller's tint rather than a parameter`
      ),
      android: leftover(
        `Fab`,
        `${ANDROID_COMPONENTS}/BottomNavBar.kt`,
        `the tab bar's FAB paints its 52dp circle inline: opaque fill, strong hairline, a full-white glyph`
      ),
    },
    island: () => (
      <div className="flex items-center gap-3">
        <FabButton aria-label="Properties">
          <PropertiesGlyph className="size-5" />
        </FabButton>
        <FabButton emphasis="primary" aria-label="Start coding">
          <PlayGlyph className="size-5" />
        </FabButton>
        <FabButton aria-label="Merge" disabled>
          <MergeGlyph className="size-5" />
        </FabButton>
      </div>
    ),
  },
  {
    id: `icon-picker`,
    title: `Icon picker`,
    kind: `Inputs & pickers`,
    blurb: `The one surface that picks a glyph, and the one exception to the circle (EXP-771): a circle is the primary ACTION, ROUNDED SQUARE is a picker. The trigger is a square at the radius ladder's MD step over card fill, sized to the field it sits beside (web h-9, desktop and the natives the 32px control rung) — a card hairline once something is picked, a DASHED one under the placeholder glyph while it is empty — and the cells of the grid it opens wear that same corner, the picked one taking the active fill under the active stroke. It offers one of TWO sets from the one registry (EXP-924): the 96 board and action glyphs, or the six device glyphs (monitor, server, laptop and the Apple, Windows and Linux marks) beside a device's name, where the short set hugs its cells. EXP-862 gave the colour picker the SAME trigger, so the board form reads as one control repeated; the swatches inside it are the counter-example: a colour has no shape to read, so those stay circles.`,
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
          <IconPicker
            value="os-linux"
            options={DEVICE_ICON_OPTIONS}
            onChange={noop}
          />
          <Button variant="glass" size="icon-sm" aria-label="New board">
            <PlusGlyph />
          </Button>
        </div>
        {/* 8 × 28px cells + 7 × 6px gaps — the popover's own column count. */}
        <div className="w-[266px]">
          <IconSwatchGrid value="flag" onChange={noop} />
        </div>
        <div className="w-max">
          <IconSwatchGrid
            value="os-linux"
            options={DEVICE_ICON_OPTIONS}
            onChange={noop}
          />
        </div>
      </div>
    ),
  },
  {
    id: `button-primary`,
    title: `Primary submit`,
    kind: `Buttons & chips`,
    blurb: `Full width, padding 14/16, radius 10, solid primary. Disabled drops to card fill with a card stroke and 50% foreground. The specimen is the web's own Button, which is a CAPSULE — the radius-10 rectangle is the mobile sheet submit, as the web row below says.`,
    status: {
      web: ok(
        `Button (variant default)`,
        `packages/ui/src/button.tsx`,
        `web/desktop primaries stay capsules; the radius-10 full-width form is the mobile sheet submit`
      ),
      desktop: ok(`surface::glass_pill_button_primary`, DESKTOP_SURFACE),
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
    id: `text-button`,
    title: `Text button`,
    kind: `Buttons & chips`,
    blurb: `EXP-962: a control made of WORDS — \`size="inline"\`, 12px, no box, no height of its own, sitting in the run of muted text around it. Two variants, and the difference is what happens when it is pressed: \`text\` is muted, brightens under the pointer and never underlines, because it toggles something IN PLACE (a fold's Show more / Show less, "Back to the current step"); \`link\` takes the primary colour and underlines on hover, because it GOES somewhere (a session band's "Continues in a newer run", a stack band's \`↓ #APP-14\`). Anything that wants a box is the pill or the primary submit — four call sites hand-drew one of these two shapes before.`,
    status: {
      web: ok(
        `Button variant="text" / variant="link", size="inline"`,
        `packages/ui/src/button.tsx`
      ),
      desktop: ok(
        `controls::text_button (TextButtonVariant::Text)`,
        DESKTOP_CONTROLS,
        `EXP-963: the output card's and the body's Show more, the edited-files footer ride it`
      ),
      ios: leftover(
        `Button("Show more").buttonStyle(.plain)`,
        `apps/ios/Exponential/UI/Session/AgentSessionView.swift`,
        `inlined twice with its own caption2 font and tertiary opacity; no shared text button exists`
      ),
      android: leftover(
        `ShowMoreToggle`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentSessionScreen.kt`,
        `a private clickable Text on the session screen; nothing else may reach it`
      ),
    },
    island: () => (
      // The band they live in: a muted 11px caption line under a feed row.
      <div className="flex flex-wrap items-center gap-4 text-[11px] text-muted-foreground">
        <Button variant="text" size="inline">
          Show more
        </Button>
        <Button variant="link" size="inline">
          Continues in a newer run · started 2h ago
        </Button>
        <Button variant="link" size="inline" className="font-mono">
          ↓ #APP-14
        </Button>
      </div>
    ),
  },
  {
    id: `pill`,
    title: `Pill`,
    kind: `Buttons & chips`,
    blurb: `The ONE capsule, a 2×3 matrix: size md 32 or sm 24, mode action / select / readonly, plus a primary PAINT flag that crosses all six. Card fill under a card stroke, label at 70% — action and select go active on hover, a selected one also takes the active stroke, readonly is metadata and never a target. There is no chip and no header button: those WERE this, under a second name. A conversation or subagent tab is sm select; a members-list role chip is sm readonly, 12px from its neighbours in a row. A bare COUNT is none of the six: a number with no word beside it is the 16px \`Badge\` below.`,
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
    id: `badge`,
    title: `Count badge`,
    kind: `Buttons & chips`,
    blurb: `EXP-962: the smallest chip there is — a 16px capsule carrying a NUMBER and nothing else, 10px semibold and tabular so a count can climb without the box twitching. \`muted\` is a count you parked (the rail's drafts), \`primary\` one that wants you (unread). Zero renders NOTHING, because a badge is a signal and an empty signal is noise, and past \`max\` it reads \`99+\`. PLACEMENT stays at the call site — a row's trailing edge, a nav glyph's corner — so the badge owns only its shape. A \`Pill size="sm"\` is 24 tall and carries a word; this carries a quantity.`,
    status: {
      web: ok(`Badge`, `packages/ui/src/badge.tsx`),
      desktop: ok(
        `surface::count_badge`,
        DESKTOP_SURFACE,
        `EXP-963: RailBadge::Count hangs it on the rail's Drafts entry`
      ),
      ios: leftover(
        `GlassSegmentedControl`,
        IOS_SEGMENTED,
        `the one count capsule is inlined in a segment; the tab bar's unread mark is a FloatingBarBadgeDot`
      ),
      android: leftover(
        `GlassSegmentedControl`,
        `${ANDROID_COMPONENTS}/GlassSegmentedControl.kt`,
        `the same inline capsule (BadgeFill) inside a segment, reachable by nothing else`
      ),
    },
    island: () => (
      <div className="flex items-center gap-5">
        <Badge count={3} />
        <Badge count={12} tone="primary" />
        <Badge count={412} />
        {/* The corner arm: the badge keeps its shape, the call site the spot. */}
        <span className="relative flex size-8 items-center justify-center rounded-md text-muted-foreground">
          <DraftsGlyph className="size-4" />
          <Badge count={2} tone="primary" className="absolute -right-0.5 -top-0.5" />
        </span>
      </div>
    ),
  },
  {
    id: `issue-chip`,
    title: `Issue chip`,
    kind: `Buttons & chips`,
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
    kind: `Buttons & chips`,
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
    kind: `Buttons & chips`,
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
    leftovers: [
      { file: `apps/web/src/components/inbox/inbox-view.tsx`, note: `a hand-rolled 28px muted disc` },
    ],
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
    kind: `Inputs & pickers`,
    blurb: `36 tall, padding 0/12, radius 12, card fill under a card stroke; focus swaps the stroke to active — no ring. Placeholder at 50%.`,
    status: {
      web: ok(`Input`, `packages/ui/src/input.tsx`),
      desktop: ok(
        `controls::glass_input`,
        DESKTOP_CONTROLS,
        `focus swaps the stroke to strokeActive, no ring (EXP-720); the corner is radius.lg (EXP-963)`
      ),
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
    kind: `Inputs & pickers`,
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
    id: `file-diff-card`,
    title: `File diff card`,
    kind: `Surfaces`,
    blurb: `EXP-916: the ONE per-file unit every diff surface is made of — the review page, a Changes face and the transcript's edited-files card all stack THIS. A sticky glassy header (\`status letter · dimmed dir / name · +a −d · chevron\`) over the unified four-column body; the collapse threshold, the "Show N more lines" step and its wording are \`contract.diffUi\`, so nobody re-derives them. Two extra header states carry a live edit run: \`pending\` (the call is still going — no counts, an inert chevron, no body) and \`failed\` (rose, a trailing "failed", no body). \`flush\` drops the outer box so a card can be a ROW of a parent that divides its own children.`,
    status: {
      web: ok(`FileDiffCard`, WEB_DIFF_CARD),
      desktop: ok(`diff::render_file_card`, DESKTOP_DIFF),
      ios: ok(`DiffFileCard`, IOS_DIFF_CARD),
      android: ok(`DiffFileCard`, ANDROID_DIFF_CARD),
    },
    island: () => <FileDiffCard file={FIXTURE_FILE} />,
  },
  {
    id: `edited-files-card`,
    title: `Edited files card`,
    kind: `Surfaces`,
    blurb: `EXP-916: what a transcript draws for a run of consecutive file edits — \`N files edited\` over a flush stack of the very same file card, hairline-divided. The grouping rule and the rows are the contract's (\`@exp/domain-contract/edit-card\`: one row per PATH, a file touched twice folds into one with summed counts, a call with no patch yet is a \`pending\`/\`failed\` stub). Everything starts collapsed except the LIVE row, which opens itself so the reader watches the edit land and folds back when the card settles; past \`cardPreviewFiles\` the rest sit behind "N more". A tap toggles a row in place — a card never navigates anywhere.`,
    status: {
      web: ok(`EditedFilesCard`, WEB_EDIT_CARD),
      desktop: ok(`session_extras::render_edit_card`, DESKTOP_SESSION_EXTRAS),
      ios: ok(`EditedFilesCard`, IOS_EDIT_CARD),
      android: ok(`EditedFilesCard`, ANDROID_EDIT_CARD),
    },
    island: () => (
      <EditedFilesCard
        view={EDIT_CARD_VIEW}
        openPaths={new Set([`apps/web/src/components/agent-session.tsx`])}
        onToggle={noop}
      />
    ),
  },
  {
    id: `file-diff-tree`,
    title: `File diff tree`,
    kind: `Lists & rows`,
    blurb: `EXP-916: the file column beside a diff — the review's summary line, a "Filter files" field, then the changed files as a TREE. \`diffFileTree\` is the shape: directories before files, each group by lower-cased name, a lone-child directory chain compacted into one node (\`packages/ui/src\`), a directory's counts its subtree's sums. Folders open by default and fold by path; a non-blank filter returns a FLAT list of matching files in input order, because a search result is not a pruned tree. Picking a row only reports the path — the caller scrolls, or closes its sheet.`,
    status: {
      web: ok(`FileDiffTree`, WEB_DIFF_TREE),
      desktop: ok(`diff_pane::file_tree`, DESKTOP_DIFF_PANE),
      ios: ok(`DiffFileTree`, IOS_DIFF_TREE),
      android: ok(`DiffFileTree`, ANDROID_DIFF_TREE),
    },
    island: () => <FileDiffTree files={TREE_FILES} onSelect={noop} />,
  },
  {
    id: `fab-chrome`,
    title: `Floating chrome`,
    kind: `Surfaces`,
    blurb: `The ONE floating-glass recipe a phone's bottom bar is made of, and only its PAINT: the card hairline, the popover fill at 85% and a backdrop blur. Radius, text colour and layout stay at the call site, because a capsule and a radius-16 tray genuinely differ from a circle. EXP-904: four files restated the string before it became one constant. EXP-962 took the 52px CIRCLE out of that list — it is \`FabButton\` now (its own entry) — so what still composes this by hand is the capsule stretched between two circles and the tray the steer composer expands into.`,
    status: {
      web: ok(`FAB_CHROME_CLASS`, `packages/ui/src/fab-chrome.tsx`),
      desktop: na(`no floating phone bar`),
      ios: ok(`FloatingBarCircle`, `apps/ios/ExpUI/Sources/FloatingBottomBar.swift`),
      android: {
        state: `leftover`,
        symbol: `Fab`,
        file: `${ANDROID_COMPONENTS}/BottomNavBar.kt`,
        note: `the tab bar FAB paints the opaque fill + strong hairline inline; no shared modifier`,
      },
    },
    island: () => (
      // The circles are `FabButton`s now; only the capsule still wears the
      // chrome by hand, which is the point of keeping it a constant.
      <div className="flex items-end gap-3">
        <FabButton emphasis="primary" aria-label="New issue">
          <PlusGlyph className="size-5" />
        </FabButton>
        <div
          className={`flex h-[52px] flex-1 items-center rounded-full px-4 text-sm text-muted-foreground ${FAB_CHROME_CLASS}`}
        >
          Message the agent…
        </div>
        <FabButton aria-label="More">
          <MoreGlyph className="size-5" />
        </FabButton>
      </div>
    ),
  },
  {
    id: `attachment-thumb`,
    title: `Attachment thumbnail`,
    kind: `Buttons & chips`,
    blurb: `A picked attachment waiting in a composer: a 64px center-cropped tile at radius MD under the card hairline, with a small circular remove badge hung off its top-right corner. A video uses the same tile with a first-frame poster on black; any other file stays a chip that carries the same badge. EXP-904: the comment, launch and steer composers each drew it by copy-paste. EXP-962 gave it two SIZES and two ARMS: \`inline\` is the image as POSTED — its own width, capped at 480 tall, contained on the section fill under the same hairline, reserving its probed aspect ratio — and the arms are \`onOpen\`, which makes the media a zoom button into the lightbox, and \`onRemove\`, the corner badge, which a posted image shows only on hover.`,
    status: {
      web: ok(
        `AttachmentThumb / AttachmentRemoveButton`,
        `packages/ui/src/attachment-thumb.tsx`
      ),
      desktop: ok(
        `comment_attachments::pending_attachments_strip`,
        `apps/desktop/crates/ui/src/comment_attachments.rs`
      ),
      ios: ok(
        `PendingAttachmentStrip`,
        `apps/ios/Exponential/UI/Components/AttachmentStrips.swift`
      ),
      android: ok(`PendingAttachmentStrip`, `${ANDROID_COMPONENTS}/AttachmentStrips.kt`),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex flex-wrap gap-2 p-2">
          <AttachmentThumb src={THUMB_FIXTURE_SRC} removeLabel="Remove image" onRemove={noop} />
          <AttachmentThumb src={THUMB_FIXTURE_SRC} removeLabel="Remove image" onRemove={noop} />
        </div>
        {/* The posted image: its own size, a zoom arm, and the delete badge
            its owner gets on hover. */}
        <div className="p-2">
          <AttachmentThumb
            size="inline"
            src={THUMB_FIXTURE_SRC}
            alt="A screenshot of the merge queue"
            width={96}
            height={96}
            openLabel="Open image"
            onOpen={noop}
            removeLabel="Delete image"
            onRemove={noop}
          />
        </div>
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
    blurb: `ONE composer for comments, steering and support replies: a radius-16 card of card fill under a card hairline, holding an optional attachment strip, a borderless 36-min field and a tool row of 24px ghost glyph buttons with a right-aligned submit whose glyph is the primary tint. EXP-877's \`inline\` arm is the steer card — the round submit rides the field's own row instead of a tool row under it. The opaque variant swaps to the opaque card fill and the strong stroke — it floats over a feed on mobile, and an alpha fill there shows the conversation through it. EXP-961 moved it into @exp/ui: the card owns CHROME AND LAYOUT only, and every caller keeps its own field, upload and send.`,
    status: {
      web: ok(`Composer / ComposerTool / ComposerSubmit`, `packages/ui/src/composer.tsx`),
      desktop: ok(`composer::glass_composer`, `apps/desktop/crates/ui/src/composer.rs`),
      ios: ok(`GlassComposer`, `apps/ios/ExpUI/Sources/GlassComposer.swift`),
      android: ok(`GlassComposer`, `${ANDROID_COMPONENTS}/GlassComposer.kt`),
    },
    island: () => (
      <div className="grid gap-3">
        {/* The comment card: the pending strip, the mention field undressed to
            the card's chrome, four tools and the round send. */}
        <Composer
          strip={
            <div className="flex flex-wrap items-center gap-2 px-2 pt-2">
              <AttachmentThumb
                src={THUMB_FIXTURE_SRC}
                removeLabel="Remove image"
                onRemove={noop}
              />
            </div>
          }
          tools={
            <>
              <ComposerTool aria-label="Add image" title="Add image">
                <EditorImageGlyph />
              </ComposerTool>
              <ComposerTool aria-label="Attach files" title="Attach files">
                <AttachGlyph />
              </ComposerTool>
              <ComposerTool aria-label="Insert issue reference" title="Insert issue reference">
                <IssueRefGlyph />
              </ComposerTool>
              <ComposerTool aria-label="Insert emoji" title="Insert emoji">
                <EmojiGlyph />
              </ComposerTool>
            </>
          }
          submit={<ComposerSubmit aria-label="Send comment" />}
        >
          <Textarea
            rows={2}
            placeholder="Leave a comment"
            className="min-h-16 border-none bg-transparent text-sm shadow-none focus-visible:border-transparent dark:bg-transparent"
          />
        </Composer>
        {/* EXP-877: the steer card — one row, the send glyph bottom-aligned
            beside the field so it stays put while the field grows. */}
        <Composer
          inline
          tools={
            <ComposerTool aria-label="Attach files" title="Attach files">
              <PlusGlyph />
            </ComposerTool>
          }
          submit={<ComposerSubmit />}
        >
          <Textarea
            rows={1}
            placeholder="Steer the run"
            className="max-h-32 min-h-9 w-full border-none bg-transparent px-3 py-2 shadow-none focus-visible:border-transparent"
          />
        </Composer>
        {/* The reply arm floats over a feed, so the fill is opaque. */}
        <Composer
          opaque
          tools={
            <ComposerTool aria-label="Attach files" title="Attach files">
              <AttachGlyph />
            </ComposerTool>
          }
          submit={<ComposerSubmit />}
        >
          <Textarea
            rows={1}
            placeholder="Reply"
            className="min-h-9 border-none bg-transparent text-sm shadow-none focus-visible:border-transparent dark:bg-transparent"
          />
        </Composer>
      </div>
    ),
  },
  {
    id: `markdown`,
    title: `Markdown blocks`,
    kind: `Surfaces`,
    blurb: `The chat-sized set the steer feed is built from. Narration is bare text at 90% behind a 12px glyph at 50% — no bubble, because a wall of them is unreadable. The person's turn IS a bubble: radius 12, active fill, strong hairline. Plan and question share ONE neutral radius-16 card; only the header line is tinted, primary for a plan and yellow for a question. Its options are full-width rows: the promoted one wears the primary fill, a pick the glass active fill, never blue; a free-text row opens the composer card inline under itself. A tool line is a 12px label with a truncated mono detail at 50%, and any long block clamps at 160 behind Show more. Inline code is tinted in chat feeds only — the issue and comment renderers keep the neutral chip.`,
    status: {
      web: ok(`QuestionCard / NarrationBubble`, `apps/web/src/components/agent-session.tsx`),
      desktop: ok(
        `SteerSessionView::render_item / render_ask`,
        `apps/desktop/crates/ui/src/steer_viewer.rs`
      ),
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
    id: `meter`,
    title: `Meter`,
    kind: `Feedback`,
    blurb: `The ONE bar every usage surface draws — the rate-limit windows, the run's context, the mini line. A capsule track in the strong stroke with a capsule fill, and exactly three tones: foreground at 30% normally, the yellow semantic from 75%, the destructive from 95%. The height is the caller's (6px full, 4px mini); the tone is the only decision. Before EXP-909 there were two bars two rows apart — a bare Progress with the primary fill, and a hand-rolled span with its own tone map — reading the same percent in different colours.`,
    status: {
      web: ok(`Meter`, `packages/ui/src/meter.tsx`),
      desktop: ok(`usage_bar::meter`, `apps/desktop/crates/ui/src/usage_bar.rs`),
      ios: ok(`AgentUsageTrack`, `apps/ios/ExpUI/Sources/UsageTrack.swift`),
      android: ok(
        `UsageTrack`,
        `${ANDROID_COMPONENTS}/UsageTrack.kt`
      ),
    },
    island: () => (
      <div className="grid w-[260px] gap-3">
        <Meter value={9} />
        <Meter value={67} />
        <Meter value={81} tone="warning" />
        <Meter value={100} tone="danger" />
        <Meter value={73} tone="normal" className="h-1" />
      </div>
    ),
  },
  {
    id: `usage-bar`,
    title: `Usage windows`,
    kind: `Feedback`,
    blurb: `Every rate-limit window the machine reported, TWO lines each: the window's name left and its countdown right, then the meter with the percent (tabular, "NN%", never "62% used"). Stale numbers dim the whole block to 50% and add one "as of …" line — they are never hidden, because aged numbers still beat none.`,
    status: {
      web: ok(`UsageWindows`, `apps/web/src/components/agent-usage-bar.tsx`),
      desktop: ok(`render_usage_windows`, `apps/desktop/crates/ui/src/usage_bar.rs`),
      ios: ok(`UsageWindows`, `apps/ios/Exponential/UI/Session/AgentUsageCards.swift`),
      android: ok(
        `UsageWindows`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentUsageBar.kt`
      ),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        `<div class="cmp-usage-bar">`,
        `<div class="line"><span class="label">Current session</span><span class="amount">resets in 1h 4m</span></div>`,
        `<div class="track"><div class="fill"></div></div>`,
        `</div>`,
        `<div class="cmp-usage-bar warn">`,
        `<div class="line"><span class="label">All models</span><span class="amount">resets in 1h 14m</span></div>`,
        `<div class="track"><div class="fill"></div></div>`,
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `usage-mini`,
    title: `Usage mini`,
    kind: `Feedback`,
    blurb: `The same report in one line: up to three windows (the five-hour one, the week, the first per-model one) as wire label · 4px meter · percent. It sits under an account row that is not the one the run spends — the other accounts in the usage overlay, every login under a device, and the account picker's hover preview — where the two-line form would not fit and the long titles would not either.`,
    status: {
      web: ok(`UsageMini`, `apps/web/src/components/agent-usage-mini.tsx`),
      desktop: ok(`render_usage_mini`, `apps/desktop/crates/ui/src/usage_bar.rs`),
      ios: ok(`AgentUsageMini`, `apps/ios/Exponential/UI/Session/AgentUsageCards.swift`),
      android: ok(
        `AgentUsageMini`,
        `apps/android/app/src/main/java/com/exponential/app/ui/session/AgentUsageBar.kt`
      ),
    },
    render: () =>
      [
        `<div class="cmp-usage-mini">`,
        `<div class="line"><span class="label">5h</span><span class="track"><span class="fill"></span></span><span class="amount">4%</span></div>`,
        `<div class="line"><span class="label">Week</span><span class="track"><span class="fill"></span></span><span class="amount">73%</span></div>`,
        `<div class="line"><span class="label">Fable</span><span class="track"><span class="fill"></span></span><span class="amount">100%</span></div>`,
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
    kind: `Colour`,
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
    kind: `Colour`,
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
    kind: `Shape & size`,
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
    kind: `Shape & size`,
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
    kind: `Motion`,
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
    kind: `Lists & rows`,
    blurb: `EXP-736: the issue's relations beneath the properties card (web + desktop) or inside the properties sheet (phones). Section header with the Add relation capsule, then one row per link: status glyph, the per-side label, identifier, title, trailing remove.`,
    status: {
      web: ok(`IssueRelationsCard`, `apps/web/src/components/issue-relations-card.tsx`),
      desktop: ok(`issue_relations::render_relations_section`, `apps/desktop/crates/ui/src/issue_relations.rs`),
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
    kind: `Lists & rows`,
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
    kind: `Lists & rows`,
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
  {
    id: `checkbox`,
    title: `Checkbox`,
    kind: `Inputs & pickers`,
    blurb: `A 16px rounded square that holds a TABLE's selection — the bulk-select column of an issue list and nothing else. It is deliberately NOT the multi-select affordance in a picker: every option row on all four clients marks itself with the leading circle pair (ui-selected / ui-unselected), so a checkbox inside a picker would make web the only client drawing selection twice. Checked takes the primary fill under the primary foreground; indeterminate is the same box with a minus.`,
    status: {
      web: ok(`Checkbox`, `packages/ui/src/checkbox.tsx`),
      desktop: ok(
        `controls::checkbox`,
        DESKTOP_CONTROLS,
        `glass box (row fill, strong stroke, radius SM), primary when checked, ui-check / ui-minus; bulk-select + checklist rows`
      ),
      ios: na(`no checkbox exists: a multi-select row draws the ui-selected / ui-unselected circle pair`),
      android: na(`same as iOS — the sheet's option rows carry the circle glyph pair, never a box`),
    },
    island: () => (
      <div className="flex items-center gap-4">
        <Checkbox checked aria-label="Selected" />
        <Checkbox checked="indeterminate" aria-label="Partially selected" />
        <Checkbox aria-label="Not selected" />
        <Checkbox disabled aria-label="Disabled" />
      </div>
    ),
  },
  {
    id: `switch`,
    title: `Switch`,
    kind: `Inputs & pickers`,
    blurb: `The 36×20 capsule that flips a setting the moment it is pressed — there is no Save beside one. Off is the active fill under the foreground knob, on the primary fill under the primary-foreground knob, and the travel is one fast duration. It almost always rides a toggle ROW, which owns the label and the description; this is the bare control.`,
    status: {
      web: ok(`Switch`, `packages/ui/src/switch.tsx`),
      desktop: ok(
        `controls::web_switch`,
        DESKTOP_CONTROLS,
        `a lint (only_controls_constructs_switches) refuses Switch::new anywhere else`
      ),
      ios: ok(
        `GlassToggleStyle`,
        IOS_CONTROLS,
        `applied app-wide as .toggleStyle(.glass), so call sites keep the stock Toggle`
      ),
      android: ok(
        `glassSwitchColors()`,
        ANDROID_SHEET_ROWS,
        `the tokens for the stock M3 Switch; there is no GlassSwitch composable`
      ),
    },
    island: () => (
      <div className="flex items-center gap-4">
        <Switch defaultChecked aria-label="On" />
        <Switch aria-label="Off" />
        <Switch disabled aria-label="Disabled" />
      </div>
    ),
  },
  {
    id: `select`,
    title: `Select`,
    kind: `Inputs & pickers`,
    blurb: `The closed single-select: a field-height trigger of card fill under a card hairline, the value left and a chevron right, focus swapping the stroke to active. Only the PLACEHOLDER arm can be photographed — SelectValue resolves against items that live inside the portalled list, so a valued trigger renders empty until the browser opens it (see @exp/ui's island limits). On the natives there is no free-standing select at all: the closed trigger is always a picker ROW opening a sheet.`,
    status: {
      web: ok(`Select / SelectTrigger`, `packages/ui/src/select.tsx`),
      desktop: ok(`surface::glass_picker_select`, DESKTOP_SURFACE),
      ios: na(`no free-standing select: the closed single-select is a GlassPickerRow opening a sheet (see picker row)`),
      android: na(`same as iOS — PickerRow (SheetOptionRows.kt) is the closed arm, and the sheet is the list`),
    },
    island: () => (
      <div className="grid gap-3">
        <Select>
          <SelectTrigger className="w-[220px]">
            <SelectValue placeholder="Pick a status" />
          </SelectTrigger>
        </Select>
        <Select>
          <SelectTrigger size="sm" className="w-[220px]">
            <SelectValue placeholder="Pick a board" />
          </SelectTrigger>
        </Select>
      </div>
    ),
  },
  {
    id: `color-picker`,
    title: `Colour picker`,
    kind: `Inputs & pickers`,
    blurb: `EXP-862 made this the icon picker's TWIN, so the board form's two triggers read as one control repeated: the same rounded square at the radius ladder's MD step, a card hairline once something is picked and a DASHED one while it is empty, opening the same 8-column grid. The swatches are the counter-example to the shape rule — a colour has no shape to read, so a cell stays a circle, and the picked one wears a ring instead of a fill.`,
    status: {
      web: ok(
        `ColorPicker`,
        `packages/ui/src/color-picker.tsx`,
        `the grid is color-swatch-grid.tsx, rendered inside the trigger's popover`
      ),
      desktop: ok(
        `board_form::color_picker`,
        `apps/desktop/crates/ui/src/board_form.rs`,
        `color_swatch_grid is the grid in the same file`
      ),
      ios: ok(
        `ColorSwatchPicker`,
        `apps/ios/ExpUI/Sources/ColorSwatchGrid.swift`,
        `the grid is ColorSwatchGrid in the same file`
      ),
      android: ok(`ColorPicker`, `${ANDROID_COMPONENTS}/ColorPicker.kt`),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex items-center gap-2">
          <ColorPicker value="" onChange={noop} />
          <ColorPicker value="#3b82f6" onChange={noop} />
          <IconPicker value="flag" onChange={noop} />
        </div>
        {/* The bare grid the trigger opens — a closed popover renders nothing. */}
        <div className="w-[266px]">
          <ColorSwatchGrid value="#3b82f6" onChange={noop} />
        </div>
      </div>
    ),
  },
  {
    id: `label`,
    title: `Field label`,
    kind: `Inputs & pickers`,
    blurb: `The 14px medium line that names a field, tied to it by htmlFor so the label is part of the hit box, and dimmed with the field when it is disabled. It exists on the WEB only: every native lays a field out as a ROW that already carries its name on the left, so a label above the control there would say the same thing twice.`,
    status: {
      web: ok(`Label`, `packages/ui/src/label.tsx`),
      desktop: na(`a field's name is the glass row's own leading text — see input row`),
      ios: na(`same — the name is the row title inside GlassTextField / GlassPickerRow`),
      android: na(`same — TextFieldRow / PickerRow carry the label themselves`),
    },
    island: () => (
      <div className="grid gap-2">
        <Label htmlFor="demo-label-title">Title</Label>
        <Input id="demo-label-title" defaultValue="Fix the merge queue" />
      </div>
    ),
  },
  {
    id: `team-avatar`,
    title: `Team avatar`,
    kind: `Buttons & chips`,
    blurb: `A team is a single accent SQUARE — the primary fill under its foreground, corners at a quarter of the side, the first letter at 44% — so it never reads as a person: user avatars are hashed-hue circles and there is no hue here to hash, because a team has one identity, not one of eight. 28 in the sidebar header, 20 in its menu, 18 in the switcher sheet.`,
    status: {
      web: ok(`TeamAvatar`, `packages/ui/src/team-avatar.tsx`),
      desktop: ok(`user_avatar::team_avatar`, `apps/desktop/crates/ui/src/user_avatar.rs`),
      ios: ok(`TeamAvatar`, `apps/ios/ExpUI/Sources/TeamAvatar.swift`),
      android: ok(`TeamAvatar`, `${ANDROID_COMPONENTS}/Avatars.kt`),
    },
    island: () => (
      <div className="flex items-center gap-3">
        <TeamAvatar name="Exponential" size={28} />
        <TeamAvatar name="Acme" size={20} />
        <TeamAvatar name="Mobile" size={18} />
      </div>
    ),
  },
  {
    id: `live-dot`,
    title: `Live dot & status glyph`,
    kind: `Buttons & chips`,
    blurb: `The two smallest marks the product has. The DOT carries a session's state in one 8px disc — the six tones are the ×4 table (live / attention / done / unread / idle / muted) — and only a LIVE one pulses, so a halo anywhere means something is happening right now. The status GLYPH is the issue's status as a pie clock: the category decides the shape, the row decides the colour (a token class for a builtin, the synced hex for a custom), and the identical glyph appears in the list, the chip and the picker.`,
    status: {
      web: ok(
        `LiveDot / StatusGlyph`,
        `packages/ui/src/live-dot.tsx`,
        `the glyph is status-glyph.tsx; LIVE_DOT_TONE is locked against the app's SESSION_DOT_CLASS`
      ),
      desktop: ok(
        `surface::live_dot`,
        DESKTOP_SURFACE,
        `ping = an animated halo, twice the disc, 60% to 0 over 1s on the decelerate curve; the run list pings on agent_busy only`
      ),
      ios: ok(
        `SessionStateDot`,
        `apps/ios/Exponential/UI/Session/SessionStateDot.swift`,
        `the halo is PulsingLiveDot; the status glyph is iconName/color in ExpUI/IssueColorExtensions.swift`
      ),
      android: ok(
        `LiveDot`,
        `apps/android/app/src/main/java/com/exponential/app/ui/issue/AgentPrCard.kt`,
        `PulsingDot / StaticDot sit in the same file; the status glyph is StatusIcon (ui/components/IssueVisuals.kt)`
      ),
    },
    island: () => (
      <div className="grid gap-3">
        <div className="flex items-center gap-3">
          <LiveDot tone="live" ping label="Running" />
          <LiveDot tone="attention" label="Needs input" />
          <LiveDot tone="done" label="In review" />
          <LiveDot tone="unread" label="Unread" />
          <LiveDot tone="idle" label="Idle" />
          <LiveDot tone="muted" label="Ended" />
        </div>
        <div className="flex items-center gap-3">
          <StatusGlyph {...BACKLOG_GLYPH} className="size-4" />
          <StatusGlyph icon="circle-dot" colorClass="text-yellow-500" className="size-4" />
          <StatusGlyph {...DONE_GLYPH} className="size-4" />
          <StatusGlyph icon="circle" colorHex="#a855f7" className="size-4" />
        </div>
      </div>
    ),
  },
  {
    id: `empty-state`,
    title: `Empty state`,
    kind: `Feedback`,
    blurb: `What a PAGE says when it has nothing: the 48px icon disc, one semibold title, one muted sentence that TEACHES the next step rather than restating the emptiness, and an optional actions slot under it — all on a centred column of at most 28rem. Never a bare "No results". Its in-list sibling is \`ListEmpty\` (same file, its own entry below): one muted line inside a list that filtered down to nothing, where a teaching block would be wrong. The third of them is \`EmptyCta\` (EXP-962, next entry): the dashed box that STARTS the list, where the empty state itself is the button.`,
    status: {
      web: ok(`EmptyState`, `packages/ui/src/empty-state.tsx`),
      desktop: ok(`controls::empty_state`, DESKTOP_CONTROLS),
      ios: leftover(
        `InboxView.emptyState`,
        `apps/ios/Exponential/UI/Inbox/InboxView.swift`,
        `every screen rolls its own private empty state (inbox, reviews, actions, my issues); there is no shared symbol`
      ),
      android: leftover(
        `EmptyState`,
        `${ANDROID_COMPONENTS}/Scaffolding.kt`,
        `a bare 28dp tinted glyph instead of the 48 disc, and its parameters are message/detail rather than title/description`
      ),
    },
    island: () => (
      <EmptyState
        icon={conceptIcon(`nav-inbox`)}
        title="Inbox zero"
        description="Notifications land here when someone mentions you or an agent finishes a run."
      >
        <Pill size="sm" mode="action">
          Notification settings
        </Pill>
      </EmptyState>
    ),
  },
  {
    id: `empty-cta`,
    title: `Empty call to action`,
    kind: `Feedback`,
    blurb: `EXP-962: the third empty, and the only one that is a BUTTON. A dashed, full-width box standing exactly where the first row will go — a 16px glyph, one title line, one muted sentence under it, the row wash and full-strength text on hover. Dashed because it is a placeholder for the row it invites; clickable because the shortest path to that row is the box itself. \`EmptyState\` teaches a PAGE with nothing on it, \`ListEmpty\` reports a list that filtered down to nothing, and this one STARTS a list: the actions panel's "describe one" nudge is the call site it was cut from.`,
    status: {
      web: ok(
        `EmptyCta`,
        `packages/ui/src/empty-state.tsx`,
        `EmptyState and ListEmpty are the other two, in the same file`
      ),
      desktop: ok(
        `ActionsView::render_nudge`,
        `apps/desktop/crates/ui/src/actions_view.rs`,
        `the same dashed strip under the actions list, opening the creator run`
      ),
      ios: na(
        `the creator run needs a device: ActionsListView.emptyState is a read-only page empty instead`
      ),
      android: na(
        `same: ActionsScreen's ActionsEmptyState reads, it does not invite — creation lives on web or desktop`
      ),
    },
    island: () => (
      <div className="w-80">
        <EmptyCta
          icon={ActionCreateGlyph}
          title="No custom actions yet"
          description="Describe one and your agent will build it."
          onClick={noop}
        />
      </div>
    ),
  },
  {
    id: `skeleton`,
    title: `Skeleton`,
    kind: `Feedback`,
    blurb: `A pulsing block standing in for text that is still loading, at the SHAPE of what will arrive — a row's worth of bars, never a spinner in a list. It is a web and desktop affordance only: both natives answer a pending screen with a centred spinner, because a phone list is short enough that a skeleton flashes before it reads.`,
    status: {
      web: ok(`Skeleton`, `packages/ui/src/skeleton.tsx`),
      desktop: ok(
        `controls::skeleton`,
        DESKTOP_CONTROLS,
        `radius MD on the theme skeleton fill, breathing 100% to 50% over the web 2s pulse on the standard curve`
      ),
      ios: na(`no skeleton or shimmer anywhere: a loading screen is a centred spinner`),
      android: na(`same — LoadingState (Scaffolding.kt) centres a spinner instead`),
    },
    island: () => (
      <div className="grid gap-2">
        <Skeleton className="h-4 w-48" />
        <Skeleton className="h-4 w-32" />
      </div>
    ),
  },
  {
    id: `dialog`,
    title: `Dialog`,
    kind: `Surfaces`,
    blurb: `The centred modal: a radius-16 card on the OPAQUE card fill under a card hairline, a semibold title, one line of body, and a footer whose LAST capsule is the primary. Cancel is borderless — two boxed buttons side by side ask the reader to choose between two equals. On a phone the same component drops to the bottom sheet arm, so a confirm never opens in the middle of a thumb's reach. Hand-written here because a closed Radix portal renders nothing at all statically (PORTAL_ONLY_IDS).`,
    status: {
      web: ok(
        `Dialog / DialogContent`,
        `packages/ui/src/dialog.tsx`,
        `alert-dialog.tsx is the trapping arm: it has no dismiss and its action is destructive`
      ),
      desktop: ok(
        `native_dialog::DialogShell`,
        `apps/desktop/crates/ui/src/native_dialog.rs`,
        `EXP-284: every IDE dialog is a real OS window, not an in-window overlay`
      ),
      ios: na(`no shared shell: a confirm is SwiftUI's stock .alert, a content dialog is the sheet (GlassSheetChrome)`),
      android: na(`same — M3 AlertDialog is called directly, and a content dialog is GlassSheet`),
    },
    render: () =>
      [
        `<div class="cmp-dialog">`,
        `<div class="title">Delete board</div>`,
        `<div class="text">“Mobile app” and its 42 issues move to trash for 48 hours.</div>`,
        `<div class="footer">`,
        `<button class="cmp-pill borderless" type="button" data-size="md" data-mode="action"><span class="label">Cancel</span></button>`,
        pill(`Delete board`, { size: `md`, primary: true }),
        `</div>`,
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-palette`,
    title: `Palette`,
    kind: `Colour`,
    blurb: `Every colour that is not a glass fill, in three groups. The neutral SURFACE palette is the shadcn theme the whole product sits on. The SEMANTIC accents are fixed sRGB, authored once and shared, because a status hue must not drift with the surface — plus the code trio, which tints inline code in the agent CHAT feeds and nowhere else. The DIFF pair is web's own look lifted verbatim so all four clients render one unified diff: a foreground per side over a 10% wash of the same hue.`,
    status: {
      web: ok(`--color-* / --diff-*`, `packages/ui/src/styles.css`),
      desktop: ok(
        `theme::{PALETTE consts} / theme::diff`,
        `apps/desktop/crates/theme/src/tokens.generated.rs`,
        `the semantic accents are top-level consts (YELLOW, GREEN, …), diff its own module`
      ),
      ios: ok(
        `DesignTokens.Palette / .Semantic / .Diff`,
        `apps/ios/ExpUI/Sources/DesignTokens.generated.swift`
      ),
      android: ok(
        `DesignTokens.Palette / .Semantic / .Diff`,
        `apps/android/app/src/main/java/com/exponential/app/ui/theme/DesignTokens.generated.kt`
      ),
    },
    render: () =>
      [
        `<div class="cmp-stack">`,
        sectionHeader(`Surface`),
        swatchGroup(`pal`, designTokens.palette),
        sectionHeader(`Semantic`),
        swatchGroup(`sem`, designTokens.semantic),
        sectionHeader(`Diff`),
        swatchGroup(`diff`, designTokens.diff),
        `</div>`,
      ].join(``),
  },
  {
    id: `tokens-type`,
    title: `Type`,
    kind: `Type`,
    blurb: `One family, Inter, and the sizes the agent TRANSCRIPT is set in (EXP-787) — the only type scale the four clients share, because a run has to read the same on a phone and on a 32-inch monitor. Body 14/22 is prose and a sent message; tool 12/18 is a tool row and every other caption in the feed. 16 is the root the rem ladder is measured from. The specimens below are set in this page's own stack: Inter is not loaded here, and faking it with a fallback would make the page lie about the one thing it documents.`,
    status: {
      web: ok(
        `--font-sans / --transcript-*`,
        `packages/ui/src/styles.css`,
        `the gap ladder derives in lib/agent-feed.ts; design-tokens.test.ts guards the parity`
      ),
      desktop: ok(`theme::transcript`, `apps/desktop/crates/theme/src/tokens.generated.rs`),
      ios: ok(
        `DesignTokens.Transcript`,
        `apps/ios/ExpUI/Sources/DesignTokens.generated.swift`
      ),
      android: ok(
        `DesignTokens.Transcript`,
        `apps/android/app/src/main/java/com/exponential/app/ui/theme/DesignTokens.generated.kt`
      ),
    },
    render: () => {
      const line = (label: string, value: string, size: string, text: string): string =>
        [
          `<div class="line">`,
          `<span class="label">${escapeHtml(label)}</span>`,
          `<span class="text ${size}">${escapeHtml(text)}</span>`,
          `<span class="value">${escapeHtml(value)}</span>`,
          `</div>`,
        ].join(``)
      return [
        `<div class="cmp-type">`,
        line(
          `fontFamily`,
          designTokens.type.fontFamily,
          `size-type-base`,
          `The quick brown fox jumps over the lazy dog`
        ),
        line(
          `baseSize`,
          `${designTokens.type.baseSize}px`,
          `size-type-base`,
          `The rem ladder is measured from here`
        ),
        line(
          `transcript.body`,
          `${designTokens.transcript.bodySize}/${designTokens.transcript.bodyLineHeight}`,
          `size-type-body`,
          `Rebased onto origin/master and force-pushed; the merge queue is green again.`
        ),
        line(
          `transcript.tool`,
          `${designTokens.transcript.toolSize}/${designTokens.transcript.toolLineHeight}`,
          `size-type-tool`,
          `Read packages/ui/src/file-diff-tree.tsx`
        ),
        `</div>`,
      ].join(``)
    },
  },
  {
    id: `tokens-icons`,
    title: `Icon registry`,
    kind: `Icons`,
    blurb: `Every CONCEPT the product can name, rendered from the registry itself. One icon set, Lucide, byte-identical on four clients, generated from packages/icons/icons.json into a committed output per platform — so an icon changes in one JSON and nowhere else. A multi-client surface names the concept (conceptIcon("nav-search")), never a raw glyph import, which is what lets the same screen carry the same mark on a phone, a browser and the IDE. This entry is an ISLAND of the real registry: a lookalike table would be the one place the icons could drift.`,
    status: {
      web: ok(
        `conceptIcon / ICON_COMPONENTS`,
        `packages/ui/src/icons.generated.ts`,
        `the concept → glyph map is SEMANTIC_ICONS in packages/icons/src/generated.ts`
      ),
      desktop: ok(
        `icons::registry`,
        `apps/desktop/crates/ui/src/icons.rs`,
        `pub mod registry include!s icons.generated.rs; concepts are consts (registry::NAV_SEARCH)`
      ),
      ios: ok(
        `AppIcons`,
        `apps/ios/ExpUI/Sources/AppIcons.generated.swift`,
        `asset names for the bundled imagesets, rendered through AppIcon — never Image(systemName:)`
      ),
      android: ok(
        `ExpIcons`,
        `apps/android/app/src/main/java/com/exponential/app/ui/icons/ExpIcons.generated.kt`,
        `the geometry itself: one lazy ImageVector per glyph, with the concepts as getters`
      ),
    },
    island: () => (
      <div className="grid grid-cols-3 gap-x-5 gap-y-1.5">
        {ICON_CONCEPTS.map(([concept, glyph]) => {
          const Glyph = conceptIcon(concept)
          return (
            <div key={concept} className="flex min-w-0 items-center gap-2 text-xs">
              <Glyph className="size-4 shrink-0 text-foreground/70" />
              <span className="truncate text-foreground/85">{concept}</span>
              <span className="ml-auto truncate font-mono text-muted-foreground">{glyph}</span>
            </div>
          )
        })}
      </div>
    ),
  },
  {
    id: `combobox`,
    title: `Combobox`,
    kind: `Inputs & pickers`,
    blurb: `The ONE searchable picker. Its shell — MobilePopover over Command — was copy-pasted about twelve times, and every copy re-decided four things a reader can see: what a picked row LOOKS like, what value= carries, how wide the popover is, and what "nothing picked" is called. Selection is now fixed and matches both natives: SINGLE select marks the picked row with a trailing ui-check, MULTI marks EVERY row with the leading ui-selected / ui-unselected circle pair (iOS AgentIssuePickerSheet.swift, Android AgentIssuePickerSheet.kt) — never a checkbox, which would make web the odd client out. value is the IDENTITY and keywords the search text, so two boards may share a name; noneLabel renders a row that reports null, which retires six sentinel strings. Single closes on pick, a multi stays open because a batch is several picks. EXP-957 added the third arm, ComboboxMenuItems: the same rows as items INSIDE a Radix context or dropdown menu, for the issue row's right-click submenus and the bulk bar, which retires the menu's own radio dot and checkbox tick; and a bulk edit over rows that disagree draws ui-indeterminate (circle-minus) on a multi row, or marks nothing at all on a single. EXP-958 folded the last two closed single-selects onto it — the status and priority menu, whose desktop arm marked no row at all, and the settings picker row, which was a Select on desktop and a hand-rolled sheet on the phone — as searchable={false} pickers with two more triggers: row (the glass form ladder's picker row, label leading, value trailing) and inline (one word of the muted sentence under the composer, which collapses to plain text with a single option). The demo shows the four triggers beside the bare ComboboxList, since a closed portal renders nothing — and a menu arm cannot render outside its menu at all.`,
    status: {
      web: ok(
        `Combobox / ComboboxList / ComboboxMenuItems`,
        `packages/ui/src/combobox.tsx`,
        `PickerOption is the row shape; ComboboxList the body without the popover; ComboboxMenuItems the rows inside a Radix menu`
      ),
      desktop: ok(
        `pickers::searchable_picker`,
        `apps/desktop/crates/ui/src/pickers.rs`,
        `EXP-963: PickerOption + PickerSelection Single/Multi; the label and board popovers wrap it`
      ),
      ios: leftover(
        `GlassPickerSheet`,
        `apps/ios/ExpUI/Sources/GlassSheet.swift`,
        `a sheet per subject over GlassSheetRow; the selection glyphs agree, the generic picker does not exist`
      ),
      android: leftover(
        `GlassSheetRow`,
        `${ANDROID_COMPONENTS}/GlassSheet.kt`,
        `only the ROW is shared — every picker sheet re-assembles sheet + search field + rows by hand`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex flex-wrap items-center gap-2">
          <div className="w-[13rem]">
            <Combobox
              mobileTitle="Assignee"
              triggerVariant="field"
              options={ASSIGNEE_OPTIONS}
              value="jonas"
              onChange={noop}
              noneLabel="Unassign"
            />
          </div>
          <Combobox
            mobileTitle="Labels"
            triggerLabel="Labels"
            multiple
            options={LABEL_OPTIONS}
            value={[]}
            onChange={noop}
          />
        </div>
        {/* EXP-958: the row trigger inside a glass group, and the inline
            word inside the composer's muted sentence — one with a choice,
            one collapsed to plain text because there is nothing to choose. */}
        <div className="w-[20rem]">
          <GlassGroup>
            <Combobox
              triggerVariant="row"
              searchable={false}
              mobileTitle="Runs on"
              options={[
                { value: `macbook`, label: `MacBook Pro` },
                { value: `homeserver`, label: `homeserver` },
              ]}
              value="macbook"
              onChange={noop}
            />
          </GlassGroup>
        </div>
        <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <Combobox
            triggerVariant="inline"
            searchable={false}
            mobileTitle="Device"
            width="sm"
            options={[
              { value: `macbook`, label: `MacBook Pro` },
              { value: `homeserver`, label: `homeserver` },
            ]}
            value="macbook"
            onChange={noop}
          />
          <Combobox
            triggerVariant="inline"
            searchable={false}
            mobileTitle="Model"
            options={[{ value: `default`, label: `CLI default` }]}
            value="default"
            onChange={noop}
          />
        </p>
        {/* The bare bodies. `cmdk` only hides its empty row once its client
            effects have registered the items, so the static specimen hides it
            the way the running list does. */}
        <div className="flex flex-wrap items-start gap-3 [&_[cmdk-empty]]:hidden">
          <div className="w-[14rem] overflow-hidden rounded-lg border border-glass-stroke-card bg-glass-card">
            <ComboboxList
              options={ASSIGNEE_OPTIONS}
              value="jonas"
              onChange={noop}
              noneLabel="Unassign"
              placeholder="Search people"
            />
          </div>
          {/* A bulk edit: "design" sits on SOME of the edited issues. */}
          <div className="w-[14rem] overflow-hidden rounded-lg border border-glass-stroke-card bg-glass-card">
            <ComboboxList
              multiple
              options={BULK_LABEL_OPTIONS}
              value={[`bug`, `mobile`]}
              onChange={noop}
              max={3}
              placeholder="Search labels"
            />
          </div>
        </div>
      </div>
    ),
  },
  {
    id: `search-field`,
    title: `Search field`,
    kind: `Inputs & pickers`,
    blurb: `The ONE "filter this list" field: the text field with the search glyph INSIDE it and a ghost clear that appears only once there is something to clear — and puts the caret back in the field, so typing continues. Eight of them existed at five heights, most a bare Input re-dressed by hand and none with either affordance, while both natives had drawn exactly this for years. Two rungs: md is the stock 36 field, sm the 28 one dense columns use — the Reviews file filter, a sidebar filter. It is an Input, not a new box: every chrome decision still comes from there.`,
    status: {
      web: ok(`SearchField`, `packages/ui/src/search-field.tsx`),
      desktop: ok(
        `controls::search_field`,
        DESKTOP_CONTROLS,
        `EXP-963: SearchFieldSize::Md 36 / Sm 28; the diff pane filter, every picker query and the search dialog draw it`
      ),
      ios: ok(`GlassSheetSearchField`, IOS_CONTROLS),
      android: ok(`GlassSheetSearchField`, `${ANDROID_COMPONENTS}/GlassSheet.kt`),
    },
    island: () => (
      <div className="grid gap-3">
        <SearchField value="" onValueChange={noop} placeholder="Search issues" />
        <SearchField value="merge queue" onValueChange={noop} placeholder="Search issues" />
        <SearchField
          size="sm"
          value="file-diff"
          onValueChange={noop}
          placeholder={contract.diffUi.filterPlaceholder}
          aria-label={contract.diffUi.filterPlaceholder}
        />
      </div>
    ),
  },
  {
    id: `date-picker`,
    title: `Date picker`,
    kind: `Inputs & pickers`,
    blurb: `The ONE date picker. Popover + Calendar was inlined three times — the properties panel, the editor chips, the mobile tray — and each copy converted between a Date and the wire's YYYY-MM-DD its own way, two of them through new Date(value), which the spec parses as UTC and which therefore shows the PREVIOUS day west of Greenwich. This one speaks the wire format on both sides and converts in exactly one place. A due date is a DATE, never an instant (REV2-49), so there is no time arm; the trigger is a Pill showing the short form, and Clear is a row under the grid rather than a second control beside it.`,
    status: {
      web: ok(
        `DatePicker`,
        `packages/ui/src/date-picker.tsx`,
        `parseDateValue / formatDateLabel are the only place the wire date becomes a Date`
      ),
      desktop: ok(`pickers::due_date_popover`, `apps/desktop/crates/ui/src/pickers.rs`),
      ios: ok(
        `DueDateSheet`,
        `apps/ios/Exponential/UI/Issue/Sheets/DueDateSheet.swift`,
        `CreateIssueView keeps a second unfoldable form, UI/Issue/DueDatePicker.swift`
      ),
      android: ok(
        `DueDateSheet`,
        `apps/android/app/src/main/java/com/exponential/app/ui/issue/DueDateSheet.kt`,
        `the grid itself is IssueDatePickerDialog.kt`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex flex-wrap items-center gap-2">
          <DatePicker value="2026-03-08" onChange={noop} />
          <DatePicker value={null} onChange={noop} />
        </div>
        {/* The grid the trigger opens — a closed portal renders nothing. */}
        <div className="w-fit overflow-hidden rounded-lg border border-glass-stroke-card bg-glass-card">
          <Calendar
            mode="single"
            selected={DUE_DATE_FIXTURE}
            defaultMonth={DUE_DATE_FIXTURE}
          />
        </div>
      </div>
    ),
  },
  {
    id: `typeahead`,
    title: `Typeahead menu`,
    kind: `Inputs & pickers`,
    blurb: `The menu that follows what someone is TYPING — @ mentions, # issue refs, : emoji, / commands — as opposed to the combobox, which owns its own field. Three copies existed, each re-implementing the same active index, the same wrap and the same above/below flip, and only one of them told its host whether it had handled the key: the other two signalled it by NOT calling the host's handler, which is how a menu ends up swallowing a send shortcut. One hook owns the keys now: arrows move and wrap, a plain Enter or Tab accepts, Enter with Cmd or Ctrl is the composer's send and passes straight through untouched, Escape dismisses, and with no items nothing is handled at all. The menu has two arms: absolute under a textarea, or anchored to a caret rect in the editor, where it portals to the body at fixed coordinates and flips above the caret when the room below runs out.`,
    status: {
      web: ok(
        `useTypeahead / TypeaheadMenu / TypeaheadRow`,
        `packages/ui/src/typeahead.tsx`,
        `handleKeyDown returns true when the menu ate the key; the editor's caret menu is the anchored arm (EXP-959)`
      ),
      desktop: ok(
        `controls::typeahead_menu / typeahead_row`,
        DESKTOP_CONTROLS,
        `the row stays completion_row_content; the anchored arm flips above the caret when room runs out, the slash menu inline`
      ),
      ios: ok(
        `EditorAutocompleteMenu`,
        `apps/ios/Exponential/UI/Markdown/EditorAutocompleteMenu.swift`,
        `the slash menu is separate: UI/Session/SlashCommandMenu.swift`
      ),
      android: ok(
        `AutocompleteMenu`,
        `apps/android/app/src/main/java/com/exponential/app/ui/markdown/AutocompleteMenu.kt`,
        `the rows are AutocompleteRows in the same file`
      ),
    },
    island: () => (
      // The menu is ABSOLUTE and hangs under its host, so the specimen gives
      // it a field to hang from and reserves the room underneath. The
      // editor's anchored arm (`anchor`, EXP-959) is a portal to
      // document.body, which a static island cannot draw.
      <div className="relative h-56">
        <div className="relative">
          <Textarea rows={2} defaultValue="Ping @mi about " />
          <TypeaheadMenu placement="below">
            <TypeaheadRow active>
              <UserAvatar user={{ id: `user-mk`, name: `Mina Kay` }} size={20} />
              <span className="min-w-0 flex-1 truncate">Mina Kay</span>
              <span className="shrink-0 text-xs text-muted-foreground">mina@example.com</span>
            </TypeaheadRow>
            <TypeaheadRow>
              <IssueChip identifier="EXP-941" title="Styleguide foundation" status={BACKLOG_GLYPH} />
            </TypeaheadRow>
            <TypeaheadRow>
              <span className="shrink-0 text-base">🎉</span>
              <span className="min-w-0 flex-1 truncate">tada</span>
            </TypeaheadRow>
          </TypeaheadMenu>
        </div>
      </div>
    ),
  },
  {
    id: `alert`,
    title: `Alert`,
    kind: `Feedback`,
    blurb: `The inline banner: a message that belongs to the page it interrupts, not a toast that flies past and not a dialog that blocks. Two variants only — the neutral card fill for a notice, and the destructive tint for a failure — and the leading glyph earns its own column only when one is passed. The admin console carried two byte-identical copies of the destructive recipe before this existed.`,
    status: {
      web: ok(`Alert / AlertTitle / AlertDescription`, `packages/ui/src/alert.tsx`),
      desktop: ok(
        `controls::alert / alert_title`,
        DESKTOP_CONTROLS,
        `default and destructive on the glass tokens, a glyph column only when one is passed; the repository dialog banner is one`
      ),
      ios: na(`no boxed banner: an error renders as a red Text line on DesignTokens.Semantic.red`),
      android: leftover(
        `GlassNotice`,
        `${ANDROID_COMPONENTS}/GlassNotice.kt`,
        `the boxed inline message, but with no title slot and no destructive variant — callers pass the red themselves`
      ),
    },
    island: () => (
      <div className="grid gap-3">
        <Alert>
          <WarningGlyph aria-hidden />
          <AlertTitle>This device is offline</AlertTitle>
          <AlertDescription>
            Sessions started here will queue until it reconnects.
          </AlertDescription>
        </Alert>
        <Alert variant="destructive">
          <AlertDescription>
            Could not load teams. Check the connection and try again.
          </AlertDescription>
        </Alert>
      </div>
    ),
  },
  {
    id: `password-input`,
    title: `Password input`,
    kind: `Inputs & pickers`,
    blurb: `The auth pages' masked field: the stock Input with a ghost eye hung inside its right gutter, which flips the type between \`password\` and \`text\`. It is the one field a typo cannot be proof-read in, so the reveal is not optional chrome. The eye pair stays a RAW lucide import on purpose — no native client draws it and the shared registry has no eye-off concept to name. EXP-961 moved it into @exp/ui with the rest of the auth surface.`,
    status: {
      web: ok(`PasswordInput`, `packages/ui/src/password-input.tsx`),
      desktop: ok(
        `login::render_email_step`,
        `apps/desktop/crates/ui/src/login.rs`,
        `the only other client with the reveal: it wires gpui-component's mask_toggle onto the shared glass_input`
      ),
      ios: leftover(
        `GlassTextField(isSecure: true)`,
        `apps/ios/ExpUI/Sources/GlassControls.swift`,
        `a bare SecureField arm — no reveal, so a mistyped password can only be fixed by clearing the field`
      ),
      android: leftover(
        `GlassTextField(visualTransformation = …)`,
        `apps/android/app/src/main/java/com/exponential/app/ui/components/GlassTextField.kt`,
        `LoginScreen passes PasswordVisualTransformation() inline and draws no trailing eye`
      ),
    },
    island: () => (
      <div className="grid max-w-sm gap-3">
        <Label htmlFor="demo-password">Password</Label>
        <PasswordInput id="demo-password" defaultValue="hunter2hunter2" />
      </div>
    ),
  },
  {
    id: `auth-shell`,
    title: `Auth form shell`,
    kind: `Surfaces`,
    blurb: `The frame every signed-out page wears: the logo and wordmark centred above a card, a centred title and one line of description inside it, the flow's own fields, then a footer line under them and the Privacy · Terms pair below the card. Login, signup, the OTP step, the invite page, the device-code page and the MCP consent screen all open in it, which is why the column is capped at 24rem — a sign-in form that spans a desktop viewport reads as a settings page.`,
    status: {
      web: ok(`AuthFormShell`, `packages/ui/src/auth-form-shell.tsx`),
      desktop: ok(
        `LoginView::render`,
        `apps/desktop/crates/ui/src/login.rs`,
        `the brand block + glass_card at l.1151 mirror this shell; it is inlined in the screen, not a symbol of its own`
      ),
      ios: leftover(
        `LoginView`,
        `apps/ios/Exponential/UI/Auth/LoginView.swift`,
        `a plain ScrollView/VStack: no shared shell, so each auth flow re-states its own header and footer`
      ),
      android: leftover(
        `LoginScreen`,
        `apps/android/app/src/main/java/com/exponential/app/ui/auth/LoginScreen.kt`,
        `same — the card, the brand row and the legal line are built inline in the screen`
      ),
    },
    island: () => (
      <AuthFormShell
        title="Welcome back"
        description="Sign in to continue to Exponential"
        footer={
          <p className="pt-4 text-center text-sm text-muted-foreground">
            No account yet? <span className="text-foreground underline">Create one</span>
          </p>
        }
      >
        <div className="grid gap-3">
          <Label htmlFor="demo-auth-email">Email</Label>
          <Input id="demo-auth-email" defaultValue="mina@example.com" />
        </div>
      </AuthFormShell>
    ),
  },
  {
    id: `logo`,
    title: `Logo mark`,
    kind: `Icons`,
    blurb: `The product's own mark — a disc with three swept curves cut OUT of it, never a glyph from the registry (the icon set is Lucide, and this is not something we may restyle). Two variants: \`dark\` paints the fixed brand ink for a light ground, \`light\` takes \`currentColor\` so it inherits whatever it sits in. Every client keeps its own copy of the geometry, so the entry names all four.`,
    status: {
      web: ok(`ExponentialLogo`, `packages/ui/src/exponential-logo.tsx`),
      desktop: ok(
        `icons::ExpIcon`,
        `apps/desktop/crates/ui/src/icons.rs`,
        `the Logo variant is generated by icon_named! off assets/icons/logo.svg; logo-white.svg is the inverted arm`
      ),
      ios: ok(
        `ExpLogoMark`,
        `apps/ios/Exponential/UI/Components/ExpLogoMark.swift`,
        `drawn with a SwiftUI Canvas rather than an asset, so it scales with Dynamic Type`
      ),
      android: leftover(
        `ExponentialMark`,
        `apps/android/app/src/main/java/com/exponential/app/ui/components/BrandMark.kt`,
        `paints the splash drawable and scales the box around it: one size, no light/dark variant`
      ),
    },
    island: () => (
      <div className="flex items-center gap-6">
        <span className="flex size-12 items-center justify-center rounded-lg bg-white">
          <ExponentialLogo variant="dark" size={32} />
        </span>
        <ExponentialLogo variant="light" size={32} />
        <span className="flex items-center gap-2">
          <ExponentialLogo variant="light" size={20} />
          <span className="text-xl font-semibold">Exponential</span>
        </span>
      </div>
    ),
  },
  {
    id: `brand-marks`,
    title: `Brand marks`,
    kind: `Icons`,
    blurb: `The agents' own marks, deliberately OUTSIDE the Lucide registry: a brand mark is not a glyph we may recolour or swap, so it never resolves through \`conceptIcon\`. Claude keeps its real path in its own orange (\`CLAUDE_FILL\`) on all four clients; Codex is the CLI's own mark; the OpenAI and Cursor marks are simplified silhouettes for the MCP setup tabs, which only need recognisability beside a name. \`AgentBrandMark\` is the resolver every run surface uses — and an agent this build ships no mark for falls back to the neutral \`settings-agents\` concept rather than wearing claude's, the same rule on all four.`,
    status: {
      web: ok(
        `ClaudeIcon / CodexIcon / AgentBrandMark`,
        `packages/ui/src/brand-icons.tsx`,
        `the resolver is agent-brand-mark.tsx; the picker's AgentMark shares these paths`
      ),
      desktop: ok(
        `coding_selects::agent_mark`,
        `apps/desktop/crates/ui/src/coding_selects.rs`,
        `ExpIcon::Claude / ExpIcon::Codex off the bundled brand SVGs (assets/icons/claude.svg, codex.svg)`
      ),
      ios: ok(
        `AgentBrandMark.image(_:)`,
        `apps/ios/Exponential/UI/Session/AgentBrandMark.swift`,
        `resolves Assets.xcassets/agent-<id>; an id outside contract.codingAgent gets the neutral glyph`
      ),
      android: ok(
        `agentIconPainter / agentIconTint`,
        `${ANDROID_COMPONENTS}/SheetOptionRows.kt`,
        `drawable/ic_agent_claude.xml keeps its own orange (tint Unspecified); every other mark takes content colour`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex items-center gap-4">
          <ClaudeIcon className="size-6" />
          <CodexIcon className="size-6" />
          <OpenAiIcon className="size-6" />
          <CursorIcon className="size-6" />
        </div>
        <div className="flex items-center gap-4 text-muted-foreground">
          <AgentBrandMark agent="claude" className="size-4" />
          <AgentBrandMark agent="codex" className="size-4" />
          <AgentBrandMark agent="some-acp-binary" className="size-4" />
        </div>
      </div>
    ),
  },
  {
    id: `board-glyph`,
    title: `Board glyph`,
    kind: `Buttons & chips`,
    blurb: `EXP-449: a board is ALWAYS its curated icon tinted with its own colour — the anonymous colour dot is gone from every picker, breadcrumb, caption and switcher. The icon name is a Lucide name straight out of the shared registry (EXP-273), so all four clients resolve the same art; a board with none falls back on its shape, the code glyph for a repo-backed board and the kanban grid for the rest.`,
    status: {
      web: ok(`BoardGlyph`, `packages/ui/src/board-glyph.tsx`, `getBoardIcon in board-icons.ts is the name resolver`),
      desktop: ok(
        `icons::board_glyph`,
        `apps/desktop/crates/ui/src/icons.rs`,
        `board_icon is the glyph; the tint is applied at the call site (sidebar::rail_board_icon)`
      ),
      ios: leftover(
        `BoardTypeDisplay.iconName(for:)`,
        `apps/ios/ExpUI/Sources/BoardIconDisplay.swift`,
        `name resolution only: every call site applies its own .foregroundStyle(Color(hex:)), so the tint is copy-pasted`
      ),
      android: ok(`BoardIcon`, `${ANDROID_COMPONENTS}/BoardIconUi.kt`),
    },
    island: () => (
      <div className="flex items-center gap-4">
        <BoardGlyph board={{ icon: `rocket`, color: `#f97316` }} className="size-5" />
        <BoardGlyph board={{ icon: `bug`, color: `#ef4444` }} className="size-5" />
        <BoardGlyph board={{ icon: `megaphone`, color: `#3b82f6` }} className="size-5" />
        {/* No stored icon: a repo-backed board reads as the code glyph. */}
        <BoardGlyph board={{ repositoryId: `repo-1`, color: `#a855f7` }} className="size-5" />
      </div>
    ),
  },
  {
    id: `issue-group-band`,
    title: `Issue group band`,
    kind: `Lists & rows`,
    blurb: `EXP-862: ONE group header for every issue list — the board's big list and the sidebar's narrow ones. A fold chevron, the status glyph, the name, the group's FULL size (not the windowed row count), and the group's own trailing action outside the fold button, because a button inside a button is invalid markup. Two densities: \`list\` is the board page's sticky edge-to-edge band, whose backdrop blur is load-bearing (rows scroll under a translucent tint), \`compact\` the 17rem sidebar's rounded strip. The status is passed IN as a resolved glyph and the tint as a resolved wash, so the component never learns the team's status rows.`,
    status: {
      web: ok(
        `IssueGroupBand`,
        `packages/ui/src/issue-group-band.tsx`,
        `components/issue-group-header.tsx is the binding that resolves the glyph and the wash`
      ),
      desktop: ok(
        `IssueListView::render_group_header`,
        `apps/desktop/crates/ui/src/issue_list.rs`,
        `render_board_nav / render_my_issues_nav take the same band at the narrow column's density`
      ),
      ios: leftover(
        `IssueListView.statusHeader`,
        `apps/ios/Exponential/UI/Issue/IssueListView.swift`,
        `private per screen; MyIssuesView.statusHeader is a second copy and does not fold at all`
      ),
      android: leftover(
        `GroupHeader`,
        `apps/android/app/src/main/java/com/exponential/app/ui/myissues/MyIssuesScreen.kt`,
        `private; the board list draws its own StatusHeader in ui/issue/IssueListScreen.kt`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div>
          <IssueGroupBand
            glyph={BACKLOG_GLYPH}
            name="Backlog"
            count={24}
            open
            onToggle={noop}
            wash={{ className: `bg-muted-foreground/10` }}
            trailing={
              <Button variant="ghost" size="icon-sm" aria-label="New issue">
                <PlusGlyph />
              </Button>
            }
          />
          <ListRow interactive>
            <span className="min-w-0 flex-1 truncate">APP-14 · Fix the merge queue</span>
          </ListRow>
          <ListRow interactive>
            <span className="min-w-0 flex-1 truncate">APP-15 · Ship the usage sheet</span>
          </ListRow>
        </div>
        {/* The sidebar's rung, folded, over a CUSTOM row's own hex wash. */}
        <IssueGroupBand
          density="compact"
          glyph={{ icon: `circle`, colorHex: `#a855f7` }}
          name="Waiting on design"
          count={3}
          open={false}
          onToggle={noop}
          wash={{ style: { backgroundColor: `#a855f71a` } }}
        />
      </div>
    ),
  },
  {
    id: `work-bar`,
    title: `Work bar`,
    kind: `Surfaces`,
    blurb: `EXP-893: the phone's ONE floating bottom bar, \`[circle] [capsule] [circle]\` in the floating-glass recipe, shared by every face of the Work screen — the issue (Properties · Comment · Start), the run (usage ring · composer · switcher) and the changes (files · Merge · switcher). EXP-916 locked the geometry to Android's: a 20px screen inset, 10px between the slots, 52px circles with 20px glyphs, a capsule padded 18px. Expanding the composer replaces the left circle and the capsule while the trailing circle stays MOUNTED, because the switcher owns lookups that must not re-run on every expand. The real bar is \`fixed … md:hidden\`, so the specimen is its SLOTS in a row.`,
    status: {
      web: ok(
        `MobileWorkBar / MobileWorkCapsule`,
        `packages/ui/src/mobile-work-bar.tsx`,
        `its circles are FabButtons now; the clearance constant is the faces' scroll padding`
      ),
      desktop: na(`no floating phone bar: the IDE's bottom edge is the terminal session bar`),
      ios: ok(
        `FloatingBottomBar / FloatingBarCircle / FloatingBarCapsule`,
        `apps/ios/ExpUI/Sources/FloatingBottomBar.swift`,
        `FloatingBarCluster is the Reviews layout, where the slots hug their content`
      ),
      android: ok(
        `FloatingBottomBar / BarCircle / BarCapsule`,
        `${ANDROID_COMPONENTS}/FloatingBottomBar.kt`,
        `the reference geometry the other three phones are locked to (EXP-916)`
      ),
    },
    island: () => (
      <div className="flex items-end gap-2.5">
        <FabButton aria-label="Properties">
          <PropertiesGlyph className="size-5" />
        </FabButton>
        <MobileWorkCapsule>
          <CommentGlyph className="size-5 shrink-0" />
          <span className="min-w-0 truncate">Comment</span>
        </MobileWorkCapsule>
        <FabButton aria-label="Switch face">
          <WorkFacesGlyph className="size-5" />
        </FabButton>
      </div>
    ),
    leftovers: [
      {
        file: `apps/web/src/components/issue-changes-face.tsx`,
        note: `the Merge capsule wears MOBILE_WORK_CAPSULE_CLASS on a SessionMergePill instead of MobileWorkCapsule`,
      },
    ],
  },
  {
    id: `work-header`,
    title: `Work header`,
    kind: `Surfaces`,
    blurb: `EXP-877: the ONE header the issue route and the session route share, so the title never moves when the face flips between an issue and its run. Row 1 is the title — an editable field for an issue, static text at exactly the field's padding and weight for a run — with the right cluster top-aligned on the SAME line; row 2 is the properties tray, absent on issue-less runs. It is sticky at the top of the view's own scroller and rides the 896px reading column every other face uses (body, transcript, diff). The band repaints the panel's own ground under the window scrim, or it reads as a black bar over the card.`,
    status: {
      web: ok(
        `WorkHeader`,
        `packages/ui/src/work-header.tsx`,
        `WORK_COLUMN_CLASS, RUN_TITLE_CLASS and DETAIL_STICKY_BAND_CLASS ship with it`
      ),
      desktop: ok(
        `work_header::render_work_header`,
        `apps/desktop/crates/ui/src/work_header.rs`,
        `WORK_COLUMN_W is the same 896; header_action_size picks the pill rung by PLACEMENT`
      ),
      ios: leftover(
        `WorkTitle`,
        `apps/ios/Exponential/UI/Work/WorkTitle.swift`,
        `the header is the system nav bar's .toolbar in WorkScreen.swift — no sticky band and no tray row`
      ),
      android: leftover(
        `WorkTopBar`,
        `apps/android/app/src/main/java/com/exponential/app/ui/work/WorkTopBar.kt`,
        `an M3 TopAppBar: the trailing verbs match, but there is no properties tray under it`
      ),
    },
    island: () => (
      <WorkHeader
        title={<h1 className={RUN_TITLE_CLASS}>Fix the merge queue</h1>}
        trailing={
          <Button variant="ghost" size="icon-sm" aria-label="More">
            <MoreGlyph />
          </Button>
        }
        tray={
          <div className={`flex flex-wrap items-center gap-2 px-5 pt-1 ${WORK_COLUMN_CLASS}`}>
            <Pill mode="select" leading={<StatusGlyph {...BACKLOG_GLYPH} className="size-3.5" />}>
              Backlog
            </Pill>
            <Pill mode="select" leading={<MergeGlyph className="size-3.5" />}>
              exp/APP-14
            </Pill>
          </div>
        }
      />
    ),
  },
  {
    id: `changes-file-sheet`,
    title: `Changed files sheet`,
    kind: `Buttons & chips`,
    blurb: `EXP-895: the phone's file list. A 64-wide column beside a diff leaves neither readable, so below md the Changes face's aside is gone and the tree lives in a bottom sheet hung off the work bar's LEADING slot. The trigger is the bar's own 52px circle carrying the files glyph over the COUNT, which is the whole affordance — a reader has to know how many files a PR touches before deciding to open it. A pick closes the sheet and reports the path; the card list scrolls to it. The sheet is a closed Radix portal at rest, so the specimen is the trigger.`,
    status: {
      web: ok(
        `ChangesFileSheet`,
        `packages/ui/src/changes-file-sheet.tsx`,
        `the sheet holds the same FileDiffTree the md+ column does; the title is contract.diffUi.changedFilesTitle`
      ),
      desktop: na(
        `no sheet: the IDE has room for the column, so the file tree is the ReviewFilesNav panel (review_files_nav.rs)`
      ),
      ios: ok(
        `DiffFileListSheet`,
        `apps/ios/Exponential/UI/Issue/DiffFileListSheet.swift`,
        `DiffFilesBarCircle in the same file is the bar trigger`
      ),
      android: ok(
        `DiffFileListSheet`,
        `apps/android/app/src/main/java/com/exponential/app/ui/issue/DiffFileListSheet.kt`,
        `FileListCircle is the reference the web trigger copies (18px glyph over the count)`
      ),
    },
    island: () => <ChangesFileSheet files={TREE_FILES} onSelect={noop} />,
  },
  {
    id: `pr-github-button`,
    title: `Open on GitHub`,
    kind: `Buttons & chips`,
    blurb: `EXP-916: THE GitHub control of every diff surface — the PR page in a new tab — as one component in three shapes, so the words (the contract's) and the behaviour are written once. \`ghost\` is the work header's action slot beside Merge; \`circle\` is a phone work-bar slot, for a run with no issue header to hang it on, and takes its chrome whole from the bar's circle recipe; \`glass\` is the Reviews header's action row beside Close PR and Merge PR. Each shape keeps the \`data-testid\` its surface had before they were merged.`,
    status: {
      web: ok(`PrGithubButton`, `packages/ui/src/pr-github-button.tsx`),
      desktop: ok(`work_header::github_button`, `apps/desktop/crates/ui/src/work_header.rs`),
      ios: leftover(
        `PrChangesFace.githubToolbarButton`,
        `apps/ios/Exponential/UI/Issue/PrChangesFace.swift`,
        `private; WorkScreen.swift holds a second inline copy under the same accessibility id`
      ),
      android: ok(
        `GithubHeaderAction`,
        `apps/android/app/src/main/java/com/exponential/app/ui/work/WorkTopBar.kt`,
        `the header arm only — the bar circle has no Android twin`
      ),
    },
    island: () => (
      <div className="flex items-end gap-3">
        <PrGithubButton prUrl="https://github.com/niach/exponential/pull/961" />
        <PrGithubButton
          prUrl="https://github.com/niach/exponential/pull/961"
          variant="circle"
        />
        <PrGithubButton
          prUrl="https://github.com/niach/exponential/pull/961"
          variant="glass"
        />
      </div>
    ),
  },
  {
    id: `context-ring`,
    title: `Context ring`,
    kind: `Feedback`,
    blurb: `EXP-877: how full the agent's context window is, as a 16px radial where the context pill used to be — and the trigger of the usage overlay the session already had. The arc is a stroked circle rotated a quarter turn, the track the same circle at 20% opacity, and the tone is the session's, not the ring's: the app maps its own thresholds onto normal / warning / danger and passes one in. A run nothing has measured renders NOTHING unless \`showEmpty\` says the run has other usage worth opening.`,
    status: {
      web: ok(
        `ContextRing`,
        `packages/ui/src/context-ring.tsx`,
        `ringGeometry + RING_TONE_CLASS ship with it; the app's lib/context-ring.ts derives percent and tone`
      ),
      desktop: ok(
        `usage_sheet::context_ring`,
        `apps/desktop/crates/ui/src/usage_sheet.rs`,
        `mounted by steer_viewer::render_context_ring; the percentage comes from usage_bar::context_percent`
      ),
      ios: ok(`ContextRing`, `apps/ios/ExpUI/Sources/ContextRing.swift`),
      android: ok(`ContextRing`, `${ANDROID_COMPONENTS}/ContextRing.kt`),
    },
    island: () => (
      <div className="flex items-center gap-3">
        <ContextRing percent={null} showEmpty />
        <ContextRing percent={30} />
        <ContextRing percent={80} tone="warning" />
        <ContextRing percent={97} tone="danger" />
      </div>
    ),
  },
  {
    id: `agent-picker`,
    title: `Agent picker`,
    kind: `Inputs & pickers`,
    blurb: `EXP-862: ONE agent picker per platform, so the trigger can never drift into a per-surface copy again — the composer's options row, the device settings' "Default agent" row and the launch pane all render this. The trigger is ICON-ONLY: the brand mark plus a chevron, with the name in the menu rows and in the tooltip, which doubles as its accessible name. \`AgentPickerTabs\` is the same picker as a segmented STRIP, for the launch pane where the agents sit side by side rather than behind a chevron. The menu is a portal, so the specimen is the closed trigger at both rungs.`,
    status: {
      web: ok(
        `AgentPicker / AgentPickerTabs`,
        `packages/ui/src/agent-picker.tsx`,
        `AgentMenuItems is exported on its own for a row's "…" menu; agentLabel names an id this build does not ship`
      ),
      desktop: ok(
        `coding_selects::agent_picker`,
        `apps/desktop/crates/ui/src/coding_selects.rs`,
        `agent_menu_items is the shared row set, exactly like the web arm`
      ),
      ios: ok(
        `AgentPickerMenu`,
        `apps/ios/ExpUI/Sources/GlassMenu.swift`,
        `AgentOptionsRow (UI/Agent/AgentOptionsRow.swift) is the composer's host`
      ),
      android: ok(
        `AgentPickerPill`,
        `apps/android/app/src/main/java/com/exponential/app/ui/agent/AgentOptionsRow.kt`,
        `AgentMenuItems in the same file is the shared row set`
      ),
    },
    island: () => (
      <div className="grid gap-4">
        <div className="flex items-center gap-4">
          <AgentPicker value="claude" onChange={noop} agents={[`claude`, `codex`]} />
          <AgentPicker
            size="sm"
            value="codex"
            onChange={noop}
            agents={[`claude`, `codex`]}
          />
        </div>
        <AgentPickerTabs value="claude" onChange={noop} agents={[`claude`, `codex`]} />
      </div>
    ),
  },
  {
    id: `lightbox`,
    title: `Media lightbox`,
    kind: `Surfaces`,
    blurb: `EXP-316/EXP-824: the shared viewer a picture, a clip or an audio attachment opens in — the description editor's image node, the storage table's filename, a results tile. The dialog is borderless and HUGS its media instead of spanning the viewport, and it is the one dialog that stays a full-screen page below sm rather than dropping to a sheet: a lightbox wants the whole screen. The video arm mounts only while open, so autoplay fires on every open and the stream stops the moment it unmounts. The dialog is a portal, so the island is its BODY (\`PreviewMedia\`) — the same switch, without the frame.`,
    status: {
      web: ok(
        `ImagePreviewDialog / PreviewMedia`,
        `packages/ui/src/image-preview-dialog.tsx`,
        `PreviewMedia is the non-portal body, split out so a static host can render the specimen`
      ),
      desktop: ok(
        `image_preview::open_image_preview`,
        `apps/desktop/crates/ui/src/image_preview.rs`,
        `EXP-284: a real OS window sized to the probed aspect; open_media_preview is the clip arm`
      ),
      ios: leftover(
        `.quickLookPreview($previewURL)`,
        `apps/ios/Exponential/UI/Components/AttachmentStrips.swift`,
        `the system QuickLook sheet, not our chrome: no shared component and no in-app audio caption`
      ),
      android: leftover(
        `ResultPreviewDialog`,
        `apps/android/app/src/main/java/com/exponential/app/ui/work/ResultsFace.kt`,
        `private and results-only; an issue attachment hands off to another app (ui/issue/AttachmentOpen.kt)`
      ),
    },
    island: () => (
      <div className="w-fit rounded-lg border border-glass-stroke-card bg-glass-card-opaque p-2">
        <PreviewMedia src={THUMB_FIXTURE_SRC} alt="Merge queue" label="merge-queue.png" />
      </div>
    ),
  },
  {
    id: `session-results`,
    title: `Session results`,
    kind: `Lists & rows`,
    blurb: `EXP-879: the screenshots a run published with \`exponential_sessions_results\`, read off the synced jsonb. One group band per topic over a wrapping strip of tiles, so an iOS, an Android and a web shot of ONE screen read as one row — which only works because every tile is the same height and takes its width from the probed aspect (a 4:3 desktop frame stands in when the upload could not be measured). On a narrow column the whole page scales down by ONE factor, the widest tile's overflow, rather than letting a row clip or each row pick its own size. Tapping a tile opens the shared lightbox. The tile URL is passed in: this package owns the tiles, the app owns the route.`,
    status: {
      web: ok(
        `SessionResultsView`,
        `packages/ui/src/session-results-view.tsx`,
        `the pure rules (grouping, tile width, the fitting factor) are session-results.ts, mirrored byte for byte ×4`
      ),
      desktop: ok(`session_results::render`, `apps/desktop/crates/ui/src/session_results.rs`),
      ios: ok(`SessionResultsFace`, `apps/ios/Exponential/UI/Work/SessionResultsFace.swift`),
      android: ok(
        `ResultsFace`,
        `apps/android/app/src/main/java/com/exponential/app/ui/work/ResultsFace.kt`,
        `ResultTile + ResultPreviewDialog sit in the same file`
      ),
    },
    island: () => (
      <SessionResultsView
        results={SESSION_RESULTS_FIXTURE}
        attachmentSrc={() => THUMB_FIXTURE_SRC}
      />
    ),
  },
  {
    id: `emoji-picker`,
    title: `Emoji picker`,
    kind: `Inputs & pickers`,
    blurb: `EXP-551: the picker the description toolbar and the comment composer share — a search field, a Recent row, then the dataset's groups. A pick hands the caller the UNICODE to insert, never a \`:shortcode:\`, and always the BASE record (EXP-600 dropped the skin-tone row on every client). The 36px cell IS the ghost icon button stretched across its grid column: only the colour-emoji face and the glyph size are the picker's own. The DATA is passed in — the app lazy-loads the generated dataset and keeps the per-device recents — and the ranked search is the shared rule the three natives mirror by hand. Category headers are the one surface that stays uppercase.`,
    status: {
      web: ok(
        `EmojiPicker / EmojiPickerPopover`,
        `packages/ui/src/emoji-picker.tsx`,
        HEADER_EXCEPTION
      ),
      desktop: ok(
        `emoji_picker::EmojiPicker`,
        `apps/desktop/crates/ui/src/emoji_picker.rs`,
        `emoji_picker_popover is the shared trigger; the grid is 8 cells wide, the popover's width driver`
      ),
      ios: ok(
        `EmojiPickerSheet`,
        `apps/ios/Exponential/UI/Markdown/EmojiPickerSheet.swift`,
        `the index and recents live in ExpUI/Sources/EmojiCatalog.swift`
      ),
      android: ok(
        `EmojiPickerSheet`,
        `apps/android/app/src/main/java/com/exponential/app/ui/emoji/EmojiPickerSheet.kt`,
        `rememberEmojiData / rememberEmojiPrefs are its dataset and recents halves`
      ),
    },
    island: () => (
      <div className="w-[21rem] overflow-hidden rounded-lg border border-glass-stroke-card bg-popover">
        <EmojiPicker
          data={indexEmojiData(EMOJI_FIXTURE)}
          recent={[`\u{1F389}`, `\u{1F436}`]}
          onPick={noop}
          autoFocusSearch={false}
        />
      </div>
    ),
  },
  {
    id: `list-empty`,
    title: `In-list empty line`,
    kind: `Feedback`,
    blurb: `The compact sibling of the empty state: one muted centred line INSIDE a list that filtered down to nothing, exactly the line \`CommandEmpty\` draws, for the lists that have no Command around them. It is deliberately not the teaching block — a search that matched nothing needs a different QUERY, not a next step, and a 48px icon disc under a search field reads as a page having gone wrong.`,
    status: {
      web: ok(`ListEmpty`, `packages/ui/src/empty-state.tsx`, `EmptyState in the same file is the page-sized one`),
      desktop: ok(
        `pickers::empty_picker_row`,
        `apps/desktop/crates/ui/src/pickers.rs`,
        `the CommandEmpty row every picker shares; controls::empty_state is the page-sized counterpart`
      ),
      ios: leftover(
        `Text("No emoji found")`,
        `apps/ios/Exponential/UI/Markdown/EmojiPickerSheet.swift`,
        `every list inlines its own Text: there is no shared line, and DeviceLogins.emptyLine is a private third copy`
      ),
      android: leftover(
        `ChangesEmptyRow`,
        `apps/android/app/src/main/java/com/exponential/app/ui/work/ChangesFace.kt`,
        `private to one face; the emoji sheet writes its own line, and EmptyState (Scaffolding.kt) is page-sized`
      ),
    },
    island: () => (
      <div className="w-72 overflow-hidden rounded-lg border border-glass-stroke-card bg-glass-card">
        <ListEmpty>No emoji found</ListEmpty>
      </div>
    ),
  },
]
