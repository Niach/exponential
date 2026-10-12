// @vitest-environment node
import { describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import type { CodingSessionResult } from "@exp/db-schema/domain"
import { boards, codingSessions, issues, teams, threadItems, threads } from "@/db/schema"
import { inRollback, probeRealDb, type RealDbTx } from "@/test/real-db"
import {
  findIssueThread,
  getThread,
  getThreadOfSession,
  joinThreadForStart,
  linkIssueToThread,
  linkRunToThread,
  listThreadIssueIds,
  openThread,
  placeSessionAlone,
  unlinkIssueFromThread,
  unlinkRunFromThread,
} from "@/lib/threads"
import {
  lockRunGuide,
  ownsGuideEntry,
  stampGuideEntry,
  writeRunGuide,
} from "@/lib/thread-guide"
import { upsertSessionResultText } from "@/lib/session-result-writes"
import { pruneOrphanThreadsInTx } from "@/lib/session-attachment-sweep"

// EXP-1284 (review T16): lib/threads.ts, lib/thread-guide.ts and the thread
// triggers (review T3: run items mirror their session's board, a thread with
// no visible item hides, an emptied thread with an empty Guide is pruned)
// against the REAL migrated Postgres, every case rolled back. Skips without
// one (CI's unit job has none; see src/test/real-db.ts).

const ready = await probeRealDb(`
  SELECT (
    EXISTS (SELECT 1 FROM information_schema.columns
      WHERE table_name = 'threads' AND column_name = 'has_visible_items')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'refresh_thread_visibility')
    AND EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'propagate_session_board_to_thread_items')
  ) AS ok
`)
if (!ready.ok) console.info(`[threads.real-db] skipped: ${ready.reason}`)

const id8 = () => crypto.randomUUID().slice(0, 8)

interface World {
  tx: RealDbTx
  userId: string
  teamId: string
  boardA: string
  boardB: string
  issue: (boardId?: string) => Promise<string>
  session: (over?: Partial<typeof codingSessions.$inferInsert>) => Promise<string>
}

async function world(tx: RealDbTx): Promise<World> {
  const userId = `test-user-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@threads.test`}, true, false, now(), now())
  `)
  const [team] = await tx
    .insert(teams)
    .values({ name: `Threads`, slug: `threads-${id8()}` })
    .returning({ id: teams.id })
  const board = async (prefix: string) =>
    (
      await tx
        .insert(boards)
        .values({ teamId: team!.id, name: prefix, slug: `${prefix.toLowerCase()}-${id8()}`, prefix })
        .returning({ id: boards.id })
    )[0]!.id
  const boardA = await board(`TA`)
  const boardB = await board(`TB`)
  return {
    tx,
    userId,
    teamId: team!.id,
    boardA,
    boardB,
    issue: async (boardId = boardA) =>
      (
        await tx
          .insert(issues)
          .values({ boardId, teamId: team!.id, title: `Issue ${id8()}` })
          .returning({ id: issues.id })
      )[0]!.id,
    session: async (over = {}) =>
      (
        await tx
          .insert(codingSessions)
          .values({ teamId: team!.id, userId, status: `running`, ...over })
          .returning({ id: codingSessions.id })
      )[0]!.id,
  }
}

const text = (topic: string, body: string, extra: Partial<CodingSessionResult> = {}) =>
  ({
    topic,
    label: null,
    attachmentId: null,
    width: null,
    height: null,
    text: body,
    ...extra,
  }) as CodingSessionResult

async function threadRow(tx: RealDbTx, id: string) {
  const [row] = await tx.select().from(threads).where(eq(threads.id, id))
  return row ?? null
}

async function itemOf(tx: RealDbTx, sessionId: string) {
  const [row] = await tx.select().from(threadItems).where(eq(threadItems.sessionId, sessionId))
  return row ?? null
}

describe.skipIf(!ready.ok)(`lib/threads.ts on Postgres (EXP-1284)`, () => {
  it(`an issue start opens a thread holding the issue; the next start joins it while it is ongoing`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const issueId = await w.issue()
      const s1 = await w.session({ issueId })
      const t1 = await joinThreadForStart(tx, {
        session: { id: s1, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      expect(await listThreadIssueIds(tx, t1.id)).toEqual([issueId])
      expect((await getThreadOfSession(tx, s1))?.id).toBe(t1.id)

      // Ended without a merged PR: still the issue's open thread.
      await tx.update(codingSessions).set({ status: `ended` }).where(eq(codingSessions.id, s1))
      const s2 = await w.session({ issueId })
      const t2 = await joinThreadForStart(tx, {
        session: { id: s2, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      expect(t2.id).toBe(t1.id)
    }))

  // Review T1: a finished thread (its PR merged, nothing live) is never
  // joined again: the next start opens a fresh one.
  it(`a start after the thread's PR merged opens a fresh thread; a live session keeps it open`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const issueId = await w.issue()
      const s1 = await w.session({ issueId })
      const t1 = await joinThreadForStart(tx, {
        session: { id: s1, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      await tx
        .update(codingSessions)
        .set({ prUrl: `https://github.com/o/r/pull/10`, prState: `merged` })
        .where(eq(codingSessions.id, s1))
      // s1 still live: the thread stays open.
      expect((await findIssueThread(tx, issueId))?.id).toBe(t1.id)

      await tx.update(codingSessions).set({ status: `ended` }).where(eq(codingSessions.id, s1))
      expect(await findIssueThread(tx, issueId)).toBeNull()
      const s2 = await w.session({ issueId })
      const t2 = await joinThreadForStart(tx, {
        session: { id: s2, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      expect(t2.id).not.toBe(t1.id)
      expect(await listThreadIssueIds(tx, t2.id)).toEqual([issueId])

      // The issue's own merged PR closes its thread too.
      const other = await w.issue()
      const s3 = await w.session({ issueId: other })
      const t3 = await joinThreadForStart(tx, {
        session: { id: s3, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId: other },
      })
      await tx.update(codingSessions).set({ status: `ended` }).where(eq(codingSessions.id, s3))
      await tx.update(issues).set({ prState: `merged` }).where(eq(issues.id, other))
      expect(await findIssueThread(tx, other)).toBeNull()
      expect(t3.id).toBeTruthy()
    }))

  it(`a requested thread is joined with the start's issues; a resume stays with its predecessor`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const [i1, i2] = [await w.issue(), await w.issue()]
      const requested = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
      const s1 = await w.session({ batchIssueIds: [i1, i2] })
      const placed = await joinThreadForStart(tx, {
        session: { id: s1, teamId: w.teamId, userId: w.userId },
        subject: { kind: `batch`, issueIds: [i1, i2] },
        requestedThreadId: requested.id,
      })
      expect(placed.id).toBe(requested.id)
      expect(await listThreadIssueIds(tx, requested.id)).toEqual([i1, i2])

      const s2 = await w.session({ resumedFromId: s1 })
      const resumed = await joinThreadForStart(tx, {
        session: { id: s2, teamId: w.teamId, userId: w.userId },
        subject: { kind: `chat` },
        predecessorId: s1,
        requestedThreadId: (await openThread(tx, { teamId: w.teamId, createdBy: null })).id,
      })
      expect(resumed.id).toBe(requested.id)
    }))

  it(`a vanished, foreign or hidden requested thread falls through to the default`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const other = await world(tx)
      const foreign = await openThread(tx, { teamId: other.teamId, createdBy: null })
      const s1 = await w.session()
      const t1 = await joinThreadForStart(tx, {
        session: { id: s1, teamId: w.teamId, userId: w.userId },
        subject: { kind: `chat` },
        requestedThreadId: foreign.id,
      })
      expect(t1.id).not.toBe(foreign.id)

      // Hidden: its only item sits on a trashed board.
      const hidden = await openThread(tx, { teamId: w.teamId, createdBy: null })
      await linkIssueToThread(tx, hidden, await w.issue(w.boardB), {
        userId: w.userId,
        source: `user`,
      })
      await tx.update(boards).set({ deletedAt: new Date() }).where(eq(boards.id, w.boardB))
      expect(await getThread(tx, hidden.id)).toBeNull()
      const s2 = await w.session()
      const t2 = await joinThreadForStart(tx, {
        session: { id: s2, teamId: w.teamId, userId: w.userId },
        subject: { kind: `chat` },
        requestedThreadId: hidden.id,
      })
      expect(t2.id).not.toBe(hidden.id)
    }))

  // Review T2: a moved session takes ITS Guide entries along; the source
  // keeps the others, and is pruned only once it has no item AND no Guide.
  it(`moving a session carries its own Guide entries and prunes an emptied source`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const actor = { userId: w.userId, source: `user` as const }
      const [s1, s2] = [await w.session(), await w.session()]
      const a = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
      await linkRunToThread(tx, a, { id: s1, teamId: w.teamId }, actor)
      await linkRunToThread(tx, a, { id: s2, teamId: w.teamId }, actor)
      await tx
        .update(threads)
        .set({
          results: [
            text(`Summary`, `legacy, the first session's`),
            text(`Summary`, `s2's`, { sessionId: s2 }),
            text(`Notes`, `s1's`, { sessionId: s1 }),
          ],
        })
        .where(eq(threads.id, a.id))
      const b = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
      await linkRunToThread(tx, b, { id: s1, teamId: w.teamId }, actor)

      // s1 was a's FIRST session: the legacy entry goes with it, stamped.
      expect((await threadRow(tx, a.id))!.results).toEqual([text(`Summary`, `s2's`, { sessionId: s2 })])
      expect((await threadRow(tx, b.id))!.results).toEqual([
        text(`Summary`, `legacy, the first session's`, { sessionId: s1 }),
        text(`Notes`, `s1's`, { sessionId: s1 }),
      ])
      const [mirror] = await tx
        .select({ results: codingSessions.results })
        .from(codingSessions)
        .where(eq(codingSessions.id, s1))
      expect(mirror!.results).toEqual((await threadRow(tx, b.id))!.results)

      // s2 leaves too: a has no item; its Guide still holds nothing of anyone
      // else's (s2 took its own), so it is pruned.
      await linkRunToThread(tx, b, { id: s2, teamId: w.teamId }, actor)
      expect(await threadRow(tx, a.id)).toBeNull()
    }))

  it(`a source left with no item but another session's Guide entries stays, hidden`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const actor = { userId: w.userId, source: `user` as const }
      const s1 = await w.session()
      const a = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
      await linkRunToThread(tx, a, { id: s1, teamId: w.teamId }, actor)
      const gone = crypto.randomUUID()
      await tx
        .update(threads)
        .set({ results: [text(`Summary`, `a deleted session's`, { sessionId: gone })] })
        .where(eq(threads.id, a.id))
      const { newThreadId } = await unlinkRunFromThread(tx, a, { id: s1, teamId: w.teamId }, actor)
      expect(newThreadId).toBeTruthy()
      const kept = await threadRow(tx, a.id)
      expect(kept?.hasVisibleItems).toBe(false)
      expect(await getThread(tx, a.id)).toBeNull()
    }))

  it(`placeSessionAlone gives a threadless session a thread of its own, once`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const issueId = await w.issue()
      const s1 = await w.session({ issueId })
      const t1 = await placeSessionAlone(tx, { id: s1, teamId: w.teamId, userId: w.userId }, [issueId])
      expect(await listThreadIssueIds(tx, t1.id)).toEqual([issueId])
      const again = await placeSessionAlone(tx, { id: s1, teamId: w.teamId, userId: w.userId }, [])
      expect(again.id).toBe(t1.id)
    }))

  it(`refuses another team's session and issue`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const other = await world(tx)
      const t = await openThread(tx, { teamId: w.teamId, createdBy: null })
      const actor = { userId: w.userId, source: `user` as const }
      await expect(
        linkRunToThread(tx, t, { id: await other.session(), teamId: other.teamId }, actor)
      ).rejects.toThrow(`That session belongs to another team`)
      await expect(linkIssueToThread(tx, t, await other.issue(), actor)).rejects.toThrow()
    }))
})

