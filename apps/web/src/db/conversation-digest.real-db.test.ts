// @vitest-environment node
import { describe, expect, it } from "vitest"
import { sql } from "drizzle-orm"
import {
  chatPreview,
  conversationDigest,
  digestMessages,
  DIGEST_TEXT_MAX,
  type ConversationDigest,
} from "@exp/domain-contract/chats"
import { sessionTitle } from "@exp/domain-contract/session-title"
import type { TreeMessage } from "@exp/domain-contract/session-tree"
import { inRollback, probeRealDb } from "@/test/real-db"

// #EXP-1380 (review of #1218): the SQL `conversation_digest` (0001_triggers.sql)
// against its TS reference `conversationDigest` on LONG bodies: the first
// prompt's body starts at its first usable line BEFORE the 500-character cut
// (so the title rule reads the same line over the digest as over the whole
// message), and cuts count code points on both sides.

const ready = await probeRealDb(`SELECT EXISTS (SELECT 1 FROM pg_proc WHERE proname = 'conversation_digest_first_body') AS ok`)
if (!ready.ok) console.info(`[conversation-digest.real-db] skipped: ${ready.reason}`)

const id8 = () => crypto.randomUUID().slice(0, 8)
const at = (s: number) => new Date(Date.UTC(2026, 9, 24, 12, 0, s)).toISOString()

const images = Array.from({ length: 14 }, (_, i) => `![shot ${i}](/api/attachments/${crypto.randomUUID()})`).join(`\n`)
const longLine = `Fix the window so the waiting card is never cut off, `.repeat(14).trim()
const cases: { name: string; body: string }[] = [
  { name: `image lines past the cut before the first prompt`, body: `\n  \n${images}\n \n${longLine}\nsecond line` },
  { name: `a first line longer than the cut`, body: `${longLine} ${longLine}` },
  { name: `astral characters around the cut`, body: `😀`.repeat(DIGEST_TEXT_MAX + 20) },
  { name: `a line of one v (no escape trims it)`, body: `v\n${longLine}` },
]

async function seedConversation(tx: Parameters<Parameters<typeof inRollback>[0]>[0]) {
  const userId = `digest-test-${id8()}`
  await tx.execute(sql`
    INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
    VALUES (${userId}, 'Tester', ${`${userId}@digest.test`}, true, false, now(), now())`)
  const team = await tx.execute<{ id: string }>(sql`
    INSERT INTO teams (name, slug) VALUES ('Digest', ${`digest-${id8()}`}) RETURNING id`)
  const teamId = team.rows[0]!.id
  const sessionId = crypto.randomUUID()
  await tx.execute(sql`
    INSERT INTO sessions (id, team_id, root_id, user_id, device_id)
    VALUES (${sessionId}, ${teamId}, ${sessionId}, ${userId}, 'dev')`)
  const conv = await tx.execute<{ id: string }>(sql`SELECT id FROM conversations WHERE session_id = ${sessionId}`)
  const conversationId = conv.rows[0]!.id
  const insert = async (rows: TreeMessage[]) => {
    for (const m of rows) {
      await tx.execute(sql`
        INSERT INTO messages (id, root_id, team_id, conversation_id, author_kind, author_id, body, card, created_at)
        VALUES (${m.id}, ${sessionId}, ${teamId}, ${conversationId}, ${m.authorKind}, ${m.authorId}, ${m.body},
          ${m.card ? JSON.stringify(m.card) : null}::jsonb, ${m.createdAt}::timestamptz)`)
    }
    const read = await tx.execute<{ digest: ConversationDigest }>(sql`SELECT digest FROM conversations WHERE id = ${conversationId}`)
    return read.rows[0]!.digest
  }
  return { userId, sessionId, insert }
}

// EXP-1458: a sealed body is kept whole (never cut or line-split, longer
// than the cut here), a sealed card is copied whole (clear fields + sealed).
const sealedValue = `v1:1:${`A`.repeat(DIGEST_TEXT_MAX + 200)}`
const sealedBody = `exp-sealed:${sealedValue}`

