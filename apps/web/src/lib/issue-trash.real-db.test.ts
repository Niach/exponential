// @vitest-environment node
import { describe, expect, it } from "vitest"
import { and, eq, inArray, sql } from "drizzle-orm"
import { ISSUE_TRASH_RETENTION_MS } from "@exp/domain-contract/issue-lifecycle"
import {
  boards,
  comments,
  events,
  issueRelations,
  issues,
  teamMembers,
  teams,
} from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import {
  listTrashedIssues,
  purgeIssueInTx,
  restoreIssueInTx,
  runIssuePurgeSweep,
  trashIssuesInTx,
} from "./issue-trash"

// #EXP-1797 (issue-lifecycle.json `trash`) against the REAL migrated
// Postgres with the custom triggers, inside a rolled-back transaction: the
// migration's columns, the child mirrors (propagate_issue_deleted_at, the
// board fan-out keeping a trashed issue's children hidden, the insert
// populate), restore, the sweep, Delete forever and the three events.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_issues_deleted')
     AND EXISTS (SELECT 1 FROM pg_trigger WHERE tgname = 'propagate_issue_deleted_at') AS ok
`)
if (!ready.ok) console.info(`[issue-trash.real-db] skipped: ${ready.reason}`)

const id8 = () => crypto.randomUUID().slice(0, 8)

async function world(tx: RealDbTx) {
  const userId = `trash-test-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@trash.test`}, true, false, now(), now())
  `)
  const [team] = await tx.insert(teams).values({ name: `Trash`, slug: `trash-${id8()}` }).returning({ id: teams.id })
  const teamId = team!.id
  await tx.insert(teamMembers).values({ teamId, userId, role: `owner` })
  const [board] = await tx
    .insert(boards)
    .values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `TR` })
    .returning({ id: boards.id })
  const issue = async (title: string) =>
    (await tx.insert(issues).values({ boardId: board!.id, teamId, title, creatorId: userId }).returning({ id: issues.id, identifier: issues.identifier }))[0]!
  const doomed = await issue(`Doomed`)
  const other = await issue(`Other`)
  const [comment] = await tx.insert(comments).values({ issueId: doomed.id, teamId, boardId: board!.id, authorId: userId, body: `hello` }).returning({ id: comments.id })
  // The trashed issue is the RELATED side: the relation hides too.
  await tx.insert(issueRelations).values({ issueId: other.id, relatedIssueId: doomed.id, type: `related`, teamId, boardId: board!.id })
  const actor = { userId, origin: { viaMcp: true, sessionId: null, agentId: null } }
  return { userId, teamId, boardId: board!.id, doomed, other, commentId: comment!.id, actor }
}

async function mirrors(tx: RealDbTx, w: Awaited<ReturnType<typeof world>>) {
  const [comment] = await tx.select({ at: comments.boardDeletedAt }).from(comments).where(eq(comments.id, w.commentId))
  const [relation] = await tx.select({ at: issueRelations.boardDeletedAt }).from(issueRelations).where(eq(issueRelations.issueId, w.other.id))
  return { comment: comment?.at ?? null, relation: relation?.at ?? null }
}

async function eventNames(tx: RealDbTx, teamId: string) {
  const rows = await tx.select({ name: events.name, payload: events.payload, emittedBy: events.emittedBy }).from(events).where(eq(events.teamId, teamId)).orderBy(events.createdAt)
  return rows.filter((row) => row.name.startsWith(`issue.`) && row.name !== `issue.created`)
}