describe.skipIf(!ready.ok)(`thread triggers on Postgres (EXP-1284 review T3)`, () => {
  it(`a run item mirrors its session's board context, and follows a move`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const issueId = await w.issue()
      const s1 = await w.session({ issueId })
      const chat = await w.session()
      const t = await openThread(tx, { teamId: w.teamId, createdBy: null })
      const actor = { userId: w.userId, source: `user` as const }
      await linkRunToThread(tx, t, { id: s1, teamId: w.teamId }, actor)
      await linkRunToThread(tx, t, { id: chat, teamId: w.teamId }, actor)
      expect((await itemOf(tx, s1))?.boardId).toBe(w.boardA)
      expect((await itemOf(tx, chat))?.boardId).toBeNull()

      // issues.move: the issue first, then the re-point of its sessions
      // (their populate trigger re-derives from the moved issue); the run
      // item follows its session.
      await tx.update(issues).set({ boardId: w.boardB }).where(eq(issues.id, issueId))
      await tx.update(codingSessions).set({ boardId: w.boardB }).where(eq(codingSessions.issueId, issueId))
      expect((await itemOf(tx, s1))?.boardId).toBe(w.boardB)
    }))

  it(`a thread hides while every item is on a trashed or archived board, without bumping its activity`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const issueId = await w.issue(w.boardA)
      const s1 = await w.session({ issueId })
      const t = await joinThreadForStart(tx, {
        session: { id: s1, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      const before = (await threadRow(tx, t.id))!.updatedAt

      await tx.update(boards).set({ deletedAt: new Date() }).where(eq(boards.id, w.boardA))
      expect((await itemOf(tx, s1))?.boardDeletedAt).not.toBeNull()
      expect((await threadRow(tx, t.id))!.hasVisibleItems).toBe(false)
      expect(await getThread(tx, t.id)).toBeNull()
      expect((await threadRow(tx, t.id))!.updatedAt).toEqual(before)

      await tx.update(boards).set({ deletedAt: null }).where(eq(boards.id, w.boardA))
      expect((await getThread(tx, t.id))?.id).toBe(t.id)

      await tx.update(boards).set({ archivedAt: new Date() }).where(eq(boards.id, w.boardA))
      expect(await getThread(tx, t.id)).toBeNull()

      // One visible item anywhere keeps it.
      await linkRunToThread(tx, t, { id: await w.session(), teamId: w.teamId }, {
        userId: w.userId,
        source: `user`,
      })
      expect((await getThread(tx, t.id))?.id).toBe(t.id)
    }))

  it(`the last item going prunes a thread with an empty Guide, keeps (hidden) one with entries`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const actor = { userId: w.userId, source: `user` as const }
      const empty = await openThread(tx, { teamId: w.teamId, createdBy: null })
      const issueId = await w.issue()
      await linkIssueToThread(tx, empty, issueId, actor)
      await unlinkIssueFromThread(tx, empty.id, issueId)
      expect(await threadRow(tx, empty.id)).toBeNull()

      // Issue delete and session delete (sweep, account deletion) cascade.
      const withGuide = await openThread(tx, { teamId: w.teamId, createdBy: null })
      const s1 = await w.session()
      const doomed = await w.issue()
      await linkRunToThread(tx, withGuide, { id: s1, teamId: w.teamId }, actor)
      await linkIssueToThread(tx, withGuide, doomed, actor)
      await tx
        .update(threads)
        .set({ results: [text(`Summary`, `kept`, { sessionId: s1 })] })
        .where(eq(threads.id, withGuide.id))
      await tx.delete(issues).where(eq(issues.id, doomed))
      expect((await threadRow(tx, withGuide.id))!.hasVisibleItems).toBe(true)
      await tx.delete(codingSessions).where(eq(codingSessions.id, s1))
      const kept = await threadRow(tx, withGuide.id)
      expect(kept).not.toBeNull()
      expect(kept!.hasVisibleItems).toBe(false)

      const plain = await openThread(tx, { teamId: w.teamId, createdBy: null })
      const s2 = await w.session()
      await linkRunToThread(tx, plain, { id: s2, teamId: w.teamId }, actor)
      await tx.delete(codingSessions).where(eq(codingSessions.id, s2))
      expect(await threadRow(tx, plain.id)).toBeNull()
    }))

  it(`the sweep drops a kept Guide-only thread a week after its last activity, never one with items`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const actor = { userId: w.userId, source: `user` as const }
      const orphan = await openThread(tx, { teamId: w.teamId, createdBy: null })
      const s1 = await w.session()
      await linkRunToThread(tx, orphan, { id: s1, teamId: w.teamId }, actor)
      await tx
        .update(threads)
        .set({ results: [text(`Summary`, `orphaned`, { sessionId: s1 })] })
        .where(eq(threads.id, orphan.id))
      await tx.delete(codingSessions).where(eq(codingSessions.id, s1))
      const live = await openThread(tx, { teamId: w.teamId, createdBy: null })
      await linkIssueToThread(tx, live, await w.issue(), actor)
      const old = new Date(Date.now() - 8 * 24 * 60 * 60 * 1000)
      await tx.execute(sql`SET LOCAL exponential.preserve_timestamps = 'on'`)
      await tx.update(threads).set({ updatedAt: old }).where(eq(threads.id, orphan.id))
      await tx.update(threads).set({ updatedAt: old }).where(eq(threads.id, live.id))

      // Inside the week: kept.
      await pruneOrphanThreadsInTx(tx, new Date(old.getTime() - 1000))
      expect(await threadRow(tx, orphan.id)).not.toBeNull()
      // Past it: the item-less thread goes, the one holding an issue stays.
      expect(
        await pruneOrphanThreadsInTx(tx, new Date(Date.now() - 7 * 24 * 60 * 60 * 1000))
      ).toBeGreaterThanOrEqual(1)
      expect(await threadRow(tx, orphan.id)).toBeNull()
      expect(await threadRow(tx, live.id)).not.toBeNull()
    }))

  it(`a thread never given an item stays visible (just created)`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const t = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
      expect((await getThread(tx, t.id))?.id).toBe(t.id)
    }))
})

