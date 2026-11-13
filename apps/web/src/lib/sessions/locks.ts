import { inArray, sql } from "drizzle-orm"
import { sessions } from "@/db/schema"
import type { Tx } from "@/lib/sessions/tx"
import { busListeners } from "@/lib/sessions/bus"
import type { Session } from "@/db/schema"

/**
 * #ERR-5: THE row-lock order of the session tree. A transaction that writes
 * several sessions (a child's reply and its bus copy into the parent, an End
 * and its subtree's reports) row-locks them all HERE, before its first
 * write: ancestors first (`depth`, then `id`), one global order, so two
 * writers on the same rows queue instead of deadlocking. A message write
 * locks its conversation's session itself (`refresh_conversation_digests`),
 * which keeps "a session before its conversation" for single-row writes.
 * `FOR NO KEY UPDATE` = what an UPDATE takes; FK checks (key share) pass.
 */
export async function lockSessionRows(tx: Tx, ids: readonly (string | null | undefined)[]): Promise<void> {
  const unique = [...new Set(ids.filter((id): id is string => !!id))]
  if (unique.length === 0) return
  await tx
    .select({ id: sessions.id })
    .from(sessions)
    .where(inArray(sessions.id, unique))
    .orderBy(sessions.depth, sessions.id)
    .for(`no key update`)
}

/** Every row of a tree, ancestors first (a whole-tree End or delete). */
export async function lockTreeRows(tx: Tx, rootId: string): Promise<void> {
  await tx.execute(
    sql`SELECT id FROM ${sessions} WHERE ${sessions.rootId} = ${rootId} ORDER BY depth, id FOR NO KEY UPDATE`
  )
}

/** A session about to write itself and then reach its listeners (the bus:
 *  its parent, its watchers): all of their rows, in the one order. */
export async function lockBusRows(tx: Tx, session: Pick<Session, `id` | `parentId`>): Promise<void> {
  const listeners = await busListeners(tx, session)
  await lockSessionRows(tx, [session.id, session.parentId, ...listeners.map((listener) => listener.id)])
}
