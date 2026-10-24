// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { eq, sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { boards, issues, notifications, teamMembers, teams } from "@/db/schema"
import { probeRealDb } from "@/test/real-db"

// Chats (fixtures/chats.json shell: phones show ONE active team):
// `notifications.markAllRead({teamId})` clears only that team's rows (an
// issue row's team = its issue's, an issue-less row's = `team_id`); the
// input-less call (the desktop) still clears every row. Against the REAL
// Postgres: the team predicate is SQL a fake DB would not check.

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM information_schema.columns
  WHERE table_name = 'notifications' AND column_name = 'team_id') AS ok`)
if (!ready.ok) console.info(`[notifications.real-db] skipped: ${ready.reason}`)

const { notificationsRouter } = await import(`@/lib/trpc/notifications`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

async function user(): Promise<string> {
  const id = `notif-test-${id8()}`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${`${id}@notifications.test`}, true, false, now(), now())
  `)
  made.users.push(id)
  return id
}

async function team(userId: string) {
  const [row] = await db
    .insert(teams)
    .values({ name: `Notif`, slug: `notif-${id8()}` })
    .returning({ id: teams.id })
  made.teams.push(row!.id)
  await db.insert(teamMembers).values({ teamId: row!.id, userId, role: `member` })
  const [board] = await db
    .insert(boards)
    .values({ teamId: row!.id, name: `Core`, slug: `core-${id8()}`, prefix: `NT` })
    .returning({ id: boards.id })
  const [issue] = await db
    .insert(issues)
    .values({ boardId: board!.id, teamId: row!.id, title: `An issue`, creatorId: userId })
    .returning({ id: issues.id })
  return { teamId: row!.id, issueId: issue!.id }
}

afterAll(async () => {
  if (!ready.ok) return
  for (const id of made.teams) await db.delete(teams).where(eq(teams.id, id))
  if (made.users.length) {
    await db.execute(sql`DELETE FROM users WHERE id IN ${sql.raw(`('${made.users.join(`','`)}')`)}`)
  }
})

describe.skipIf(!ready.ok)(`notifications.markAllRead (Chats: one team)`, () => {
  const caller = (userId: string) =>
    notificationsRouter.createCaller({ session: { user: { id: userId } }, db } as never)

  it(`clears only the named team's rows; input-less clears all; a non-member is refused`, async () => {
    const me = await user()
    const a = await team(me)
    const b = await team(me)
    const insert = async (values: { issueId?: string; teamId?: string }) =>
      (
        await db
          .insert(notifications)
          .values({ userId: me, type: values.issueId ? `issue_assigned` : `agent_message`, title: `t`, ...values })
          .returning({ id: notifications.id })
      )[0]!.id
    const aIssue = await insert({ issueId: a.issueId })
    const aTeam = await insert({ teamId: a.teamId })
    const bIssue = await insert({ issueId: b.issueId })
    const bTeam = await insert({ teamId: b.teamId })
    const read = async (id: string) =>
      (await db.select({ readAt: notifications.readAt }).from(notifications).where(eq(notifications.id, id)))[0]!
        .readAt !== null

    await caller(me).markAllRead({ teamId: a.teamId })
    expect([await read(aIssue), await read(aTeam), await read(bIssue), await read(bTeam)]).toEqual([
      true,
      true,
      false,
      false,
    ])

    const stranger = await user()
    await expect(caller(stranger).markAllRead({ teamId: b.teamId })).rejects.toMatchObject({ code: `FORBIDDEN` })

    await caller(me).markAllRead()
    expect([await read(bIssue), await read(bTeam)]).toEqual([true, true])
  })
})
