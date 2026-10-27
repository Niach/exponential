import { eq, sql } from "drizzle-orm"
import type { MessageCard } from "@exp/db-schema/domain"
import { PRIVATE_PR_LEAD, PRIVATE_REPLY_LEAD, PRIVATE_REPORT_LEAD } from "@exp/domain-contract/chats"
import { messages, type Event } from "@/db/schema"
import { TRIGGER_COPY } from "@/lib/agents/triggers"
import { askHiddenText, privateText } from "@/lib/sessions/bus"
import type { Tx } from "@/lib/sessions/tx"

// #EXP-1409: a conversation whose readers change (an agent moved from a
// private device to a shared one flips its trees to `shared`) must not
// expose what a private session's bus wrote there in full while only its
// owner could read it. Every such line in the trees `rootIds` that its
// readers may NOW not see (`askHidden`) becomes the neutral line the bus
// writes today: a relayed reply, an end report, a PR line (the 0199 rules),
// an ask mirror (`askHiddenText`) and a `session.ended` trigger firing (its
// ids-only payload). Runs in the move's own transaction.

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
  name: string
  emittedBy: Event[`emittedBy`]
  payload: Record<string, unknown>
}

function neutral(row: Row): { body: string; card: MessageCard | null } | null {
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

/** Rewrites the private lines of `rootIds`' trees their readers may no
 *  longer see. Returns how many rows changed. */
export async function hidePrivateLinesInTx(tx: Tx, rootIds: readonly string[]): Promise<number> {
  if (rootIds.length === 0) return 0
  const roots = sql.join(rootIds.map((id) => sql`${id}::uuid`), sql`, `)
  const lines = await tx.execute<Row>(sql`
    SELECT m.id, o.id AS "originId", m.body, m.card,
      (m.card IS NULL AND m.author_kind = 'agent' AND EXISTS (
        SELECT 1 FROM messages r
        WHERE r.session_id = o.id AND r.author_kind = 'agent' AND r.card IS NULL
          AND r.author_session_id IS NULL AND r.created_at = m.created_at
      )) AS relay,
      q.card->>'kind' AS "askKind"
    FROM messages m
    JOIN sessions l ON l.id = m.session_id
    JOIN sessions o ON o.id = m.author_session_id
    LEFT JOIN messages q ON m.card->>'kind' = 'ask' AND q.id::text = m.card->>'messageId'
    WHERE l.root_id IN (${roots}) AND m.session_id <> m.author_session_id
      AND NOT o.shared AND (l.shared OR l.user_id <> o.user_id)
  `)
  // A `session.ended` firing whose session is private (or gone) and
  // hidden from the conversation's readers.
  const firings = await tx.execute<Firing>(sql`
    SELECT m.id, m.body, e.name, e.emitted_by AS "emittedBy", e.payload
    FROM messages m
    JOIN sessions l ON l.id = m.session_id
    JOIN events e ON e.id::text = m.card->>'eventId'
    LEFT JOIN sessions o ON o.id::text = e.payload->>'sessionId'
    WHERE l.root_id IN (${roots}) AND m.card->>'event' = 'trigger_fired' AND e.name = 'session.ended'
      AND (o.id IS NULL OR (NOT o.shared AND (l.shared OR l.user_id <> o.user_id)))
  `)
  const now = new Date()
  let changed = 0
  for (const row of lines.rows) {
    const next = neutral(row)
    if (!next || (next.body === row.body && JSON.stringify(next.card) === JSON.stringify(row.card))) continue
    await tx.update(messages).set({ body: next.body, card: next.card, updatedAt: now }).where(eq(messages.id, row.id))
    changed += 1
  }
  for (const row of firings.rows) {
    const payload = { rootId: row.payload.rootId ?? null, sessionId: row.payload.sessionId ?? null }
    const body = TRIGGER_COPY.event({ name: row.name, emittedBy: row.emittedBy, payload })
    if (body === row.body) continue
    await tx.update(messages).set({ body, updatedAt: now }).where(eq(messages.id, row.id))
    changed += 1
  }
  return changed
}
