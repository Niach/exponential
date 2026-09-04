import { alias } from "drizzle-orm/pg-core"
import { and, eq, inArray, ne, or, sql } from "drizzle-orm"
import {
  ISSUE_RELATION_LABELS,
  type IssueRelationSource,
  type IssueRelationType,
} from "@exp/db-schema/domain"
import {
  boards,
  comments,
  issueRelations,
  issues,
  type IssueRelation,
} from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import { recordIssueEvent } from "@/lib/integrations/activity"
import { resolveIssueRefs } from "@/lib/integrations/mentions"
import { extractIssueRefs } from "@/lib/issue-refs"

// EXP-736 — the ONE place issue relations are written, and the ONE place the
// per-side labels and the activity phrases are derived from.
//
// Storage is canonical-direction (see @exp/db-schema domain.ts): `blocks` =
// issue blocks related, `parent` = issue is parent of related, `duplicate` =
// issue is the duplicate and related the canonical (dual-written with
// issues.duplicate_of_id), `related` = symmetric and normalized so
// issue_id < related_issue_id. Every writer funnels through
// canonicalizeRelation, so a pick made from either side lands on the SAME row
// and the UNIQUE(issue_id, related_issue_id, type) index actually dedupes.
//
// This module is import-safe from the client: the only value imports are
// drizzle helpers, the schema tables and lib/integrations/*, whose own
// `db` imports are type-only and elided. The pure half (canonicalizeRelation,
// relationLabel, relationEventPhrase) is what the relations card and the
// timeline read.

type Tx = Parameters<
  // eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
  Parameters<typeof import("@/db/connection").db.transaction>[0]
>[0]

// eslint-disable-next-line quotes -- esbuild rejects template literals inside typeof import()
type Db = typeof import("@/db/connection").db

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

