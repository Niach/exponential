import { ISSUE_RELATION_LABELS, type IssueRelationType } from "@exp/db-schema/domain"

// EXP-736 — the PURE half of issue relations (canonical direction, the
// per-side labels, the activity phrases): what the relations card, the
// timeline and the PR graph read. Client-safe by construction: no db, no
// server modules. The writers live in `lib/issue-relations.ts` (server).

/** Which side of a stored row an issue sits on. */
export type RelationDirection = `forward` | `inverse`

export interface CanonicalRelation {
  issueId: string
  relatedIssueId: string
  type: IssueRelationType
}

/**
 * Fold a client's `{issueId, relatedIssueId, type, inverse}` pick into the one
 * stored direction. `inverse` swaps the pair ("sub-issue of" is "parent" seen
 * from the child); `related` is symmetric, so the pair is additionally ordered
 * by id — otherwise the same link could exist as two rows.
 */
export function canonicalizeRelation(
  issueId: string,
  otherId: string,
  type: IssueRelationType,
  inverse = false
): CanonicalRelation {
  let from = inverse ? otherId : issueId
  let to = inverse ? issueId : otherId
  if (type === `related` && from > to) {
    const swap = from
    from = to
    to = swap
  }
  return { issueId: from, relatedIssueId: to, type }
}

/**
 * The per-side display label, byte-locked against contract.json.
 *
 * A type the table does not know (a fifth relation type from a NEWER server
 * reaching an old tab, since rows arrive over Electric and are never version
 * gated) degrades to the symmetric `related` phrasing — the same
 * "never drop, never crash" rule the unknown status category follows. Reading
 * `.forward` off `undefined` would throw INSIDE the timeline render.
 */
export function relationLabel(
  type: IssueRelationType,
  direction: RelationDirection
): string {
  const labels = ISSUE_RELATION_LABELS[type] ?? ISSUE_RELATION_LABELS.related
  return direction === `forward` ? labels.forward : labels.inverse
}

export type RelationSideKey = `${IssueRelationType}:${RelationDirection}`

/**
 * EXP-760: the Linear-style section headings the relations block renders
 * DIRECTLY (no "Relations" card title above them). Both `related` sides share
 * one heading; the order is the render order. Desktop mirrors this table in
 * `crates/domain/src/relations.rs` (`group_title`), lock-tested on both ends.
 */
export const RELATION_GROUP_TITLES: Record<RelationSideKey, string> = {
  "parent:forward": `Sub-issues`,
  "parent:inverse": `Parent`,
  "blocks:inverse": `Blocked by`,
  "blocks:forward": `Blocks`,
  "duplicate:forward": `Duplicate of`,
  "duplicate:inverse": `Duplicated by`,
  "related:forward": `Related`,
  "related:inverse": `Related`,
}

export const RELATION_GROUP_ORDER: RelationSideKey[] = [
  `parent:forward`,
  `parent:inverse`,
  `blocks:inverse`,
  `blocks:forward`,
  `duplicate:forward`,
  `duplicate:inverse`,
  `related:forward`,
]

/** Heading for one side; unknown types degrade to "Related" (see relationLabel). */
export function relationGroupTitle(
  type: IssueRelationType,
  direction: RelationDirection
): string {
  return (
    RELATION_GROUP_TITLES[`${type}:${direction}`] ??
    RELATION_GROUP_TITLES[`related:forward`]
  )
}

export interface RelationGroup<T> {
  key: RelationSideKey
  title: string
  rows: T[]
}

/**
 * Fold flat rows into ordered heading groups. `related:inverse` merges into
 * the `related:forward` group; unknown types land in "Related" too. Rows keep
 * their input order inside a group (the hook already sorts by identifier).
 */
export function groupRelationRows<
  T extends { type: IssueRelationType; direction: RelationDirection },
>(rows: T[]): RelationGroup<T>[] {
  const buckets = new Map<RelationSideKey, T[]>()
  for (const row of rows) {
    const sideKey: RelationSideKey = `${row.type}:${row.direction}`
    const key: RelationSideKey =
      row.type === `related` || !(sideKey in RELATION_GROUP_TITLES)
        ? `related:forward`
        : sideKey
    const bucket = buckets.get(key)
    if (bucket) bucket.push(row)
    else buckets.set(key, [row])
  }
  return RELATION_GROUP_ORDER.filter((key) => buckets.has(key)).map((key) => ({
    key,
    title: RELATION_GROUP_TITLES[key],
    rows: buckets.get(key) ?? [],
  }))
}

export type RelationEventKind = `relation_added` | `relation_removed`

/**
 * The ONE phrase table for the two relation activity events. `related` reads
 * as an addition/removal (it is symmetric, so "marked as related to" would be
 * noise); every other type reads as a state the issue was put in or left.
 * iOS EventPhrases, Android EventRow and desktop timeline.rs lock against
 * these exact strings.
 */
export function relationEventParts(
  kind: RelationEventKind,
  payload: Record<string, unknown>
): { prefix: string; identifier: string | null } {
  const type = payload.type as IssueRelationType | undefined
  const direction: RelationDirection =
    payload.direction === `inverse` ? `inverse` : `forward`
  const identifier =
    typeof payload.relatedIdentifier === `string` &&
    payload.relatedIdentifier.length > 0
      ? payload.relatedIdentifier
      : null

  // Unknown types read as the symmetric addition/removal too (see
  // relationLabel): "added related issue EXP-3" is wrong-ish but harmless,
  // where a thrown label is a blank timeline.
  if (!type || !(type in ISSUE_RELATION_LABELS) || type === `related`) {
    return {
      prefix: kind === `relation_added` ? `added related issue` : `removed related issue`,
      identifier,
    }
  }
  const label = relationLabel(type, direction)
  return {
    prefix: kind === `relation_added` ? `marked as ${label}` : `no longer ${label}`,
    identifier,
  }
}

/** The flattened phrase — `relationEventParts` with the identifier inlined. */
export function relationEventPhrase(
  kind: RelationEventKind,
  payload: Record<string, unknown>
): string {
  const { prefix, identifier } = relationEventParts(kind, payload)
  return `${prefix} ${identifier ?? `an issue`}`
}