describe.skipIf(!ready.ok)(`conversation digest: sealed content (EXP-1458)`, () => {
  it(`keeps a sealed body and a sealed card whole, SQL = TS`, async () => {
    await inRollback(async (tx) => {
      const { userId, sessionId, insert } = await seedConversation(tx)
      const card = (kind: string) => ({ kind, decision: `pending`, pictures: [{ label: `web list`, attachmentId: crypto.randomUUID() }], sealed: sealedValue })
      const rows: TreeMessage[] = [
        { id: crypto.randomUUID(), sessionId, authorKind: `user`, authorId: userId, authorSessionId: null, body: sealedBody, card: null, answeredAt: null, consumedAt: null, createdAt: at(1) },
        { id: crypto.randomUUID(), sessionId, authorKind: `agent`, authorId: null, authorSessionId: null, body: ``, card: card(`question`), answeredAt: null, consumedAt: null, createdAt: at(2) },
        { id: crypto.randomUUID(), sessionId, authorKind: `agent`, authorId: null, authorSessionId: null, body: sealedBody, card: null, answeredAt: null, consumedAt: null, createdAt: at(3) },
      ]
      const sqlDigest = await insert(rows)
      expect(sqlDigest).toEqual(conversationDigest(sessionId, rows))
      expect(sqlDigest.first?.body).toBe(sealedBody)
      expect(sqlDigest.preview?.body).toBe(sealedBody)
      expect(sqlDigest.waiting?.card).toEqual(rows[1]!.card)
    })
  })

  it(`a plaintext body that only mentions the prefix stays plaintext (cut)`, async () => {
    await inRollback(async (tx) => {
      const { userId, sessionId, insert } = await seedConversation(tx)
      const body = `see exp-sealed: ${`x`.repeat(DIGEST_TEXT_MAX)}`
      const rows: TreeMessage[] = [
        { id: crypto.randomUUID(), sessionId, authorKind: `user`, authorId: userId, authorSessionId: null, body, card: null, answeredAt: null, consumedAt: null, createdAt: at(1) },
      ]
      const sqlDigest = await insert(rows)
      expect(sqlDigest).toEqual(conversationDigest(sessionId, rows))
      expect(sqlDigest.first?.body.length).toBe(DIGEST_TEXT_MAX)
    })
  })
})

describe.skipIf(!ready.ok)(`conversation digest: SQL = TS on long bodies`, () => {
  for (const c of cases)
    it(c.name, async () => {
      await inRollback(async (tx) => {
        const userId = `digest-test-${id8()}`
        await tx.execute(sql`
          INSERT INTO users (id, name, email, email_verified, is_admin, created_at, updated_at)
          VALUES (${userId}, 'Tester', ${`${userId}@digest.test`}, true, false, now(), now())`)
        const team = await tx.execute<{ id: string }>(sql`
          INSERT INTO teams (name, slug) VALUES ('Digest', ${`digest-${id8()}`}) RETURNING id`)
        const teamId = team.rows[0]!.id
        const sessionId = crypto.randomUUID()
        await tx.execute(sql`
          INSERT INTO sessions (id, team_id, root_id, user_id, device_id)
          VALUES (${sessionId}, ${teamId}, ${sessionId}, ${userId}, 'dev')`)
        const conv = await tx.execute<{ id: string }>(sql`SELECT id FROM conversations WHERE session_id = ${sessionId}`)
        const conversationId = conv.rows[0]!.id
        const rows: TreeMessage[] = [
          { id: crypto.randomUUID(), sessionId, authorKind: `user`, authorId: userId, authorSessionId: null, body: c.body, card: null, answeredAt: null, consumedAt: null, createdAt: at(1) },
          { id: crypto.randomUUID(), sessionId, authorKind: `agent`, authorId: null, authorSessionId: null, body: c.body, card: null, answeredAt: null, consumedAt: null, createdAt: at(2) },
          { id: crypto.randomUUID(), sessionId, authorKind: `agent`, authorId: null, authorSessionId: null, body: ``, card: { kind: `plan`, plan: c.body }, answeredAt: null, consumedAt: null, createdAt: at(3) },
        ]
        for (const m of rows) {
          await tx.execute(sql`
            INSERT INTO messages (id, root_id, team_id, conversation_id, author_kind, author_id, body, card, created_at)
            VALUES (${m.id}, ${sessionId}, ${teamId}, ${conversationId}, ${m.authorKind}, ${m.authorId}, ${m.body},
              ${m.card ? JSON.stringify(m.card) : null}::jsonb, ${m.createdAt}::timestamptz)`)
        }
        const read = await tx.execute<{ digest: ConversationDigest }>(sql`SELECT digest FROM conversations WHERE id = ${conversationId}`)
        const sqlDigest = read.rows[0]!.digest
        const tsDigest = conversationDigest(sessionId, rows)
        expect(sqlDigest).toEqual(tsDigest)
        // The title over the digest = the title over the whole conversation.
        const session = { id: sessionId, rootId: sessionId, title: null }
        expect(sessionTitle(session, [], [], digestMessages(sessionId, sqlDigest))).toBe(sessionTitle(session, [], [], rows))
        // The preview over the digest starts like the whole one (it is cut).
        const preview = (messages: TreeMessage[]) => chatPreview({ rootId: sessionId, messages, needsYou: true, busy: false }).text
        expect(preview(rows).startsWith(preview(digestMessages(sessionId, sqlDigest)))).toBe(true)
      })
    })
})
