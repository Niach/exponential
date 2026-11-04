import { sql } from "drizzle-orm"
import type { MessageCard } from "@exp/db-schema/domain"
import { PRIVATE_PR_LEAD, PRIVATE_REPLY_LEAD, PRIVATE_REPORT_LEAD } from "@exp/domain-contract/chats"
import type { Event } from "@/db/schema"
import { TRIGGER_COPY } from "@/lib/agents/triggers"
import { askHiddenText, privateText } from "@/lib/sessions/bus"
import { clearCard, isSealedBody, isSealedCard } from "@/lib/sessions/sealed"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1409: a conversation whose readers change (an agent moved from a
// private device to a shared one flips its trees to `shared`) must not
// expose what a private session's bus wrote there in full while only its
// owner could read it. Every such line in the trees `rootIds` that its
// readers may NOW not see (`askHidden`) becomes the neutral line the bus
// writes today: a relayed reply, an end report, a PR line (the 0199 rules),
// an ask mirror (`askHiddenText`) and a `session.ended` trigger firing (its
// ids-only payload). Runs in the move's own transaction. EXP-1458: a
// SEALED line (body envelope or `sealed` card) is never read: the tree's
// new readers would get its key, so it too becomes the neutral line its
// clear `kind` picks (a reply's otherwise) and its card keeps only the
// clear part (no `sealed`).
//
// #EXP-1410: a firing is matched on its own card and the ids its own body
// quotes, never through `events`: that row is pruned after 30 days while the
// firing stays. Left alone on purpose: a private session's explicit
// `sessions_send` into its owner's private conversation. The author chose to
// write those words there and they reach the conversation as a message, not
// as a bus copy of private work, so they stay in full after a move.

interface Row extends Record<string, unknown> {
  id: string
  originId: string
  body: string
  card: MessageCard | null
  relay: boolean
  askKind: string | null
}

interface Firing extends Record<string, unknown> {
  id: string
  body: string
  card: MessageCard | null
  rootId: string | null
  sessionId: string | null
}

/** Who a firing's line says emitted its event (`TRIGGER_COPY.event`). */
function firingEmitter(body: string): Event[`emittedBy`] {
  const head = body.split(`\n`, 1)[0] ?? ``
  return head.includes(`(from a session)`) ? `session` : head.includes(`(from a person)`) ? `user` : `server`
}

function neutral(row: Row): { body: string; card: MessageCard | null } | null {
  if (isSealedBody(row.body) || isSealedCard(row.card)) return neutralSealed(row)
  const origin = { id: row.originId }
  const card = row.card
  if (!card) return row.relay ? { body: privateText(origin, PRIVATE_REPLY_LEAD), card: null } : null
  if (card.kind === `report`) {
    return { body: privateText(origin, PRIVATE_REPORT_LEAD), card: { ...card, summary: PRIVATE_REPORT_LEAD } }
  }
  if (card.kind === `event` && card.event === `pr_opened`) {
    const { pullRequestId: _pr, ...rest } = card
    return { body: privateText(origin, PRIVATE_PR_LEAD), card: rest }
  }
  if (card.kind === `ask` && (row.askKind === `question` || row.askKind === `plan`)) {
    return { body: askHiddenText(origin, { kind: row.askKind } as never), card }
  }
  return null
}

/** EXP-1458: a sealed line by its CLEAR fields only. The ask mirror is
 *  server-written ({kind, sessionId, messageId}: clear) and stays. */
function neutralSealed(row: Row): { body: string; card: MessageCard | null } {
  const origin = { id: row.originId }
  const card = row.card
  if (!card) return { body: privateText(origin, PRIVATE_REPLY_LEAD), card: null }
  const clear = clearCard(card as unknown as Record<string, unknown>)
  if (card.kind === `ask` && (row.askKind === `question` || row.askKind === `plan`)) {
    return { body: askHiddenText(origin, { kind: row.askKind } as never), card: clear as MessageCard }
  }
  if (card.kind === `report`) {
    return {
      body: privateText(origin, PRIVATE_REPORT_LEAD),
      card: { ...clear, summary: PRIVATE_REPORT_LEAD } as MessageCard,
    }
  }
  if (card.kind === `event` && card.event === `pr_opened`) {
    const { pullRequestId: _pr, ...rest } = clear
    return { body: privateText(origin, PRIVATE_PR_LEAD), card: rest as MessageCard }
  }
  return { body: privateText(origin, PRIVATE_REPLY_LEAD), card: clear as MessageCard }
}

/** Rewrites the private lines of `rootIds`' trees their readers may no
 *  longer see. Returns how many rows changed. */
export async function hidePrivateLinesInTx(tx: Tx, rootIds: readonly string[]): Promise<number> {
  if (rootIds.length === 0) return 0
  const roots = sql.join(rootIds.map((id) => sql`${id}::uuid`), sql`, `)
  const lines = await tx.execute<Row>(sql`
    SELECT m.id, o.id AS "originId", m.body, m.card,
      (m.card IS NULL AND m.author_kind = 'agent' AND EXISTS (
        SELECT 1 FROM messages r
        WHERE r.conversation_id = (SELECT oc.id FROM conversations oc WHERE oc.session_id = o.id) AND r.author_kind = 'agent' AND r.card IS NULL
          AND r.author_session_id IS NULL AND r.created_at = m.created_at
      )) AS relay,
      q.card->>'kind' AS "askKind"
    FROM messages m
    JOIN conversations mc ON mc.id = m.conversation_id
    JOIN sessions l ON l.id = mc.session_id
    JOIN sessions o ON o.id = m.author_session_id
    LEFT JOIN messages q ON m.card->>'kind' = 'ask' AND q.id::text = m.card->>'messageId'
    WHERE l.root_id IN (${roots}) AND mc.session_id <> m.author_session_id
      AND NOT o.shared AND (l.shared OR l.user_id <> o.user_id)
  `)
  // A `session.ended` firing whose session is private (or gone) and hidden
  // from the conversation's readers. Its ids come from its own body: the
  // top-level keys sit on lines of their own, two spaces in (a nested or
  // string value's never match: JSON escapes newlines and quotes).
  const firedId = (key: string) => sql.raw(`substring(m.body from E'\\n  "${key}": "([0-9a-fA-F-]{36})"')`)
  const firings = await tx.execute<Firing>(sql`
    SELECT m.id, m.body, m.card, ${firedId(`rootId`)} AS "rootId", ${firedId(`sessionId`)} AS "sessionId"
    FROM messages m
    JOIN conversations mc ON mc.id = m.conversation_id
    JOIN sessions l ON l.id = mc.session_id
    LEFT JOIN sessions o ON o.id::text = ${firedId(`sessionId`)}
    WHERE l.root_id IN (${roots}) AND m.card->>'event' = 'trigger_fired' AND m.card->>'name' = 'session.ended'
      AND (o.id IS NULL OR (NOT o.shared AND (l.shared OR l.user_id <> o.user_id)))
  `)
  const changes: { id: string; body: string; card: MessageCard | null }[] = []
  for (const row of lines.rows) {
    const next = neutral(row)
    if (!next || (next.body === row.body && JSON.stringify(next.card) === JSON.stringify(row.card))) continue
    changes.push({ id: row.id, body: next.body, card: next.card })
  }
  for (const row of firings.rows) {
    const payload = { rootId: row.rootId, sessionId: row.sessionId }
    const body = TRIGGER_COPY.event({ name: `session.ended`, emittedBy: firingEmitter(row.body), payload })
    if (body !== row.body) changes.push({ id: row.id, body, card: row.card })
  }
  if (changes.length === 0) return 0
  // One statement however long the conversation is (a JSON `null` card reads
  // as SQL NULL, as the row had it).
  await tx.execute(sql`
    UPDATE messages m SET body = c.body, card = c.card, updated_at = ${new Date()}
    FROM jsonb_to_recordset(${JSON.stringify(changes)}::jsonb) AS c(id uuid, body text, card jsonb)
    WHERE m.id = c.id
  `)
  return changes.length
}
