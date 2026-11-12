// #EXP-1797 (issue-lifecycle.json `trash`): the issue trash. Every issue
// delete (tRPC issues.delete + bulkDelete, MCP issues_write {op: delete}, an
// app's exponential.issues.delete) lands here: a SOFT delete that stamps
// deleted_at + who, cascades nothing, and emits `issue.deleted`. Restore
// clears it (`issue.restored`); the watchdog's sweep purges it after the
// retention window, an owner's Delete forever purges it now (`issue.purged`).
// The child rows' board_deleted_at mirror follows the issue through the
// propagate_issue_deleted_at trigger, so the shapes need no writer help.

import { and, desc, eq, inArray, isNotNull, isNull, lte } from "drizzle-orm"
import {
  ISSUE_TRASH_EVENTS,
  ISSUE_TRASH_RETENTION_MS,
} from "@exp/domain-contract/issue-lifecycle"
import { db } from "@/db/connection"
import { attachments, boards, issues } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import { emitEventInTx, type EmitEventArgs } from "@/lib/events"
import type { IssueEventOrigin } from "@/lib/integrations/event-origin"
import { collectAttachmentStorageKeys } from "@/lib/storage/issue-attachments"
import type { Db, Tx } from "@/lib/sessions/tx"

/** Who trashed, restored or purged: the member and how they acted. `null` =
 *  the server's sweep. */
export type IssueTrashActor = {
  userId: string
  origin: IssueEventOrigin
} | null

type IssueIdentity = {
  id: string
  teamId: string
  identifier: string
  title: string
  boardId: string
}

const identityColumns = {
  id: issues.id,
  teamId: issues.teamId,
  identifier: issues.identifier,
  title: issues.title,
  boardId: issues.boardId,
}

/** The fixture's `eventPayload`, in order. */
export function issueTrashEventPayload(
  issue: Omit<IssueIdentity, `teamId`>,
  actor: IssueTrashActor
): Record<string, unknown> {
  return {
    issueId: issue.id,
    identifier: issue.identifier,
    title: issue.title,
    boardId: issue.boardId,
    actorUserId: actor?.userId ?? null,
    actorAgentId: actor?.origin.agentId ?? null,
    sessionId: actor?.origin.sessionId ?? null,
    viaMcp: actor?.origin.viaMcp ?? false,
  }
}

function emitterOf(actor: IssueTrashActor): EmitEventArgs[`emittedBy`] {
  if (!actor) return { server: true }
  if (actor.origin.sessionId) return { sessionId: actor.origin.sessionId }
  return { userId: actor.userId }
}

async function emitTrashEvent(
  tx: Tx,
  name: string,
  issue: IssueIdentity,
  actor: IssueTrashActor
): Promise<void> {
  await emitEventInTx(tx, {
    teamId: issue.teamId,
    name,
    payload: issueTrashEventPayload(issue, actor),
    emittedBy: emitterOf(actor),
  })
}

/** Moves the live issues among `issueIds` to the trash; an already-trashed
 *  id is skipped. Returns the trashed rows. */
export async function trashIssuesInTx(
  tx: Tx,
  issueIds: string[],
  actor: NonNullable<IssueTrashActor>,
  now: Date = new Date()
): Promise<IssueIdentity[]> {
  if (issueIds.length === 0) return []
  const trashed = await tx
    .update(issues)
    .set({
      deletedAt: now,
      deletedBy: actor.userId,
      deletedByAgentId: actor.origin.agentId ?? null,
    })
    .where(and(inArray(issues.id, issueIds), isNull(issues.deletedAt)))
    .returning(identityColumns)
  for (const issue of trashed) {
    await emitTrashEvent(tx, ISSUE_TRASH_EVENTS.deleted, issue, actor)
  }
  return trashed
}

