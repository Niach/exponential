// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import { eq, sql } from "drizzle-orm"
import { attachments, boards, issues, teamMembers, teams } from "@/db/schema"
import { inRollback, probeRealDb } from "@/test/real-db"

// #EXP-2115: Settings → Storage "Sweep unreferenced images" against the real
// migrated Postgres: one old image per kind of reference (sealed chats, sealed
// cards, results, team and agent icons included) plus one nothing names.

vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/team-membership`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/team-membership")>()),
  assertTeamOwner: async () => undefined,
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'populate_message_session_mirrors') AS ok
`)
if (!ready.ok) console.info(`[attachments-sweep.real-db] skipped: ${ready.reason}`)

const { purgeSweptImagesInTx, SWEPT_IMAGE_RETENTION_MS } = await import(`@/lib/swept-images`)
const { attachmentsRouter } = await import(`@/lib/trpc/attachments`)
const id8 = () => crypto.randomUUID().slice(0, 8)

describe.skipIf(!ready.ok)(`attachments.sweepUnreferencedImages on real rows`, () => {
  it(`deletes only the issue image nothing names`, async () => {
    await inRollback(async (tx) => {
      const userId = `sweep-test-${id8()}`
      await tx.execute(sql`
        INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
        VALUES (${userId}, 'Tester', ${`${userId}@sweep.test`}, true, false, now(), now())`)
      const [team] = await tx.insert(teams).values({ name: `Sweep`, slug: `sweep-${id8()}` }).returning({ id: teams.id })
      const teamId = team!.id
      await tx.insert(teamMembers).values({ teamId, userId, role: `owner` })
      const [board] = await tx.insert(boards).values({ teamId, name: `Core`, slug: `core-${id8()}`, prefix: `SW` }).returning({ id: boards.id })
      const issue = async () =>
        (await tx.insert(issues).values({ boardId: board!.id, teamId, title: `I`, creatorId: userId }).returning({ id: issues.id }))[0]!.id
      const plainIssue = await issue()
      const sealedIssue = await issue()
      const refIssue = await issue()
      const session = async (sealed: boolean) => {
        const id = crypto.randomUUID()
        await tx.execute(sql`
          INSERT INTO sessions (id, team_id, root_id, user_id, device_id, seal_public_key, seal_epoch)
          VALUES (${id}, ${teamId}, ${id}, ${userId}, 'dev', ${sealed ? `A`.repeat(43) : null}, ${sealed ? 1 : null})`)
        return id
      }
      const clear = await session(false)
      const sealed = await session(true)
      const message = async (rootId: string, values: { body?: string; card?: unknown; attachmentIds?: string[]; refs?: string[] }) => {
        const conv = await tx.execute<{ id: string }>(sql`SELECT id FROM conversations WHERE session_id = ${rootId}`)
        const list = (ids?: string[]) => (ids ? `{${ids.join(`,`)}}` : null)
        const row = await tx.execute<{ id: string }>(sql`
          INSERT INTO messages (root_id, team_id, conversation_id, author_kind, author_session_id, body, card, attachment_ids, ref_issue_ids)
          VALUES (${rootId}, ${teamId}, ${conv.rows[0]!.id}, 'agent', ${rootId}, ${values.body ?? ``},
            ${values.card ? JSON.stringify(values.card) : null}::jsonb, ${list(values.attachmentIds)}::uuid[], ${list(values.refs)}::uuid[])
          RETURNING id`)
        return row.rows[0]!.id
      }
      const image = async (kind: string, owner: Partial<typeof attachments.$inferInsert> = { issueId: plainIssue }) => {
        const id = crypto.randomUUID()
        await tx.insert(attachments).values({
          id, teamId, uploaderId: userId, filename: `${kind}.png`, contentType: `image/png`, sizeBytes: 1,
          storageKey: kind, url: `/api/attachments/${id}`, createdAt: new Date(Date.now() - 3 * 864e5), ...owner,
        })
        return id
      }
      const url = (id: string) => `![x](/api/attachments/${id})`

      const stale = await image(`stale`)
      // Owner-less and foreign-owned rows.
      const guide = await image(`guide`, {})
      await tx.execute(sql`UPDATE sessions SET guide = ${JSON.stringify([{ attachmentId: guide }])}::jsonb WHERE id = ${clear}`)
      const icon = await image(`icon`, {})
      await tx.update(teams).set({ iconUrl: `/api/attachments/${icon}` }).where(eq(teams.id, teamId))
      await image(`unsent`, {})
      await image(`chat`, { messageId: await message(sealed, { body: `exp-sealed:v1:1:xxxx` }) })
      const sealedPicture = await image(`sealed-card`, { messageId: await message(sealed, { body: `` }) })
      await message(sealed, { card: { kind: `surface`, pictures: [{ label: `web`, attachmentId: sealedPicture }], sealed: `x` } })
      await image(`result`, { issueId: plainIssue, sessionId: clear })
      // Issue images something else names.
      await tx.update(issues).set({ description: url(await image(`description`)) }).where(eq(issues.id, plainIssue))
      await message(clear, { body: url(await image(`chat-body`)) })
      await message(clear, { card: { kind: `plan`, plan: url(await image(`card`)) } })
      await message(clear, { attachmentIds: [await image(`attachment-ids`)] })
      const inGuide = await image(`guide-text`)
      await tx.execute(sql`UPDATE sessions SET guide = guide || ${JSON.stringify([{ text: url(inGuide) }])}::jsonb WHERE id = ${clear}`)
      await tx.execute(sql`INSERT INTO prompts (team_id, name, body) VALUES (${teamId}, 'P', ${url(await image(`prompt`))})`)
      await tx.execute(sql`INSERT INTO agents (team_id, owner_id, name, instructions) VALUES (${teamId}, ${userId}, 'A', ${url(await image(`agent`))})`)
      // Sealed content: judged by the issue it is clearly about.
      await tx.execute(sql`INSERT INTO session_issues (session_id, issue_id, team_id) VALUES (${sealed}, ${sealedIssue}, ${teamId})`)
      await image(`sealed-linked`, { issueId: sealedIssue })
      await message(sealed, { body: `exp-sealed:v1:1:yyyy`, refs: [refIssue] })
      await image(`sealed-ref`, { issueId: refIssue })

      const caller = attachmentsRouter.createCaller({
        session: { user: { id: userId } },
        db: tx,
        request: new Request(`http://localhost:5173/api/trpc/x`),
      } as never)
      const list = await caller.listForTeam({ teamId })
      expect(list.attachments.filter((row) => row.isImage && !row.referenced).map((row) => row.filename)).toEqual([`stale.png`])
      const result = await caller.sweepUnreferencedImages({ teamId })
      expect(result.deletedCount).toBe(1)
      const marked = async () =>
        (await tx.execute<{ filename: string }>(sql`SELECT filename FROM attachments WHERE team_id = ${teamId} AND swept_at IS NOT NULL`)).rows.map((row) => row.filename)
      expect(await marked()).toEqual([`stale.png`])
      expect((await caller.listForTeam({ teamId })).attachments.map((row) => row.filename)).not.toContain(`stale.png`)

      // Inside the window nothing goes; past it, a reference that came back
      // clears the mark instead.
      const later = new Date(Date.now() + 1000)
      expect(await purgeSweptImagesInTx(tx, new Date(Date.now() - SWEPT_IMAGE_RETENTION_MS))).toEqual({ storageKeys: [] })
      await message(clear, { body: url(stale) })
      expect(await purgeSweptImagesInTx(tx, later)).toEqual({ storageKeys: [] })
      expect(await marked()).toEqual([])
      await tx.execute(sql`DELETE FROM messages WHERE body LIKE ${`%${stale}%`}`)
      await caller.sweepUnreferencedImages({ teamId })
      expect(await purgeSweptImagesInTx(tx, later)).toEqual({ storageKeys: [`stale`] })
    })
  })
})
