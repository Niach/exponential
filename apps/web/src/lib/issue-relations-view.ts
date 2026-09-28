// EXP-1097: what the issue detail DRAWS for its relations, on every client —
// the "Sub-issue of" parent line above the title, the Linear-style
// Sub-issues section (completion ring + `done/total`, rows, only a `+`), and
// ONE foldable band per remaining relation side (Blocked by / Blocking /
// Duplicate of / Duplicated by / Related). Phones draw the same bands inside
// the properties sheet. Pure and FIXTURE-LOCKED ×4
// (`packages/domain-contract/fixtures/issue-relations-view.json`): desktop
// `domain::relations_view`, iOS `IssueRelationsView.swift`, Android
// `IssueRelationsView.kt` return byte-identical output, copy included.
//
// Storage stays canonical-direction (`lib/issue-relations.ts`): `parent` =
// issueId is the parent of relatedIssueId, `blocks` = issueId blocks
// relatedIssueId, `duplicate` = issueId duplicates relatedIssueId, `related`
// = symmetric. A row whose other end is not synced is dropped (it would have
// no identifier to draw); an unknown type folds into Related.

/** A band's side key — its fold state is stored per key. */
export type RelationBandKey =
  | `blocked_by`
  | `blocking`
  | `duplicate_of`
  | `duplicated_by`
  | `related`

export interface RelationsViewIssue {
  id: string
  identifier: string
  title: string
  /** The dual-written ANCHOR enum (`issues.status`). */
  status: string
}

export interface RelationsViewRelation {
  type: string
  issueId: string
  relatedIssueId: string
}

export interface RelationsViewInput {
  subjectId: string
  relations: RelationsViewRelation[]
  /** Every synced issue the relations may name (the team's issues). */
  issues: RelationsViewIssue[]
  /** Bands the user folded/unfolded this session, overriding the default. */
  toggled: RelationBandKey[]
  /** Bands whose "Show N more" was pressed. */
  showAll: RelationBandKey[]
}

export interface RelationsViewRow {
  id: string
  identifier: string
  title: string
  status: string
  open: boolean
}

export interface RelationsViewBand {
  key: RelationBandKey
  title: string
  count: number
  openCount: number
  expanded: boolean
  /** The rows drawn right now (none while folded). */
  rows: RelationsViewRow[]
  /** "Show N more", null when nothing is hidden. */
  more: string | null
  /** "Show less", only once "Show N more" was pressed and rows exceed the cap. */
  less: string | null
}

export interface RelationsView {
  /** The "Sub-issue of" line's parent, null when the subject has none. */
  parent: RelationsViewRow | null
  subIssues: {
    rows: RelationsViewRow[]
    done: number
    total: number
    /** `2/5`, null when there are no sub-issues. */
    progress: string | null
  }
  bands: RelationsViewBand[]
  /** Total rows across the bands (the phone sheet's "Relations" count). */
  relationCount: number
}

// ── Copy (byte-identical ×4) ────────────────────────────────────────────
export const RELATIONS_VIEW_COPY = {
  subIssues: `Sub-issues`,
  subIssueOf: `Sub-issue of`,
  addSubIssues: `Add sub-issues`,
  relations: `Relations`,
  add: `Add`,
  blockedBy: `Blocked by`,
  blocking: `Blocking`,
  duplicateOf: `Duplicate of`,
  duplicatedBy: `Duplicated by`,
  related: `Related`,
  showLess: `Show less`,
} as const

export const relationsShowMore = (count: number) => `Show ${count} more`
export const relationsProgress = (done: number, total: number) =>
  `${done}/${total}`

/** Rows a band shows before "Show N more". */
export const RELATIONS_BAND_CAP = 3

const CLOSED_ANCHORS = new Set<string>([`done`, `cancelled`, `duplicate`])

const BAND_ORDER: RelationBandKey[] = [
  `blocked_by`,
  `blocking`,
  `duplicate_of`,
  `duplicated_by`,
  `related`,
]

const BAND_TITLES: Record<RelationBandKey, string> = {
  blocked_by: RELATIONS_VIEW_COPY.blockedBy,
  blocking: RELATIONS_VIEW_COPY.blocking,
  duplicate_of: RELATIONS_VIEW_COPY.duplicateOf,
  duplicated_by: RELATIONS_VIEW_COPY.duplicatedBy,
  related: RELATIONS_VIEW_COPY.related,
}

