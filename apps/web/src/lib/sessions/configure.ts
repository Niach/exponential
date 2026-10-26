import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"
import type { SessionsConfigureInput } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { mcpServers, sessions } from "@/db/schema"
import { generateTxId } from "@/lib/trpc"
import { assertCanSteer, loadSessionForUser, sessionDeviceOwner } from "@/lib/sessions/access"
import { assertRunnerVocabulary, effectiveCli, liftWallInTx, runnerPicks } from "@/lib/sessions/send"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// EXP-1334 — `sessions.configure`: a live session's launch options change
// MID-SESSION (plan mode, effort, ultracode, computer use, MCP picks). The
// DB is the bus again: the row moves, the host device sees it on the
// sessions shape and applies it — plan mode and the model at once (live
// switches), the rest between turns by resuming the SAME run with the new
// flags (`coding::pickup` Switch). The MCP pick stays SERVER-ONLY: the
// device reads it back through `sessions.mcpPicks` at the next turn.

export async function configureSession(
  userId: string,
  input: SessionsConfigureInput
): Promise<{ txid: number }> {
  const target = await loadSessionForUser(userId, input.sessionId)
  await assertCanSteer(userId, target)
  if (target.status === `ended`) {
    throw new TRPCError({
      code: `BAD_REQUEST`,
      message: `This session has ended: start a new one to change its options`,
    })
  }
  const effort = input.effort === undefined ? undefined : input.effort?.trim() || null
  // The agent the session runs: its own `cli`, else its device's default
  // (unknowable = validated as claude, the device masks what it cannot do).
  const cli = (await effectiveCli(target)) ?? undefined
  assertRunnerVocabulary({
    cli,
    model: input.model,
    effort: effort ?? undefined,
    ultracode: input.ultracode,
  })
  const mcpServerIds =
    input.mcpServerIds === undefined ? undefined : await assertMcpServers(target.teamId, input.mcpServerIds)

  const picks = runnerPicks(input)
  const result = await db.transaction(async (tx) => {
    const txid = await generateTxId(tx)
    // EXP-1376: a model pick on a walled session lifts a model wall at once
    // (the queued messages run on it) or refuses, never a silent success;
    // read under the row lock.
    await liftWallInTx(tx, target.id, { switching: false, model: picks.model })
    const set: Partial<typeof sessions.$inferInsert> = picks
    if (mcpServerIds !== undefined) set.mcpServerIds = mcpServerIds
    await tx
      .update(sessions)
      .set({ ...set, updatedAt: new Date() })
      .where(eq(sessions.id, target.id))
    return { txid }
  })
  // The host device acts on the row at its next pass; a relay nudge kills
  // the latency (best-effort, like a send).
  const config = getSteerRelayConfig()
  if (config) void relayPostNudge(config, await sessionDeviceOwner(target), target.deviceId)
  return result
}

/** The picked team MCP servers (deduped, pick order), refusing any outside
 *  the session's team. An empty pick = none (stored NULL). */
async function assertMcpServers(teamId: string, ids: string[]): Promise<string[] | null> {
  const unique = [...new Set(ids)]
  if (unique.length === 0) return null
  const rows = await db
    .select({ id: mcpServers.id })
    .from(mcpServers)
    .where(and(eq(mcpServers.teamId, teamId), inArray(mcpServers.id, unique)))
  if (rows.length !== unique.length) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: `Unknown MCP server for this team` })
  }
  return unique
}

/** The device's read-back of a session's server-only MCP pick (sorted ids,
 *  `[]` = none), compared against what its run launched with. */
export function mcpPicks(session: { mcpServerIds: string[] | null }): string[] {
  return [...new Set(session.mcpServerIds ?? [])].sort()
}