describe.skipIf(!ready.ok)(`issue trash (real db)`, () => {
  it(`trash hides the issue and its children, restore brings them back intact`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)

      const trashed = await trashIssuesInTx(tx, [w.doomed.id], w.actor)
      expect(trashed.map((row) => row.id)).toEqual([w.doomed.id])
      const [row] = await tx.select().from(issues).where(eq(issues.id, w.doomed.id))
      expect(row!.deletedAt).not.toBeNull()
      expect(row!.deletedBy).toBe(w.userId)
      // The board mirror on the issue itself stays the board's.
      expect(row!.boardDeletedAt).toBeNull()
      let m = await mirrors(tx, w)
      expect(m.comment).not.toBeNull()
      expect(m.relation).not.toBeNull()
      // A child written while trashed is born hidden.
      const [late] = await tx.insert(comments).values({ issueId: w.doomed.id, teamId: w.teamId, boardId: w.boardId, authorId: w.userId, body: `late` }).returning({ at: comments.boardDeletedAt })
      expect(late!.at).not.toBeNull()
      // Trashing twice is a no-op.
      expect(await trashIssuesInTx(tx, [w.doomed.id], w.actor)).toEqual([])

      const listed = await listTrashedIssues(w.teamId, tx)
      expect(listed.map((issue) => issue.identifier)).toEqual([w.doomed.identifier])
      expect(listed[0]!.purgeAt.getTime() - listed[0]!.deletedAt.getTime()).toBe(ISSUE_TRASH_RETENTION_MS)

      const restored = await restoreIssueInTx(tx, w.doomed.id, w.actor)
      expect(restored?.identifier).toBe(w.doomed.identifier)
      const [back] = await tx.select().from(issues).where(eq(issues.id, w.doomed.id))
      expect(back!.deletedAt).toBeNull()
      expect(back!.deletedBy).toBeNull()
      m = await mirrors(tx, w)
      expect(m).toEqual({ comment: null, relation: null })
      expect(await tx.select({ id: comments.id }).from(comments).where(eq(comments.issueId, w.doomed.id))).toHaveLength(2)
      expect(await restoreIssueInTx(tx, w.doomed.id, w.actor)).toBeNull()
      expect(await listTrashedIssues(w.teamId, tx)).toEqual([])

      const named = await eventNames(tx, w.teamId)
      expect(named.map((e) => e.name)).toEqual([`issue.deleted`, `issue.restored`])
      expect(named[0]!.payload).toEqual({
        issueId: w.doomed.id,
        identifier: w.doomed.identifier,
        title: `Doomed`,
        boardId: w.boardId,
        actorUserId: w.userId,
        actorAgentId: null,
        sessionId: null,
        viaMcp: true,
      })
      expect(named[0]!.emittedBy).toBe(`user`)
    })
  })

  it(`a board restore keeps a trashed issue's children hidden`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      await trashIssuesInTx(tx, [w.doomed.id], w.actor)
      await tx.update(boards).set({ deletedAt: new Date() }).where(eq(boards.id, w.boardId))
      await tx.update(boards).set({ deletedAt: null }).where(eq(boards.id, w.boardId))
      const m = await mirrors(tx, w)
      expect(m.comment).not.toBeNull()
      expect(m.relation).not.toBeNull()
      // A trashed board's issues are not listed (the board's trash owns them).
      await tx.update(boards).set({ deletedAt: new Date() }).where(eq(boards.id, w.boardId))
      expect(await listTrashedIssues(w.teamId, tx)).toEqual([])
    })
  })

  it(`the sweep purges past the retention only; Delete forever purges now`, async () => {
    await inRollback(async (tx) => {
      const w = await world(tx)
      await trashIssuesInTx(tx, [w.doomed.id, w.other.id], w.actor)
      const now = new Date(`2100-01-01T00:00:00.000Z`)
      // Backdate one past the window, keep one inside it.
      await tx.update(issues).set({ deletedAt: new Date(now.getTime() - ISSUE_TRASH_RETENTION_MS) }).where(eq(issues.id, w.doomed.id))
      await tx.update(issues).set({ deletedAt: new Date(now.getTime() - ISSUE_TRASH_RETENTION_MS + 60_000) }).where(eq(issues.id, w.other.id))
      const reclaimed: string[] = []
      const purged = await runIssuePurgeSweep(now, { executor: tx, deleteKeys: async (keys) => void reclaimed.push(...keys) })
      expect(purged).toBe(1)
      const left = await tx.select({ id: issues.id }).from(issues).where(inArray(issues.id, [w.doomed.id, w.other.id]))
      expect(left.map((row) => row.id)).toEqual([w.other.id])
      expect(await tx.select({ id: comments.id }).from(comments).where(eq(comments.issueId, w.doomed.id))).toEqual([])

      const { purged: forever } = await purgeIssueInTx(tx, w.other.id, { actor: w.actor })
      expect(forever?.id).toBe(w.other.id)
      // Only a trashed issue purges.
      const live = (await tx.insert(issues).values({ boardId: w.boardId, teamId: w.teamId, title: `Live` }).returning({ id: issues.id }))[0]!
      expect((await purgeIssueInTx(tx, live.id, { actor: w.actor })).purged).toBeNull()

      const purges = (await eventNames(tx, w.teamId)).filter((e) => e.name === `issue.purged`)
      expect(purges.map((e) => [e.emittedBy, (e.payload as { actorUserId: string | null }).actorUserId])).toEqual([
        [`server`, null],
        [`user`, w.userId],
      ])
      expect(await tx.select({ id: issues.id }).from(issues).where(and(eq(issues.teamId, w.teamId), sql`deleted_at IS NOT NULL`))).toEqual([])
    })
  })
})
