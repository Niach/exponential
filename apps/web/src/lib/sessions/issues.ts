import { TRPCError } from "@trpc/server"
import { and, eq, inArray, ne, sql } from "drizzle-orm"
import type { SessionIssueSource } from "@exp/db-schema/domain"
import { boards, issues, sessionIssues, sessions, type Session } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import type { IssueEventOrigin } from "@/lib/integrations/event-origin"
import { applyIssuesTakenInTx, applyIssuesUnlinkedInTx } from "@/lib/sessions/issue-lifecycle"
import { postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"

// NEW-1: a session's issues (`session_issues`, n ↔ n). A row sits on the
// session that HOLDS the issue: a link lands on the named session, the
// issues a start names on the new session, a filed issue on the filer; a
// forward (`issueIds` on a message) MOVES the tree's rows onto the target;
// a continuation takes over the ended session's rows. A root's list = the
// union over its tree. Each new link posts an `event` card on the ROOT
// (`issue_linked`, or `issue_filed` when the agent created the issue); a
// re-link is a no-op. Board + visibility mirrors are trigger-written. The
// lifecycle (issue-lifecycle.json): a row on a WORKER takes the issue.

type Holder = Pick<Session, `id` | `rootId` | `teamId` | `shared` | `userId` | `repositoryId`>

const HOLDER = {
  id: sessions.id,
  rootId: sessions.rootId,
  teamId: sessions.teamId,
  shared: sessions.shared,
  userId: sessions.userId,
  repositoryId: sessions.repositoryId,
}

async function loadHolder(tx: Tx, sessionId: string): Promise<Holder> {
  const [row] = await tx.select(HOLDER).from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  return row
}

/** The ids of every session of the root's tree. */
export async function treeSessionIds(tx: Tx, rootId: string): Promise<string[]> {
  const rows = await tx.select({ id: sessions.id }).from(sessions).where(eq(sessions.rootId, rootId))
  return rows.map((row) => row.id)
}

/** The given issues that exist in the team on a visible board, in input
 *  order; anything else is refused (the caller's input was wrong). */
export async function assertIssuesInTeam(
  tx: Tx,
  teamId: string,
  issueIds: string[]
): Promise<Array<{ id: string; boardId: string }>> {
  const unique = [...new Set(issueIds)]
  if (unique.length === 0) return []
  const rows = await tx
    .select({ id: issues.id, boardId: issues.boardId })
    .from(issues)
    .innerJoin(boards, eq(boards.id, issues.boardId))
    .where(and(inArray(issues.id, unique), eq(boards.teamId, teamId), boardVisible()))
  if (rows.length !== unique.length) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `Every issue must belong to the session's team`,
    })
  }
  const byId = new Map(rows.map((row) => [row.id, row]))
  return unique.map((id) => byId.get(id)!)
}

export interface LinkArgs {
  /** The session that takes the rows. */
  sessionId: string
  issueIds: string[]
  source: SessionIssueSource
  addedBy: string | null
  addedBySessionId: string | null
  /** The agent created the issue for this session (`issue_filed`). */
  filed?: boolean
  /** No event cards: a start's own issues (the brief names them). */
  quiet?: boolean
}

function origin(args: Pick<LinkArgs, `source` | `addedBySessionId`>): IssueEventOrigin | undefined {
  return args.source === `agent` ? { viaMcp: true, sessionId: args.addedBySessionId } : undefined
}

/** Rows for `issueIds` on `sessionId` (a re-link is a no-op), the worker
 *  lifecycle, the root's event cards. */