/** The per-side display label, byte-locked against contract.json. */
export function relationLabel(
  type: IssueRelationType,
  direction: RelationDirection
): string {
  const labels = ISSUE_RELATION_LABELS[type]
  return direction === `forward` ? labels.forward : labels.inverse
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

  if (!type || type === `related`) {
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

async function identifiersFor(
  tx: Tx,
  ids: string[]
): Promise<Map<string, string>> {
  const rows = await tx
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(inArray(issues.id, ids))
  return new Map(rows.map((row) => [row.id, row.identifier]))
}

// One event row PER issue, so both sides' timelines read naturally: the
// issue_id side gets `direction: 'forward'`, the related side `'inverse'`, and
// each payload names the OTHER issue.
async function recordRelationEvents(
  tx: Tx,
  args: {
    row: Pick<
      IssueRelation,
      `issueId` | `relatedIssueId` | `type` | `source` | `teamId`
    >
    kind: RelationEventKind
    actorUserId: string | null
  }
): Promise<void> {
  const { row, kind, actorUserId } = args
  const identifiers = await identifiersFor(tx, [
    row.issueId,
    row.relatedIssueId,
  ])
  const sides: Array<{ issueId: string; other: string; direction: RelationDirection }> = [
    { issueId: row.issueId, other: row.relatedIssueId, direction: `forward` },
    { issueId: row.relatedIssueId, other: row.issueId, direction: `inverse` },
  ]
  for (const side of sides) {
    await recordIssueEvent(tx, {
      issueId: side.issueId,
      teamId: row.teamId,
      actorUserId,
      type: kind,
      payload: {
        type: row.type,
        relatedIssueId: side.other,
        relatedIdentifier: identifiers.get(side.other) ?? null,
        direction: side.direction,
        source: row.source,
      },
    })
  }
}

/**
 * Insert ONE canonical relation row. Returns the row only when it was really
 * inserted — an existing row is a no-op, except that an explicit `user` pick
 * over an auto-derived `reference` row UPGRADES its source (a manual link
 * must survive the `#IDENT` token being edited out). Both issues get a
 * `relation_added` event on a real insert, and never on a no-op or an upgrade.
 *
 * `args` must already be canonical — callers go through canonicalizeRelation.
 */
export async function insertRelationInTx(
  tx: Tx,
  args: {
    issueId: string
    relatedIssueId: string
    type: IssueRelationType
    source: IssueRelationSource
    teamId: string
    actorUserId: string | null
  }
): Promise<IssueRelation | null> {
  const { issueId, relatedIssueId, type, source, teamId, actorUserId } = args
  if (issueId === relatedIssueId) return null

  const pair = and(
    eq(issueRelations.issueId, issueId),
    eq(issueRelations.relatedIssueId, relatedIssueId),
    eq(issueRelations.type, type)
  )

  const [existing] = await tx
    .select({ id: issueRelations.id, source: issueRelations.source })
    .from(issueRelations)
    .where(pair)
    .limit(1)
  if (existing) {
    if (source === `user` && existing.source === `reference`) {
      await tx
        .update(issueRelations)
        .set({ source: `user` })
        .where(eq(issueRelations.id, existing.id))
    }
    return null
  }

  const [inserted] = await tx
    .insert(issueRelations)
    .values({
      issueId,
      relatedIssueId,
      type,
      source,
      teamId,
      // populate_issue_relation_board_id overwrites with issue-derived truth;
      // passed as a subselect to satisfy the NOT NULL insert contract (same
      // shape recordIssueEvent uses).
      boardId: sql`(select ${issues.boardId} from ${issues} where ${issues.id} = ${issueId})`,
    })
    // A concurrent writer may have won the pair between the select and here.
    .onConflictDoNothing()
    .returning()
  if (!inserted) return null

  await recordRelationEvents(tx, {
    row: inserted,
    kind: `relation_added`,
    actorUserId,
  })
  return inserted
}

/**
 * Delete ONE relation row, by id or by its canonical pair. Records
 * `relation_removed` on both issues when a row actually died; returns the
 * deleted row, or null when there was nothing to delete.
 */
export async function deleteRelationInTx(
  tx: Tx,
  target:
    | { id: string }
    | { issueId: string; relatedIssueId: string; type: IssueRelationType }
    | {
        issueId: string
        relatedIssueId: string
        type: IssueRelationType
        source: IssueRelationSource
      },
  actorUserId: string | null
): Promise<IssueRelation | null> {
  const where =
    `id` in target
      ? eq(issueRelations.id, target.id)
      : and(
          eq(issueRelations.issueId, target.issueId),
          eq(issueRelations.relatedIssueId, target.relatedIssueId),
          eq(issueRelations.type, target.type),
          ...(`source` in target
            ? [eq(issueRelations.source, target.source)]
            : [])
        )

  const [deleted] = await tx
    .delete(issueRelations)
    .where(where)
    .returning()
  if (!deleted) return null

  await recordRelationEvents(tx, {
    row: deleted,
    kind: `relation_removed`,
    actorUserId,
  })
  return deleted
}

/**
 * Keep the `duplicate` relation row in lockstep with `issues.duplicate_of_id`
 * (the dual-write, EXP-736). Called from EVERY writer that persists the
 * column: issues.update/bulkUpdate (finalizeIssueUpdateInTx), the PR
 * lifecycle automation and the status-deletion reassignment — the last two
 * clear it through applyStatusDerivations when an issue moves off
 * `duplicate`.
 *
 * Deliberately RECONCILING rather than delta-driven: it compares the mirror to
 * the column's ACTUAL next state on every call, never to the previous value.
 * A `previous === next` short-circuit would make a missing row (a link written
 * before EXP-736 and only backfilled by 0099, a row lost to a repointed
 * canonical) permanently unrepairable — the one write that could heal it is
 * exactly the write that looks like a no-op. Both halves are idempotent, so
 * the redundant calls cost one indexed lookup and record no events.
 */
export async function syncDuplicateMirror(
  tx: Tx,
  args: {
    issueId: string
    teamId: string
    actorUserId: string | null
    /** The value the column held. Kept for the call-site contract (and what
     * the router tests read); the reconcile above deliberately does NOT
     * branch on it. */
    previousDuplicateOfId: string | null
    nextDuplicateOfId: string | null
  }
): Promise<void> {
  const { issueId, teamId, actorUserId } = args
  const next = args.nextDuplicateOfId

  // By (issue, type) rather than the exact pair: the canonical issue may have
  // been repointed, and an issue is a duplicate of at most one. Anything
  // pointing somewhere other than `next` is stale by definition.
  const stale = await tx
    .delete(issueRelations)
    .where(
      and(
        eq(issueRelations.issueId, issueId),
        eq(issueRelations.type, `duplicate`),
        ...(next != null ? [ne(issueRelations.relatedIssueId, next)] : [])
      )
    )
    .returning()
  for (const row of stale) {
    await recordRelationEvents(tx, {
      row,
      kind: `relation_removed`,
      actorUserId,
    })
  }

  if (next != null) {
    // No-op when the row already stands (and no event with it); writes the
    // missing mirror when it does not.
    await insertRelationInTx(tx, {
      issueId,
      relatedIssueId: next,
      type: `duplicate`,
      source: `user`,
      teamId,
      actorUserId,
    })
  }
}

/**
 * Delta-maintain the `related`/`reference` rows behind `#IDENT` tokens
 * (lib/issue-refs.ts), mirroring the @mention delta in resolveMentions:
 *
 * - tokens ADDED by this edit resolve to same-team, visible issues and get a
 *   canonical `related` row (a no-op when any row for the pair exists, so a
 *   manual link is never downgraded);
 * - tokens REMOVED by this edit only drop their row when the identifier no
 *   longer appears ANYWHERE on the issue — the new text plus every other slot
 *   (description + comments). `excludeCommentId` names the comment slot this
 *   call is replacing; its ABSENCE means the call is replacing the
 *   DESCRIPTION, so the stored description is skipped instead.
 * - only `source='reference'` rows are removed. An explicit pick stays.
 */
export async function syncReferenceRelations(
  tx: Tx,
  args: {
    issueId: string
    teamId: string
    actorUserId: string | null
    previousText: string
    nextText: string
    excludeCommentId?: string | null
  }
): Promise<void> {
  const { issueId, teamId, actorUserId, excludeCommentId } = args
  const previous = new Set(extractIssueRefs(args.previousText))
  const next = new Set(extractIssueRefs(args.nextText))
  const added = [...next].filter((identifier) => !previous.has(identifier))
  const removed = [...previous].filter((identifier) => !next.has(identifier))
  if (added.length === 0 && removed.length === 0) return

  if (added.length > 0) {
    const targets = await resolveIssueRefs(tx, args.nextText, teamId, {
      excludeIssueId: issueId,
    })
    for (const target of targets) {
      if (!added.includes(target.identifier.toUpperCase())) continue
      const canonical = canonicalizeRelation(issueId, target.id, `related`)
      await insertRelationInTx(tx, {
        ...canonical,
        source: `reference`,
        teamId,
        actorUserId,
      })
    }
  }

  if (removed.length === 0) return

  // Everything the identifier could still be written in, AFTER this edit.
  const surviving = [args.nextText]
  if (excludeCommentId === undefined || excludeCommentId === null) {
    const commentRows = await tx
      .select({ body: comments.body })
      .from(comments)
      .where(eq(comments.issueId, issueId))
    surviving.push(...commentRows.map((row) => row.body))
  } else {
    const [issue] = await tx
      .select({ description: issues.description })
      .from(issues)
      .where(eq(issues.id, issueId))
      .limit(1)
    surviving.push(issue?.description ?? ``)
    const commentRows = await tx
      .select({ body: comments.body })
      .from(comments)
      .where(
        and(
          eq(comments.issueId, issueId),
          ne(comments.id, excludeCommentId)
        )
      )
    surviving.push(...commentRows.map((row) => row.body))
  }
  const stillReferenced = new Set(
    surviving.flatMap((text) => extractIssueRefs(text))
  )
  const orphaned = removed.filter(
    (identifier) => !stillReferenced.has(identifier)
  )
  if (orphaned.length === 0) return

  const orphanRows = await tx
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(and(eq(issues.teamId, teamId), inArray(issues.identifier, orphaned)))
  for (const row of orphanRows) {
    if (row.id === issueId) continue
    const canonical = canonicalizeRelation(issueId, row.id, `related`)
    await deleteRelationInTx(
      tx,
      { ...canonical, source: `reference` },
      actorUserId
    )
  }
}

export interface IssueRelationView {
  id: string
  type: IssueRelationType
  source: IssueRelationSource
  issueId: string
  relatedIssueId: string
  /** Which half of the label pair this issue reads: it is the `issue_id` side
   * (`forward`) or the `related_issue_id` side (`inverse`). */
  direction: RelationDirection
  otherIssueId: string
  otherIdentifier: string
  /** The far issue's board — the caller's grant/permission check reads it
   * (MCP confines a token to the boards its grant names). */
  otherBoardId: string
  /** The far issue's team, for the same reason. */
  otherTeamId: string
}

/**
 * BOTH sides of one issue's relation graph, folded to that issue's point of
 * view. The catch-up read behind `issues.get` and the MCP issue tool — the
 * continuous delivery path is the `issue_relations` shape.
 *
 * The far issue's board is joined through `boardVisible()`: a relation whose
 * other side sits on a trashed or archived board is DROPPED, exactly as the
 * shape's `board_deleted_at`/`board_archived_at` mirrors drop it. Without the
 * predicate this read is the one place a hidden board's identifiers leak.
 * Membership is NOT re-checked here — every caller has already resolved access
 * to the subject issue's team, and a relation may only be written within one
 * team.
 */
export async function loadIssueRelations(
  db: Db | Tx,
  issueId: string
): Promise<IssueRelationView[]> {
  const other = alias(issues, `other_issue`)
  const rows = await db
    .select({
      id: issueRelations.id,
      type: issueRelations.type,
      source: issueRelations.source,
      issueId: issueRelations.issueId,
      relatedIssueId: issueRelations.relatedIssueId,
      otherId: other.id,
      otherIdentifier: other.identifier,
      otherBoardId: other.boardId,
      otherTeamId: other.teamId,
    })
    .from(issueRelations)
    .innerJoin(
      other,
      eq(
        other.id,
        sql`case when ${issueRelations.issueId} = ${issueId} then ${issueRelations.relatedIssueId} else ${issueRelations.issueId} end`
      )
    )
    .innerJoin(boards, eq(boards.id, other.boardId))
    .where(
      and(
        or(
          eq(issueRelations.issueId, issueId),
          eq(issueRelations.relatedIssueId, issueId)
        ),
        boardVisible()
      )
    )

  return rows.map((row) => ({
    id: row.id,
    type: row.type,
    source: row.source,
    issueId: row.issueId,
    relatedIssueId: row.relatedIssueId,
    direction: row.issueId === issueId ? (`forward` as const) : (`inverse` as const),
    otherIssueId: row.otherId,
    otherIdentifier: row.otherIdentifier,
    otherBoardId: row.otherBoardId,
    otherTeamId: row.otherTeamId,
  }))
}
