// @vitest-environment node
import { afterAll, describe, expect, it, vi } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { probeRealDb } from "@/test/real-db"

// EXP-1458 against the real migrated Postgres: a SEALED conversation
// message notifies the members its `mention_user_ids` lists with generic
// copy (never a preview, never the envelope); a plaintext one parses its
// `@email`s and honours `mention_user_ids` beside them.

const pushes = vi.hoisted(() => [] as { title: string; body?: string }[])
vi.mock(`@/lib/integrations/fcm`, () => ({
  sendToUsers: async (_targets: unknown, payload: { title: string; body?: string }) => {
    pushes.push(payload)
  },
}))

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'messages' AND column_name = 'mention_user_ids') AS ok
`)
if (!ready.ok) console.info(`[notifications-sealed.real-db] skipped: ${ready.reason}`)

const { notifySessionMessage, SEALED_MESSAGE_NOTIFY_BODY, addsMentionUserIds } = await import(
  `@/lib/integrations/notifications`
)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }
const sealedBody = `exp-sealed:v1:1:${`A`.repeat(80)}`

afterAll(async () => {
  if (!ready.ok) return
  for (const id of made.teams) await db.execute(sql`DELETE FROM teams WHERE id = ${id}`)
  for (const id of made.users) await db.execute(sql`DELETE FROM users WHERE id = ${id}`)
})

async function user(teamId: string): Promise<{ id: string; email: string }> {
  const id = `sealed-notify-${id8()}`
  const email = `${id}@sealed.test`
  await db.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${id}, 'Tester', ${email}, true, false, now(), now())`)
  made.users.push(id)
  await db.execute(sql`INSERT INTO team_members (team_id, user_id, role) VALUES (${teamId}, ${id}, 'member')`)
  return { id, email }
}

async function sharedThread() {
  const team = await db.execute<{ id: string }>(sql`
    INSERT INTO teams (name, slug) VALUES ('Sealed', ${`sealed-${id8()}`}) RETURNING id`)
  const teamId = team.rows[0]!.id
  made.teams.push(teamId)
  const [starter, ada, bob] = [await user(teamId), await user(teamId), await user(teamId)]
  const sessionId = crypto.randomUUID()
  await db.execute(sql`
    INSERT INTO sessions (id, team_id, root_id, user_id, device_id, shared, title)
    VALUES (${sessionId}, ${teamId}, ${sessionId}, ${starter.id}, 'dev', true, 'Fix the login')`)
  const conv = await db.execute<{ id: string }>(sql`SELECT id FROM conversations WHERE session_id = ${sessionId}`)
  const post = async (body: string, mentionUserIds: string[] | null) => {
    const row = await db.execute<{ id: string }>(sql`
      INSERT INTO messages (root_id, team_id, conversation_id, author_kind, author_id, body, mention_user_ids)
      VALUES (${sessionId}, ${teamId}, ${conv.rows[0]!.id}, 'user', ${starter.id}, ${body},
        ${mentionUserIds ? sql`ARRAY[${sql.join(mentionUserIds.map((id) => sql`${id}`), sql`, `)}]::text[]` : null})
      RETURNING id`)
    return row.rows[0]!.id
  }
  const inbox = async (userId: string) =>
    (
      await db.execute<{ body: string | null }>(sql`
        SELECT body FROM notifications WHERE user_id = ${userId} AND session_id = ${sessionId}`)
    ).rows.map((r) => r.body)
  return { starter, ada, bob, post, inbox }
}

describe.skipIf(!ready.ok)(`EXP-1458: session message notifications`, () => {
  it(`a sealed message notifies its mention_user_ids with generic copy`, async () => {
    const t = await sharedThread()
    pushes.length = 0
    await notifySessionMessage(await t.post(sealedBody, [t.ada.id]))
    expect(await t.inbox(t.ada.id)).toEqual([SEALED_MESSAGE_NOTIFY_BODY])
    expect(await t.inbox(t.bob.id)).toEqual([])
    expect(pushes.map((p) => p.body)).toEqual([SEALED_MESSAGE_NOTIFY_BODY])
    expect(JSON.stringify(pushes)).not.toContain(`exp-sealed:`)
  })

  it(`a sealed message without mention_user_ids notifies nobody`, async () => {
    const t = await sharedThread()
    await notifySessionMessage(await t.post(sealedBody, null))
    expect(await t.inbox(t.ada.id)).toEqual([])
  })

  it(`a plaintext message: parsed @emails plus mention_user_ids, previewed`, async () => {
    const t = await sharedThread()
    const body = `Ping @${t.ada.email} about it`
    await notifySessionMessage(await t.post(body, [t.bob.id]))
    expect(await t.inbox(t.ada.id)).toEqual([body])
    expect(await t.inbox(t.bob.id)).toEqual([body])
  })
})

describe(`addsMentionUserIds`, () => {
  it(`a new message counts any listed id; an edit only an added one`, () => {
    expect(addsMentionUserIds(null, [`a`])).toBe(true)
    expect(addsMentionUserIds(null, [])).toBe(false)
    expect(addsMentionUserIds([`a`], [`a`])).toBe(false)
    expect(addsMentionUserIds([`a`], [`a`, `b`])).toBe(true)
    expect(addsMentionUserIds([`a`], null)).toBe(false)
  })
})