export async function linkIssues(tx: Tx, args: LinkArgs): Promise<{ linked: string[] }> {
  if (args.issueIds.length === 0) return { linked: [] }
  const holder = await loadHolder(tx, args.sessionId)
  const ordered = await assertIssuesInTeam(tx, holder.teamId, args.issueIds)
  const [{ next } = { next: 0 }] = await tx
    .select({ next: sql<number>`coalesce(max(${sessionIssues.sortOrder}), 0)` })
    .from(sessionIssues)
    .where(eq(sessionIssues.sessionId, holder.id))
  const inserted = await tx
    .insert(sessionIssues)
    .values(
      ordered.map((issue, index) => ({
        sessionId: holder.id,
        issueId: issue.id,
        teamId: holder.teamId,
        shared: holder.shared,
        ownerId: holder.userId,
        source: args.source,
        addedBy: args.addedBy,
        addedBySessionId: args.addedBySessionId,
        sortOrder: Number(next) + index + 1,
      }))
    )
    .onConflictDoNothing({ target: [sessionIssues.sessionId, sessionIssues.issueId] })
    .returning({ issueId: sessionIssues.issueId })
  const linked = inserted.map((row) => row.issueId)
  await applyIssuesTakenInTx(tx, {
    session: holder,
    issueIds: linked,
    actorUserId: args.addedBy,
    origin: origin(args),
  })
  if (!args.quiet) {
    for (const issueId of linked) {
      await postSystemCard(tx, holder.rootId, {
        kind: `event`,
        event: args.filed ? `issue_filed` : `issue_linked`,
        issueId,
        sessionId: holder.id,
      })
    }
  }
  return { linked }
}

/**
 * FORWARD `issueIds` to `sessionId`: the rows any session of its tree
 * holds move onto it (one row per issue; a row the target already holds
 * stays), the rest are linked fresh. The target takes them (lifecycle).
 */
export async function forwardIssues(
  tx: Tx,
  args: Omit<LinkArgs, `filed`>
): Promise<{ linked: string[]; moved: string[] }> {
  if (args.issueIds.length === 0) return { linked: [], moved: [] }
  const holder = await loadHolder(tx, args.sessionId)
  const ids = (await assertIssuesInTeam(tx, holder.teamId, args.issueIds)).map((issue) => issue.id)
  const tree = (await treeSessionIds(tx, holder.rootId)).filter((id) => id !== holder.id)
  const held = new Set(
    (
      await tx
        .select({ issueId: sessionIssues.issueId })
        .from(sessionIssues)
        .where(and(eq(sessionIssues.sessionId, holder.id), inArray(sessionIssues.issueId, ids)))
    ).map((row) => row.issueId)
  )
  const moved: string[] = []
  if (tree.length > 0) {
    // Rows elsewhere in the tree: the target already holds it → drop the
    // other row; else → move the row.
    await tx
      .delete(sessionIssues)
      .where(
        and(
          inArray(sessionIssues.sessionId, tree),
          inArray(sessionIssues.issueId, ids.filter((id) => held.has(id)))
        )
      )
    const movable = ids.filter((id) => !held.has(id))
    if (movable.length > 0) {
      const rows = await tx
        .selectDistinctOn([sessionIssues.issueId], {
          id: sessionIssues.id,
          issueId: sessionIssues.issueId,
        })
        .from(sessionIssues)
        .where(and(inArray(sessionIssues.sessionId, tree), inArray(sessionIssues.issueId, movable)))
        .orderBy(sessionIssues.issueId, sessionIssues.createdAt)
      if (rows.length > 0) {
        await tx
          .update(sessionIssues)
          .set({ sessionId: holder.id, shared: holder.shared, ownerId: holder.userId, updatedAt: new Date() })
          .where(inArray(sessionIssues.id, rows.map((row) => row.id)))
        // The other copies of a moved issue go.
        await tx
          .delete(sessionIssues)
          .where(
            and(
              inArray(sessionIssues.sessionId, tree),
              inArray(sessionIssues.issueId, rows.map((row) => row.issueId))
            )
          )
        moved.push(...rows.map((row) => row.issueId))
      }
    }
  }
  const fresh = ids.filter((id) => !held.has(id) && !moved.includes(id))
  const { linked } = await linkIssues(tx, { ...args, issueIds: fresh, quiet: true })
  // The target takes what it already held or received: In progress again.
  await applyIssuesTakenInTx(tx, {
    session: holder,
    issueIds: ids.filter((id) => !fresh.includes(id)),
    actorUserId: args.addedBy,
    origin: origin(args),
  })
  return { linked, moved }
}