describe.skipIf(!ready.ok)(`lib/thread-guide.ts on Postgres (EXP-1284)`, () => {
  it(`locks the session then its thread and writes both; a later session files beside, never over`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const issueId = await w.issue()
      const s1 = await w.session({ issueId })
      const t = await joinThreadForStart(tx, {
        session: { id: s1, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      const write = async (sessionId: string, topic: string, body: string) => {
        const lock = (await lockRunGuide(tx, sessionId))!
        const next = upsertSessionResultText(lock.results, topic, body, undefined, {
          sessionId,
          owns: (row) => ownsGuideEntry(lock.owner, row),
        })!
        await writeRunGuide(tx, lock, next)
        return lock
      }
      const lock1 = await write(s1, `Summary`, `first`)
      expect(lock1.threadId).toBe(t.id)
      expect(lock1.owner).toMatchObject({ sessionId: s1, chainIds: [s1], ownsLegacy: true })

      const s2 = await w.session({ issueId })
      await joinThreadForStart(tx, {
        session: { id: s2, teamId: w.teamId, userId: w.userId },
        subject: { kind: `issue`, issueId },
      })
      const lock2 = await write(s2, `Summary`, `second`)
      expect(lock2.owner.ownsLegacy).toBe(false)
      await write(s1, `Summary`, `first, revised`)

      const guide = (await threadRow(tx, t.id))!.results as CodingSessionResult[]
      expect(guide.map((row) => [row.sessionId, row.text])).toEqual([
        [s1, `first, revised`],
        [s2, `second`],
      ])
      const [mirror] = await tx
        .select({ results: codingSessions.results })
        .from(codingSessions)
        .where(eq(codingSessions.id, s1))
      expect(mirror!.results).toEqual(guide)
    }))

  it(`a resume owns its chain's entries and the thread's legacy ones; a PR's entries belong to whoever holds it`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const s1 = await w.session({ status: `ended` })
      const t = await placeSessionAlone(tx, { id: s1, teamId: w.teamId, userId: w.userId }, [])
      const s2 = await w.session({ resumedFromId: s1, prUrl: `https://github.com/o/r/pull/5` })
      await joinThreadForStart(tx, {
        session: { id: s2, teamId: w.teamId, userId: w.userId },
        subject: { kind: `chat` },
        predecessorId: s1,
      })
      const lock = (await lockRunGuide(tx, s2))!
      expect(lock.threadId).toBe(t.id)
      expect(lock.owner.chainIds).toEqual([s2, s1])
      expect(lock.owner.ownsLegacy).toBe(true)
      const owns = (row: CodingSessionResult) => ownsGuideEntry(lock.owner, row)
      expect(owns(text(`A`, `x`, { sessionId: s1 }))).toBe(true)
      expect(owns(text(`A`, `x`))).toBe(true)
      expect(owns(text(`A`, `x`, { sessionId: crypto.randomUUID() }))).toBe(false)
      expect(
        owns(text(`A`, `x`, { sessionId: crypto.randomUUID(), prUrl: `https://github.com/o/r/pull/5` }))
      ).toBe(true)
      expect(stampGuideEntry(lock, text(`A`, `x`)).sessionId).toBe(s2)
    }))

  it(`a session with no thread writes its own row`, () =>
    inRollback(async (tx) => {
      const w = await world(tx)
      const s1 = await w.session()
      const lock = (await lockRunGuide(tx, s1))!
      expect(lock.threadId).toBeNull()
      expect(lock.owner.ownsLegacy).toBe(true)
      await writeRunGuide(tx, lock, [text(`Summary`, `own`)])
      const [row] = await tx
        .select({ results: codingSessions.results })
        .from(codingSessions)
        .where(eq(codingSessions.id, s1))
      expect(row!.results).toEqual([text(`Summary`, `own`)])
      expect(await lockRunGuide(tx, crypto.randomUUID())).toBeNull()
    }))
})

