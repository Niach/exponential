import { TRPCError } from "@trpc/server"
import { and, eq, inArray, sql } from "drizzle-orm"
import type { SessionIssueSource } from "@exp/db-schema/domain"
import { boards, issues, sessionIssues, sessions } from "@/db/schema"
import { boardVisible } from "@/lib/board-visibility"
import { applyIssuesStartedInTx } from "@/lib/integrations/pr-sync"
import { postSystemCard } from "@/lib/sessions/messages"
import type { Tx } from "@/lib/sessions/tx"

// EXP-1291: a thread's issues (`session_issues`, n ↔ n). ALWAYS on the ROOT:
// a child's link lands on its root, so the thread header lists one set. Each
// NEW link posts an `event` card on the root (`issue_linked`, or
// `issue_filed` when the agent created the issue for the thread); a re-link
// is a no-op. Board + visibility mirrors are trigger-written. EXP-1334: every
// NEW link (start facets, filed issues, MCP and manual links alike) starts a
// not-yet-started issue (pr-sync's `started` automation).

async function resolveRoot(tx: Tx, sessionId: string) {
  const [row] = await tx
    .select({
      rootId: sessions.rootId,
      teamId: sessions.teamId,
      shared: sessions.shared,
      userId: sessions.userId,
    })
    .from(sessions)
    .where(eq(sessions.id, sessionId))
    .limit(1)
  if (!row) throw new TRPCError({ code: `NOT_FOUND`, message: `Session not found` })
  return row
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

export async function linkIssues(
  tx: Tx,
  args: {
    rootId: string
    issueIds: string[]
    source: SessionIssueSource
    addedBy: string | null
    addedBySessionId: string | null
    /** The agent created the issue for this thread (`issue_filed`). */
    filed?: boolean
    /** No event cards: the thread's own start facets (the brief names them). */
    quiet?: boolean
  }
): Promise<{ linked: string[] }> {
  if (args.issueIds.length === 0) return { linked: [] }
  const root = await resolveRoot(tx, args.rootId)
  const ordered = await assertIssuesInTeam(tx, root.teamId, args.issueIds)
  const [{ next } = { next: 0 }] = await tx
    .select({ next: sql<number>`coalesce(max(${sessionIssues.sortOrder}), 0)` })
    .from(sessionIssues)
    .where(eq(sessionIssues.sessionId, root.rootId))
  const inserted = await tx
    .insert(sessionIssues)
    .values(
      ordered.map((issue, index) => ({
        sessionId: root.rootId,
        issueId: issue.id,
        teamId: root.teamId,
        shared: root.shared,
        ownerId: root.userId,
        source: args.source,
        addedBy: args.addedBy,
        addedBySessionId: args.addedBySessionId,
        sortOrder: Number(next) + index + 1,
      }))
    )
    .onConflictDoNothing({ target: [sessionIssues.sessionId, sessionIssues.issueId] })
    .returning({ issueId: sessionIssues.issueId })
  const linked = inserted.map((row) => row.issueId)
  await applyIssuesStartedInTx(tx, {
    issueIds: linked,
    teamId: root.teamId,
    actorUserId: args.addedBy,
    ...(args.source === `agent`
      ? { origin: { viaMcp: true, sessionId: args.addedBySessionId } }
      : {}),
  })
  if (!args.quiet) {
    for (const issueId of linked) {
      await postSystemCard(tx, root.rootId, {
        kind: `event`,
        event: args.filed ? `issue_filed` : `issue_linked`,
        issueId,
        sessionId: args.addedBySessionId,
      })
    }
  }
  return { linked }
}

/**
 * EXP-1341: hand issues to ONE session of the tree (the dispatcher groups
 * and forwards freely): link the new ones to the root (`linkIssues`), then
 * stamp `assigned_session_id` on all of them (a reassignment moves it). The
 * root itself = its own: the stamp clears.
 */
export async function assignIssues(
  tx: Tx,
  args: {
    sessionId: string
    issueIds: string[]
    source: SessionIssueSource
    addedBy: string | null
    addedBySessionId: string | null
    quiet?: boolean
  }
): Promise<{ linked: string[] }> {
  if (args.issueIds.length === 0) return { linked: [] }
  const root = await resolveRoot(tx, args.sessionId)
  const result = await linkIssues(tx, {
    rootId: root.rootId,
    issueIds: args.issueIds,
    source: args.source,
    addedBy: args.addedBy,
    addedBySessionId: args.addedBySessionId,
    quiet: args.quiet,
  })
  await tx
    .update(sessionIssues)
    .set({
      assignedSessionId: args.sessionId === root.rootId ? null : args.sessionId,
      updatedAt: new Date(),
    })
    .where(
      and(
        eq(sessionIssues.sessionId, root.rootId),
        inArray(sessionIssues.issueId, [...new Set(args.issueIds)])
      )
    )
  return result
}

/** EXP-1334: the person's message a MANUAL link posts on the root (one for
 *  all newly linked issues), so the dispatcher wakes and routes the work. */
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

export async function unlinkIssue(
  tx: Tx,
  args: { rootId: string; issueId: string; removedBySessionId?: string | null }
): Promise<{ unlinked: boolean }> {
  const root = await resolveRoot(tx, args.rootId)
  const deleted = await tx
    .delete(sessionIssues)
    .where(
      and(eq(sessionIssues.sessionId, root.rootId), eq(sessionIssues.issueId, args.issueId))
    )
    .returning({ id: sessionIssues.id })
  if (deleted.length === 0) return { unlinked: false }
  await postSystemCard(tx, root.rootId, {
    kind: `event`,
    event: `issue_unlinked`,
    issueId: args.issueId,
    sessionId: args.removedBySessionId ?? null,
  })
  return { unlinked: true }
}