/** A continuation (`to`) takes over the rows of the ended session `from`. */
export async function takeOverIssues(
  tx: Tx,
  from: Pick<Session, `id`>,
  to: Holder,
  actorUserId: string | null
): Promise<string[]> {
  const rows = await tx
    .update(sessionIssues)
    .set({ sessionId: to.id, shared: to.shared, ownerId: to.userId, updatedAt: new Date() })
    .where(eq(sessionIssues.sessionId, from.id))
    .returning({ issueId: sessionIssues.issueId })
  const ids = rows.map((row) => row.issueId)
  await applyIssuesTakenInTx(tx, { session: to, issueIds: ids, actorUserId })
  return ids
}

/** The issues `sessionId` holds (a person's message = the worker takes them
 *  again; bus rows never call this). */
export async function retakeHeldIssues(
  tx: Tx,
  session: Holder,
  actorUserId: string | null
): Promise<string[]> {
  const rows = await tx
    .select({ issueId: sessionIssues.issueId })
    .from(sessionIssues)
    .where(eq(sessionIssues.sessionId, session.id))
  return applyIssuesTakenInTx(tx, {
    session,
    issueIds: rows.map((row) => row.issueId),
    actorUserId,
  })
}

/** The person's message a MANUAL link posts (one for all newly linked
 *  issues), so the session wakes and takes the work. */
export function manualLinkText(identifiers: string[]): string | null {
  return identifiers.length ? `Link ${identifiers.map((ident) => `#${ident}`).join(` `)}` : null
}

/** The identifiers of `issueIds`, in input order. */
export async function issueIdentifiers(tx: Tx, issueIds: string[]): Promise<string[]> {
  if (issueIds.length === 0) return []
  const rows = await tx
    .select({ id: issues.id, identifier: issues.identifier })
    .from(issues)
    .where(inArray(issues.id, issueIds))
  const byId = new Map(rows.map((row) => [row.id, row.identifier]))
  return issueIds.map((id) => byId.get(id)).filter((ident): ident is string => !!ident)
}

/** Drop `issueId` from `sessionId` (else from whichever session of its
 *  tree holds it: the thread header's "−"). The lifecycle's `unlinked`. */
export async function unlinkIssue(
  tx: Tx,
  args: {
    sessionId: string
    issueId: string
    removedBy?: string | null
    removedBySessionId?: string | null
  }
): Promise<{ unlinked: boolean }> {
  const holder = await loadHolder(tx, args.sessionId)
  let deleted = await tx
    .delete(sessionIssues)
    .where(and(eq(sessionIssues.sessionId, holder.id), eq(sessionIssues.issueId, args.issueId)))
    .returning({ id: sessionIssues.id })
  if (deleted.length === 0) {
    const tree = (await treeSessionIds(tx, holder.rootId)).filter((id) => id !== holder.id)
    if (tree.length > 0) {
      deleted = await tx
        .delete(sessionIssues)
        .where(
          and(
            inArray(sessionIssues.sessionId, tree),
            eq(sessionIssues.issueId, args.issueId),
            ne(sessionIssues.sessionId, holder.id)
          )
        )
        .returning({ id: sessionIssues.id })
    }
  }
  if (deleted.length === 0) return { unlinked: false }
  await applyIssuesUnlinkedInTx(tx, {
    teamId: holder.teamId,
    issueIds: [args.issueId],
    actorUserId: args.removedBy ?? null,
    ...(args.removedBySessionId
      ? { origin: { viaMcp: true, sessionId: args.removedBySessionId } }
      : {}),
  })
  await postSystemCard(tx, holder.rootId, {
    kind: `event`,
    event: `issue_unlinked`,
    issueId: args.issueId,
    sessionId: args.removedBySessionId ?? null,
  })
  return { unlinked: true }
}
