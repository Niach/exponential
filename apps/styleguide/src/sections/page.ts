/**
 * EXP-1019 — THE page model: the four sections of `sections.json`, each banded
 * by `kind`, each band holding entries.
 *
 * Two things feed it and they are shaped differently on purpose:
 *
 *   - `COMPONENTS` (`components.tsx`) — the ~95 entries the page already had.
 *     They carry a `kind` and nothing else, so their SECTION is DERIVED here
 *     (`sectionOfSpec`): a Style kind is Style, an id in `SPECIAL_ENTRY_IDS`
 *     is a composition, everything else is a general control.
 *   - `ENTRIES` (`entries/index.ts`, the EXP-1029 contract) — the registered
 *     entries. Their section is the contract's (`entry.section`, mirroring
 *     `sections.json`) and is NEVER re-derived; only their nav BAND is ours
 *     (`ENTRY_BAND`), because the contract has no `kind` vocabulary.
 *
 * Both collapse into one `PageEntry`, which is all `render.ts` draws: a
 * reader never learns which of the two lists an entry came from, and neither
 * does the renderer.
 */

import type { ReactElement } from "react"

import { STYLE_KINDS } from "../components.tsx"
import type {
  ComponentKind,
  ComponentPlatform,
  ComponentSpec,
  ComponentStatus,
} from "../components.tsx"
import { ENTRIES } from "../entries/index.ts"
import type { StyleguideEntry } from "../entries/types.ts"
import { SECTIONS } from "./index.ts"
import type { SectionId, StyleguideSection } from "./index.ts"

/**
 * The COMPOSITIONS (section 3). The rule, and the only one:
 *
 *   a general component is a control ANY screen reuses; a special component
 *   is built ON TOP of those and has ONE owner — one screen, one feature or
 *   one surface draws it, and nothing else ever will.
 *
 * So the app shell, the two auth/settings page frames, the work chrome, the
 * agent-run surfaces (the composer, the diff/changes family, the session
 * results, the usage readouts, the context ring), the GitHub settings pair
 * and the comment card are here; a pill, a row, a text field, a picker, a
 * skeleton and an alert are not. The list is EXPLICIT rather than a flag on
 * each spec because it is the reviewable statement of what counts as a
 * composition — one diff shows the whole judgement.
 */
export const SPECIAL_ENTRY_IDS: readonly string[] = [
  // The app frames.
  `app-shell`,
  `page-header`,
  `auth-shell`,
  // The work screen's own chrome.
  `work-bar`,
  `work-header`,
  `bulk-bar`,
  `session-bar`,
  // The agent run: what it types into, what it says, what it produced.
  `composer`,
  `markdown`,
  `session-results`,
  `file-diff-card`,
  `file-diff-tree`,
  `edited-files-card`,
  `changes-file-sheet`,
  `pr-github-button`,
  `context-ring`,
  `usage-bar`,
  `usage-mini`,
  // One-owner cards.
  `comment-card`,
  `relations-card`,
  `entity-preview-card`,
  `github-connection`,
  `repo-picker`,
  `lightbox`,
]

/** The bands a section of CONTROLS draws, in nav order. */
const CONTROL_BANDS: readonly ComponentKind[] = [
  `Inputs & pickers`,
  `Buttons & chips`,
  `Lists & rows`,
  `Surfaces`,
  `Feedback`,
]

/**
 * Nav band order per section. Style bands by the value it documents; the other
 * three band by the control vocabulary. The union of both lists IS
 * `ComponentKind`, so no entry can land in no band.
 */
export const BAND_ORDER: Record<SectionId, readonly ComponentKind[]> = {
  style: STYLE_KINDS,
  general: CONTROL_BANDS,
  special: CONTROL_BANDS,
  views: CONTROL_BANDS,
}

/**
 * The band a REGISTERED entry (EXP-1029) draws in. The contract owns the
 * section, this owns the band — `sections.json` has no `kind` vocabulary, and
 * teaching it one would make two indices where there is one. Gated: every
 * registered id appears here.
 */
export const ENTRY_BAND: Readonly<Record<string, ComponentKind>> = {
  picker: `Inputs & pickers`,
  [`picker-board`]: `Inputs & pickers`,
  [`picker-issue`]: `Inputs & pickers`,
  [`picker-action`]: `Inputs & pickers`,
  [`picker-account`]: `Inputs & pickers`,
  [`picker-device`]: `Inputs & pickers`,
  [`picker-assignee`]: `Inputs & pickers`,
  [`picker-icon`]: `Inputs & pickers`,
  [`picker-status`]: `Inputs & pickers`,
  [`picker-priority`]: `Inputs & pickers`,
  [`picker-label`]: `Inputs & pickers`,
  [`sub-shell`]: `Surfaces`,
  menu: `Surfaces`,
  [`issue-context-menu`]: `Surfaces`,
  [`composer-dialog`]: `Surfaces`,
  [`device-settings`]: `Surfaces`,
  toast: `Feedback`,
  [`session-tree`]: `Lists & rows`,
  [`pr-graph-badge`]: `Buttons & chips`,
  [`workflow-graph`]: `Surfaces`,
}