/** The identifier's trailing number, for a natural `EXP-9 < EXP-10` order. */
function identifierNumber(identifier: string): number {
  const match = /(\d+)$/.exec(identifier)
  return match ? Number(match[1]) : Number.MAX_SAFE_INTEGER
}

function byIdentifier(a: RelationsViewRow, b: RelationsViewRow): number {
  const byNumber = identifierNumber(a.identifier) - identifierNumber(b.identifier)
  if (byNumber !== 0) return byNumber
  return a.identifier < b.identifier ? -1 : a.identifier > b.identifier ? 1 : 0
}

/** Open rows first, each half in identifier order. */
function openFirst(a: RelationsViewRow, b: RelationsViewRow): number {
  if (a.open !== b.open) return a.open ? -1 : 1
  return byIdentifier(a, b)
}

function toRow(issue: RelationsViewIssue): RelationsViewRow {
  return {
    id: issue.id,
    identifier: issue.identifier,
    title: issue.title,
    status: issue.status,
    open: !CLOSED_ANCHORS.has(issue.status),
  }
}

export function issueRelationsView(input: RelationsViewInput): RelationsView {
  const byId = new Map(input.issues.map((issue) => [issue.id, issue]))
  const subject = input.subjectId
  let parent: RelationsViewRow | null = null
  const children: RelationsViewRow[] = []
  const buckets = new Map<RelationBandKey, RelationsViewRow[]>()
  const seen = new Set<string>()

  const push = (key: RelationBandKey, row: RelationsViewRow) => {
    const dedupe = `${key}:${row.id}`
    if (seen.has(dedupe)) return
    seen.add(dedupe)
    const bucket = buckets.get(key)
    if (bucket) bucket.push(row)
    else buckets.set(key, [row])
  }

  for (const relation of input.relations) {
    const forward = relation.issueId === subject
    const inverse = relation.relatedIssueId === subject
    if (forward === inverse) continue // not about the subject (or a self-loop)
    const other = byId.get(forward ? relation.relatedIssueId : relation.issueId)
    if (!other) continue
    const row = toRow(other)
    switch (relation.type) {
      case `parent`:
        if (forward) {
          if (!seen.has(`child:${row.id}`)) {
            seen.add(`child:${row.id}`)
            children.push(row)
          }
        } else if (!parent || byIdentifier(row, parent) < 0) {
          parent = row
        }
        break
      case `blocks`:
        push(forward ? `blocking` : `blocked_by`, row)
        break
      case `duplicate`:
        push(forward ? `duplicate_of` : `duplicated_by`, row)
        break
      default:
        push(`related`, row)
    }
  }

  children.sort(byIdentifier)
  const done = children.filter((row) => !row.open).length

  const toggled = new Set(input.toggled)
  const showAll = new Set(input.showAll)
  const bands: RelationsViewBand[] = BAND_ORDER.filter((key) =>
    buckets.has(key)
  ).map((key) => {
    const all = (buckets.get(key) ?? []).slice().sort(openFirst)
    const openCount = all.filter((row) => row.open).length
    // Blockers are the actionable relation: they open by default while one
    // is still open. Duplicates and Related stay folded.
    const openByDefault =
      (key === `blocked_by` || key === `blocking`) && openCount > 0
    const expanded = toggled.has(key) ? !openByDefault : openByDefault
    const everything = showAll.has(key)
    const overflow = all.length > RELATIONS_BAND_CAP
    const rows = !expanded
      ? []
      : everything || !overflow
        ? all
        : all.slice(0, RELATIONS_BAND_CAP)
    return {
      key,
      title: BAND_TITLES[key],
      count: all.length,
      openCount,
      expanded,
      rows,
      more:
        expanded && overflow && !everything
          ? relationsShowMore(all.length - RELATIONS_BAND_CAP)
          : null,
      less:
        expanded && overflow && everything ? RELATIONS_VIEW_COPY.showLess : null,
    }
  })

  return {
    parent,
    subIssues: {
      rows: children,
      done,
      total: children.length,
      progress: children.length > 0 ? relationsProgress(done, children.length) : null,
    },
    bands,
    relationCount: bands.reduce((sum, band) => sum + band.count, 0),
  }
}
