// EXP-897: starting an issue that something else BLOCKS. The launcher offers a
// third start mode there — a STACKED pull request, cut from the blocker's
// branch and based on its PR — so the diff shows only this issue's own work.
//
// The rule is ONE pure function plus the dialog's byte-locked copy, mirrored
// ×4 (iOS `ExpCore/Sources/Domain/StackStart.swift`, Android
// `domain/StackStart.kt`, desktop `chat_launch::blockers_of`) with the same
// three test names:
//
// 1. `counts only blocked-by relations`
// 2. `drops a blocker that is done, cancelled or a duplicate`
// 3. `drops a blocker whose issue row is not synced`

/** The dialog's title. Byte-identical ×4. */
export const BLOCKED_START_TITLE = `This issue is blocked`
/** The secondary button: start a plain run, ignoring the blockers. */
export const START_ANYWAY_LABEL = `Start anyway`
/** The primary button: start on top of the lowest blocker's branch. */
export const STACKED_PR_LABEL = `Stacked PR`
/** The body, around the blocker chips: `<prefix>` chips `<suffix>`. */
export const BLOCKED_START_BODY_PREFIX = `This issue is blocked by `
export const BLOCKED_START_BODY_SUFFIX = `. Start anyway, or start a stacked PR?`

/** The anchor statuses that mean a blocker is no longer in the way. A custom
 *  status anchors into one of these automatically (EXP-314), so this set is
 *  keyed on the dual-written ANCHOR enum, never on status rows. */
const CLOSED_ANCHORS = new Set<string>([`done`, `cancelled`, `duplicate`])

/** The synced `issue_relations` columns this rule reads. `type`/`source` are
 *  the contract's values; the DIRECTION is derived, never stored: a canonical
 *  `blocks` row is `issue_id` blocks `related_issue_id` (EXP-736). */
export interface StackStartRelation {
  type: string
  issueId: string
  relatedIssueId: string
}

/** What a blocker row must carry to be judged. */
export interface StackStartIssue {
  id: string
  identifier: string
  /** The dual-written ANCHOR enum (`issues.status`). */
  status: string
}

/**
 * The issues that BLOCK `issueId` and are still open, ordered by identifier.
 *
 * Only `blocks` relations count, and only from the blocked side (`direction
 * === 'inverse'`, i.e. the row's `related_issue_id` is this issue): a row this
 * issue blocks is not in its way. A blocker whose row has not synced (another
 * team's board, a trashed board) is dropped rather than rendered blank, and a
 * blocker that is done, cancelled or a duplicate is no blocker at all.
 */
export function openBlockers<T extends StackStartIssue>(
  issueId: string,
  relations: readonly StackStartRelation[],
  issues: readonly T[]
): T[] {
  const byId = new Map(issues.map((issue) => [issue.id, issue]))
  const seen = new Set<string>()
  const out: T[] = []
  for (const relation of relations) {
    if (relation.type !== `blocks`) continue
    // The inverse side only: `issue_id` is the blocker, `related_issue_id`
    // the blocked one.
    if (relation.relatedIssueId !== issueId) continue
    const blockerId = relation.issueId
    if (blockerId === issueId || seen.has(blockerId)) continue
    seen.add(blockerId)
    const blocker = byId.get(blockerId)
    if (!blocker) continue
    if (CLOSED_ANCHORS.has(blocker.status)) continue
    out.push(blocker)
  }
  out.sort((a, b) => (a.identifier < b.identifier ? -1 : a.identifier > b.identifier ? 1 : 0))
  return out
}

/** The body sentence for the surfaces that cannot host chips (a native alert,
 *  a toast): the same prefix and suffix around plain `#IDENT` identifiers. */
export function blockedStartBody(identifiers: readonly string[]): string {
  const names = identifiers.map((identifier) => `#${identifier}`).join(`, `)
  return `${BLOCKED_START_BODY_PREFIX}${names}${BLOCKED_START_BODY_SUFFIX}`
}