// Review T16 (recheck): two `steer.startSession` starts of the SAME issue on
// one device, each into its own thread, land in their own threads however
// the device's starts arrive. The router itself reads through the global db
// (membership lookups), which cannot see this rolled-back transaction's
// rows, so this drives what `codingSessions.start` does — take the
// remembered thread (by the echoed startId, else oldest first), then
// `joinThreadForStart` with it — against the real database.
describe.skipIf(!ready.ok)(`two pending starts of one issue on one device (EXP-1284 T16)`, () => {
  const land = async (
    w: World,
    issueId: string,
    startId: string | undefined
  ): Promise<{ session: string; thread: string }> => {
    const { takeStartThread } = await import(`@/lib/start-threads`)
    const session = await w.session({ issueId })
    const requested = takeStartThread({
      ownerId: w.userId,
      deviceId: `mac-1`,
      subject: { issueId },
      startId,
    })
    const thread = await joinThreadForStart(w.tx, {
      session: { id: session, teamId: w.teamId, userId: w.userId },
      subject: { kind: `issue`, issueId },
      requestedThreadId: requested,
    })
    return { session, thread: thread.id }
  }

  const setUp = async (tx: RealDbTx) => {
    const { _clearStartThreads, rememberStartThread } = await import(`@/lib/start-threads`)
    _clearStartThreads()
    const w = await world(tx)
    const issueId = await w.issue()
    const t1 = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
    const t2 = await openThread(tx, { teamId: w.teamId, createdBy: w.userId })
    for (const [startId, thread] of [
      [`start-1`, t1.id],
      [`start-2`, t2.id],
    ] as const) {
      rememberStartThread({
        ownerId: w.userId,
        deviceId: `mac-1`,
        subject: { issueId },
        threadId: thread,
        startId,
      })
    }
    return { w, issueId, t1: t1.id, t2: t2.id }
  }

  it(`echoed startIds landing in reverse order each take their own thread`, () =>
    inRollback(async (tx) => {
      const { w, issueId, t1, t2 } = await setUp(tx)
      const second = await land(w, issueId, `start-2`)
      const first = await land(w, issueId, `start-1`)
      expect(second.thread).toBe(t2)
      expect(first.thread).toBe(t1)
      expect((await getThreadOfSession(tx, second.session))?.id).toBe(t2)
      expect((await getThreadOfSession(tx, first.session))?.id).toBe(t1)
      expect(await listThreadIssueIds(tx, t1)).toEqual([issueId])
      expect(await listThreadIssueIds(tx, t2)).toEqual([issueId])
    }))

  it(`without startIds (older daemons) they land oldest first, one per thread`, () =>
    inRollback(async (tx) => {
      const { w, issueId, t1, t2 } = await setUp(tx)
      const first = await land(w, issueId, undefined)
      const second = await land(w, issueId, undefined)
      expect([first.thread, second.thread]).toEqual([t1, t2])
    }))
})
