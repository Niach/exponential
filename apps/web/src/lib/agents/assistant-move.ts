import { sql } from "drizzle-orm"
import { db } from "@/db/connection"
// Vite's ?raw inlines the file at build time (as bootstrap-cloud.ts does with
// the triggers): no fs read at runtime.
import moveSql from "./assistant-move.sql?raw"
import { assistantMoveOn } from "./assistant-move-switch"

// #EXP-1986 (agent-settings.json `assistant.move`, #EXP-1984): THE MOVE to one
// Assistant per member in their hidden personal team, behind ONE switch.
// Off (unset, the default): every Assistant stays one per (team, member) as
// before and nothing moves. On (`ASSISTANT_MOVE=on`, set by the release
// shepherd only once the clients that work across the move are released on
// every platform and the minimum versions name them): new Assistants are
// created in the personal team (lib/agents/assistant.ts) and every boot runs
// `assistant-move.sql`, which moves whoever is still in a team (idempotent).

export { assistantMoveOn } from "./assistant-move-switch"

/** The move's statements, in order. */
export function assistantMoveStatements(): string[] {
  return moveSql
    .split(`--> statement-breakpoint`)
    .map((statement) => statement.trim())
    .filter(Boolean)
}

type Runner = Pick<typeof db, `transaction`>

/** Runs the move in ONE transaction under an advisory lock (two instances
 *  booting at once queue; the second finds nobody left). No-op while off. */
export async function runAssistantMove(database: Runner = db): Promise<void> {
  if (!assistantMoveOn()) return
  await database.transaction(async (tx) => {
    await tx.execute(sql`SELECT pg_advisory_xact_lock(hashtext('exp1986-assistant-move'))`)
    for (const statement of assistantMoveStatements()) await tx.execute(sql.raw(statement))
  })
}
