// @vitest-environment node
import { afterAll, describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
import { probeRealDb } from "@/test/real-db"

// EXP-1468 against the real migrated Postgres: a PR body's pictures come
// from the opener's story cards; a SEALED one counts through its clear
// `pictures` (entries with an attachmentId) unless its clear `story` is false.

const ready = await probeRealDb(`
  SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'messages' AND column_name = 'ref_issue_ids') AS ok
`)
if (!ready.ok) console.info(`[pr-stories.real-db] skipped: ${ready.reason}`)

const { loadStoryPictures } = await import(`@/lib/pr-stories`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const made = { teams: [] as string[], users: [] as string[] }

afterAll(async () => {
  if (!ready.ok) return
  for (const id of made.teams) await db.execute(sql`DELETE FROM teams WHERE id = ${id}`)
  for (const id of made.users) await db.execute(sql`DELETE FROM users WHERE id = ${id}`)
})

describe.skipIf(!ready.ok)(`EXP-1468: story pictures of sealed cards`, () => {
  it(`reads plaintext story cards and a sealed one's clear pictures`, async () => {
    const userId = `stories-test-${id8()}`
    await db.execute(sql`
      INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
      VALUES (${userId}, 'Tester', ${`${userId}@stories.test`}, true, false, now(), now())`)
    made.users.push(userId)
    const team = await db.execute<{ id: string }>(sql`
      INSERT INTO teams (name, slug) VALUES ('Stories', ${`stories-${id8()}`}) RETURNING id`)
    const teamId = team.rows[0]!.id
    made.teams.push(teamId)
    const sessionId = crypto.randomUUID()
    await db.execute(sql`
      INSERT INTO sessions (id, team_id, root_id, user_id, device_id)
      VALUES (${sessionId}, ${teamId}, ${sessionId}, ${userId}, 'dev')`)
    const conv = await db.execute<{ id: string }>(sql`SELECT id FROM conversations WHERE session_id = ${sessionId}`)
    const conversationId = conv.rows[0]!.id
    const plain = crypto.randomUUID()
    const sealedId = crypto.randomUUID()
    const sealed = `v1:1:${`A`.repeat(80)}`
    const cards = [
      { kind: `surface`, pictures: [{ label: `web list`, attachmentId: plain }] },
      { kind: `surface`, story: true, pictures: [{ label: `ios list`, attachmentId: sealedId }, { label: `web later` }], sealed },
      { kind: `surface`, story: false, pictures: [{ label: `web chat`, attachmentId: crypto.randomUUID() }], sealed },
    ]
    for (const [index, card] of cards.entries()) {
      await db.execute(sql`
        INSERT INTO messages (root_id, team_id, conversation_id, author_kind, author_session_id, body, card, created_at)
        VALUES (${sessionId}, ${teamId}, ${conversationId}, 'agent', ${sessionId}, '', ${JSON.stringify(card)}::jsonb,
          now() + ${`${index} seconds`}::interval)`)
    }
    const pictures = await loadStoryPictures(sessionId, null)
    expect(pictures.map((picture) => [picture.label, picture.attachmentId])).toEqual([
      [`web list`, plain],
      [`ios list`, sealedId],
    ])
  })
})