/** The band an unregistered entry falls back to, so the page always draws it. */
const ENTRY_BAND_FALLBACK: ComponentKind = `Surfaces`

/**
 * One thing the page draws — a `ComponentSpec` or a `StyleguideEntry`, flattened.
 * `status` is absent on a placeholder (no table, one line of body) and so is
 * `owner` on a spec that predates the contract.
 */
export interface PageEntry {
  id: string
  title: string
  blurb: string
  section: SectionId
  /** The nav band inside the section. */
  kind: ComponentKind
  status?: Record<ComponentPlatform, ComponentStatus>
  leftovers?: { file: string; note: string }[]
  /** The issue that fills a registered entry; absent on an existing spec. */
  owner?: string
  /** Renders one line naming its owner, and no status table. */
  placeholder?: true
  render?: () => string
  island?: () => ReactElement
}

export function isPageIsland(
  entry: PageEntry
): entry is PageEntry & { island: () => ReactElement } {
  return entry.island !== undefined
}

/** One nav band inside a section. */
export interface PageBand {
  kind: ComponentKind
  entries: PageEntry[]
}

/** One of the four sections, with everything it draws. */
export interface PageSection {
  section: StyleguideSection
  bands: PageBand[]
  /** Every entry of the section, in band order — the nav and routing order. */
  entries: PageEntry[]
}

/**
 * Where an EXISTING spec belongs. Derived, never stored: a spec carries a
 * `kind`, and a mis-sorted entry should be impossible rather than merely
 * gated.
 */
export function sectionOfSpec(spec: { id: string; kind: ComponentKind }): SectionId {
  if (STYLE_KINDS.includes(spec.kind)) return `style`
  return SPECIAL_ENTRY_IDS.includes(spec.id) ? `special` : `general`
}

/** The band a registered entry draws in. */
export function bandOfEntry(entry: Pick<StyleguideEntry, `id`>): ComponentKind {
  return ENTRY_BAND[entry.id] ?? ENTRY_BAND_FALLBACK
}

function fromSpec(spec: ComponentSpec): PageEntry {
  return {
    id: spec.id,
    title: spec.title,
    blurb: spec.blurb,
    section: sectionOfSpec(spec),
    kind: spec.kind,
    status: spec.status,
    leftovers: spec.leftovers,
    ...(spec.island === undefined ? { render: spec.render } : { island: spec.island }),
  }
}

function fromEntry(entry: StyleguideEntry): PageEntry {
  return {
    id: entry.id,
    title: entry.title,
    blurb: entry.blurb,
    // The contract's, never re-derived.
    section: entry.section,
    kind: bandOfEntry(entry),
    status: entry.status,
    owner: entry.owner,
    ...(entry.placeholder === true ? { placeholder: entry.placeholder } : {}),
    ...(entry.island === undefined ? { render: entry.render } : { island: entry.island }),
  }
}

/** Every page entry, unsorted — the specs first, then the registered entries. */
export function pageEntries(
  components: readonly ComponentSpec[],
  entries: readonly StyleguideEntry[] = ENTRIES
): PageEntry[] {
  return [...components.map(fromSpec), ...entries.map(fromEntry)]
}

/**
 * The whole page: four sections in `sections.json` order, each banded in
 * `BAND_ORDER`, empty bands dropped. Within a band, entries keep the order
 * their source list gave them.
 */
export function buildPage(
  components: readonly ComponentSpec[],
  entries: readonly StyleguideEntry[] = ENTRIES
): PageSection[] {
  const all = pageEntries(components, entries)
  return SECTIONS.map((section) => {
    const mine = all.filter((entry) => entry.section === section.id)
    const bands = BAND_ORDER[section.id]
      .map((kind) => ({ kind, entries: mine.filter((entry) => entry.kind === kind) }))
      .filter((band) => band.entries.length > 0)
    return { section, bands, entries: bands.flatMap((band) => band.entries) }
  })
}

/**
 * The section label the summary line uses — the contract's title without its
 * number (`2 General components` → `general components`).
 */
export function sectionLabel(section: StyleguideSection): string {
  return section.title.replace(/^\d+\s+/, ``).toLowerCase()
}

/**
 * The label ONE segment of the section bar carries: the contract's title
 * stripped to its leading noun (`2 General components` → `General`). Four
 * numbered titles do not fit a sidebar, and a capsule that ellipsises its own
 * section names is worse than a short one — the digits are printed once, in
 * the toolbar hint, and the full title rides the segment's tooltip, the nav's
 * blurb line and every entry's meta line.
 */
export function sectionShortLabel(section: StyleguideSection): string {
  return sectionLabel(section).split(` `)[0]!.replace(/^./, (first) => first.toUpperCase())
}