/** Brings a trashed issue back. null = it was not in the trash. */
export async function restoreIssueInTx(
  tx: Tx,
  issueId: string,
  actor: NonNullable<IssueTrashActor>
): Promise<IssueIdentity | null> {
  const [restored] = await tx
    .update(issues)
    .set({ deletedAt: null, deletedBy: null, deletedByAgentId: null })
    .where(and(eq(issues.id, issueId), isNotNull(issues.deletedAt)))
    .returning(identityColumns)
  if (!restored) return null
  await emitTrashEvent(tx, ISSUE_TRASH_EVENTS.restored, restored, actor)
  return restored
}

/** Hard-deletes ONE trashed issue (the cascade the old delete had) and
 *  returns its files' storage keys for the caller to reclaim after commit.
 *  `cutoff` (the sweep) purges only an issue deleted at or before it, so a
 *  concurrent restore wins the race. */
export async function purgeIssueInTx(
  tx: Tx,
  issueId: string,
  opts: { actor: IssueTrashActor; cutoff?: Date }
): Promise<{ purged: IssueIdentity | null; storageKeys: string[] }> {
  const attachmentRows = await tx
    .select({
      storageKey: attachments.storageKey,
      posterStorageKey: attachments.posterStorageKey,
    })
    .from(attachments)
    .where(eq(attachments.issueId, issueId))
  const [purged] = await tx
    .delete(issues)
    .where(
      and(
        eq(issues.id, issueId),
        isNotNull(issues.deletedAt),
        opts.cutoff ? lte(issues.deletedAt, opts.cutoff) : undefined
      )
    )
    .returning(identityColumns)
  if (!purged) return { purged: null, storageKeys: [] }
  await emitTrashEvent(tx, ISSUE_TRASH_EVENTS.purged, purged, opts.actor)
  return { purged, storageKeys: collectAttachmentStorageKeys(attachmentRows) }
}

export type TrashedIssue = {
  id: string
  identifier: string
  title: string
  boardId: string
  deletedAt: Date
  deletedBy: string | null
  deletedByAgentId: string | null
  purgeAt: Date
}

/** The team's issue trash, newest deleted first; a trashed or archived
 *  board's issues are not listed (the board's own trash restores them). */
export async function listTrashedIssues(
  teamId: string,
  executor: Db = db
): Promise<TrashedIssue[]> {
  const rows = await executor
    .select({
      id: issues.id,
      identifier: issues.identifier,
      title: issues.title,
      boardId: issues.boardId,
      deletedAt: issues.deletedAt,
      deletedBy: issues.deletedBy,
      deletedByAgentId: issues.deletedByAgentId,
    })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(and(eq(issues.teamId, teamId), isNotNull(issues.deletedAt), boardVisible()))
    .orderBy(desc(issues.deletedAt))
  return rows.map((row) => ({
    ...row,
    deletedAt: row.deletedAt!,
    purgeAt: new Date(row.deletedAt!.getTime() + ISSUE_TRASH_RETENTION_MS),
  }))
}

/** The watchdog's sweep: purges every issue trashed past the retention
 *  window, one transaction each, then reclaims the files. Returns how many
 *  were purged. `deleteKeys` is injected so this module never reaches the
 *  S3 client itself. */
export async function runIssuePurgeSweep(
  now: Date = new Date(),
  opts: { executor?: Db; deleteKeys?: (keys: string[]) => Promise<void> } = {}
): Promise<number> {
  const executor = opts.executor ?? db
  const cutoff = new Date(now.getTime() - ISSUE_TRASH_RETENTION_MS)
  const due = await executor
    .select({ id: issues.id })
    .from(issues)
    .where(and(isNotNull(issues.deletedAt), lte(issues.deletedAt, cutoff)))
    .limit(200)
  let purged = 0
  for (const { id } of due) {
    const result = await executor.transaction((tx) =>
      purgeIssueInTx(tx, id, { actor: null, cutoff })
    )
    if (!result.purged) continue
    purged += 1
    if (result.storageKeys.length > 0) {
      const remove =
        opts.deleteKeys ??
        (await import(`@/lib/storage/bun-s3-cleanup`)).deleteStorageObjectsViaBun
      await remove(result.storageKeys)
    }
  }
  return purged
}
