// #VAPP-236 (eApps A2, docs/spec/apps.md §7.3): an app hands judgement to a
// TEAM AGENT. One run form, three callers: an automation (as its sponsor),
// an exposed tool and the `exponential.apps.handoff` action (as the calling
// member). Every hand-off goes through `sessions.send {agentId}` (the one
// entrypoint: the member's own conversation with that agent, created on
// first use on the agent's device) tagged `via <app>`, its message the
// package's frozen prompt + the input as a fenced data block (@exp/emcp
// exposes.json `agent.briefCases`: data is never instructions, §12.1).
import { TRPCError } from "@trpc/server"
import { and, asc, eq, ne } from "drizzle-orm"
import { agentBrief, agentRateAllows, agentSlug, type AgentRun } from "@exp/emcp/exposes"
import { db } from "@/db/connection"
import { agents, sessions } from "@/db/schema"
import { sendSession, treeAgentId } from "@/lib/sessions/send"

/** The hand-offs of each app in the last minute (one instance serves the
 *  deploy; a restart forgets them, which only ever lets more through). */
const recent = new Map<string, number[]>()

/** Counts and records one hand-off when the app is under the cap. */
export function takeHandoffSlot(appId: string, now: number = Date.now()): boolean {
  const since = now - 60_000
  const list = (recent.get(appId) ?? []).filter((at) => at > since)
  const allowed = agentRateAllows(list.length)
  if (allowed) list.push(now)
  if (list.length > 0) recent.set(appId, list)
  else recent.delete(appId)
  return allowed
}

/** Test-only: forget the counted hand-offs. */
export function resetHandoffSlots(): void {
  recent.clear()
}

/** The team agent a slug names (never someone's personal Assistant): the
 *  oldest whose name slugs to it. */
export async function agentBySlug(teamId: string, slug: string): Promise<{ id: string; name: string } | null> {
  const rows = await db
    .select({ id: agents.id, name: agents.name })
    .from(agents)
    .where(and(eq(agents.teamId, teamId), ne(agents.role, `assistant`)))
    .orderBy(asc(agents.createdAt), asc(agents.id))
  return rows.find((row) => agentSlug(row.name) === slug) ?? null
}

/** The agent acting in `sessionId`'s tree (its root's conversation agent,
 *  else the agent that started it), or null. */
export async function actingAgentId(sessionId: string | null | undefined): Promise<string | null> {
  if (!sessionId) return null
  const [row] = await db.select({ rootId: sessions.rootId }).from(sessions).where(eq(sessions.id, sessionId)).limit(1)
  if (!row) return null
  const [root] = await db
    .select({ agentId: sessions.agentId, startedByAgentId: sessions.startedByAgentId })
    .from(sessions)
    .where(eq(sessions.id, row.rootId))
    .limit(1)
  return root ? treeAgentId(root) : null
}

/** The slug of the agent acting in `sessionId`'s tree, or null. */
export async function actingAgentSlug(sessionId: string | null | undefined): Promise<string | null> {
  const agentId = await actingAgentId(sessionId)
  if (!agentId) return null
  const [agent] = await db.select({ name: agents.name }).from(agents).where(eq(agents.id, agentId)).limit(1)
  return agent ? agentSlug(agent.name) : null
}

export type HandoffResult = { sessionId: string } | { code: `rateLimited` | `agentMissing` | `toolError`; error: string }

/** Hands `input` to the run's agent as `userId`. */
export async function handOff(args: {
  app: { id: string; slug: string; name: string; teamId: string }
  userId: string
  run: AgentRun
  input: Record<string, unknown>
  /** `automation <name>`, `tool <name>`, `handoff <name>`. */
  source: string
  ref?: { kind: string; id: string } | null
}): Promise<HandoffResult> {
  const agent = await agentBySlug(args.app.teamId, args.run.agent)
  if (!agent) return { code: `agentMissing`, error: `the team has no agent ${JSON.stringify(args.run.agent)} (its name slugged)` }
  if (!takeHandoffSlot(args.app.id)) return { code: `rateLimited`, error: `${args.app.name} handed off too often this minute` }
  const text = agentBrief({ prompt: args.run.prompt, input: args.input, app: { name: args.app.name, slug: args.app.slug }, source: args.source, ref: args.ref ?? null })
  try {
    const sent = await sendSession({ userId: args.userId, input: { agentId: agent.id, teamId: args.app.teamId, text }, viaAppId: args.app.id })
    return { sessionId: sent.sessionId }
  } catch (err) {
    const words = err instanceof TRPCError || err instanceof Error ? err.message : `the hand-off failed`
    return { code: `toolError`, error: words.slice(0, 500) }
  }
}
