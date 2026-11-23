import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq, isNotNull, or } from "drizzle-orm"
import {
  ASSISTANT,
  ASSISTANT_COPY,
  assistantScheduleTriggers,
  checkInProblem,
  legacyCheckIn,
  type AgentTriggerLike,
} from "@exp/domain-contract/agent-settings"
import type { AgentTrigger } from "@exp/db-schema/domain"
import { createHash } from "node:crypto"
import { db } from "@/db/connection"
import { agents, devices, teamMembers, teams, userVoiceSettings, type Agent } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import { invalidateMembershipCaches } from "@/lib/auth/membership-cache"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// S-3 / #EXP-1984 (agent-settings.json `assistant`, `assistant.move`):
// every member's ONE Assistant across their teams (`agents.role =
// assistant`), stored in their hidden personal team (`teams.kind =
// personal`, created with it; #EXP-1989: `uniq_agents_assistant` holds one
// per owner, and migration 0293 moved every per-team one). Created on first use on the member's own
// newest desktop (else their newest device); owned and edited by that member
// only; never deleted. Its device is the member's VOICE device:
// `user_voice_settings.voice_device_id` / `agent_id` stay as a server-kept
// MIRROR of the voice Assistant (old clients, the heartbeat and the voice
// runtime read them unchanged).

type Exec = Pick<typeof db, `select` | `insert` | `update`>

const PERSONAL_TEAM_NAME = `Personal`

/** A personal team's slug: unique, never a person's word (no screen shows
 *  it). The move's migration computes the same one in SQL. */
export function personalTeamSlug(userId: string): string {
  return `personal-${createHash(`sha256`).update(userId).digest(`hex`).slice(0, 24)}`
}

/** The member's hidden personal team, created on first use. */
export async function ensurePersonalTeam(exec: Exec, userId: string): Promise<string> {
  const mine = () =>
    exec
      .select({ id: teams.id })
      .from(teams)
      .where(and(eq(teams.ownerId, userId), eq(teams.kind, `personal`)))
      .limit(1)
  const [found] = await mine()
  if (found) return found.id
  await exec
    .insert(teams)
    .values({ name: PERSONAL_TEAM_NAME, slug: personalTeamSlug(userId), kind: `personal`, ownerId: userId })
    .onConflictDoNothing()
  const [created] = await mine()
  if (!created) throw new TRPCError({ code: `CONFLICT`, message: `Your Assistant just changed; try again.` })
  // The shapes' team list (getUserTeamIds) now holds it.
  invalidateMembershipCaches()
  return created.id
}

/** The member's ONE Assistant (#EXP-1989: the index holds one per owner). */
export async function findAssistant(exec: Exec, ownerId: string): Promise<Agent | null> {
  const [row] = await exec
    .select()
    .from(agents)
    .where(and(eq(agents.ownerId, ownerId), eq(agents.role, `assistant`)))
    .limit(1)
  return row ?? null
}

/** Where a new Assistant runs: the member's own newest desktop, else their
 *  newest device; null = none yet (the page reads `copy.noDevice`). */
async function assistantDevice(exec: Exec, userId: string): Promise<string | null> {
  const own = await exec
    .select({ deviceId: devices.deviceId, kind: devices.kind })
    .from(devices)
    .where(eq(devices.userId, userId))
    .orderBy(desc(devices.lastSeenAt))
  return (own.find((row) => row.kind === `desktop`) ?? own[0])?.deviceId ?? null
}

/** The member's Assistant, created on first use. Null while they have no
 *  device to run it on. Their ONE Assistant in their personal team; `teamId`
 *  (clients with a current team send it) is a membership check only. */
export async function ensureAssistant(userId: string, teamId?: string | null, exec: Exec = db): Promise<Agent | null> {
  if (teamId && !(await getTeamMember(userId, teamId))) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Not a member of this team` })
  }
  const existing = await findAssistant(exec, userId)
  if (existing) return existing
  const deviceId = await assistantDevice(exec, userId)
  if (!deviceId) return null
  const home = await ensurePersonalTeam(exec, userId)
  const [created] = await exec
    .insert(agents)
    .values({
      teamId: home,
      name: ASSISTANT_COPY.name,
      icon: null,
      color: null,
      instructions: ``,
      instructionsUpdatedAt: new Date(),
      triggers: assistantScheduleTriggers(ASSISTANT.defaultSchedule as never).map((trigger) => ({
        ...trigger,
        id: crypto.randomUUID(),
      })) as AgentTrigger[],
      deviceId,
      ownerId: userId,
      role: `assistant`,
      sortOrder: 0,
    })
    .onConflictDoNothing()
    .returning()
  // Two first uses raced: the other one's row is the Assistant.
  const assistant = created ?? (await findAssistant(exec, userId))
  if (assistant) await syncVoiceMirror(exec, userId)
  return assistant
}

/** #EXP-1846 (agent-settings.json `checkIn`): any agent's triggers as
 *  `agents.create`/`update` store them. An OLD client's Assistant page
 *  writes its Check in as one untagged preset schedule: that write is
 *  tagged (`legacyCases`). Then at most one Check in, on its preset's cron. */
export function checkedAgentTriggers<T extends AgentTriggerLike>(role: string, triggers: T[]): T[] {
  const legacy = role === `assistant` ? legacyCheckIn(triggers) : null
  const next = legacy ? triggers.map((trigger) => ({ ...trigger, preset: legacy })) : triggers
  const problem = checkInProblem(next)
  if (problem) throw new TRPCError({ code: `BAD_REQUEST`, message: problem })
  return next
}

/** The voice Assistant + device a member's voice runs on: the stored
 *  `agent_id` while it is one of their Assistants, else their oldest
 *  Assistant (#EXP-1935: only in teams they still belong to; the one of a
 *  team they left is no longer theirs to message); the device = its device when that is one of their own
 *  desktops (voice.json: phones and servers never run the voice). */
export async function voiceAssistantOf(
  exec: Exec,
  userId: string,
  storedAgentId: string | null
): Promise<{ agentId: string | null; voiceDeviceId: string | null }> {
  const mine = await exec
    .select({ id: agents.id, deviceId: agents.deviceId })
    .from(agents)
    .innerJoin(teams, eq(teams.id, agents.teamId))
    .leftJoin(teamMembers, and(eq(teamMembers.teamId, agents.teamId), eq(teamMembers.userId, userId)))
    .where(
      and(
        eq(agents.ownerId, userId),
        eq(agents.role, `assistant`),
        // #EXP-1984: their personal team has no team_members row; its owner is its member.
        or(isNotNull(teamMembers.userId), and(eq(teams.kind, `personal`), eq(teams.ownerId, userId)))
      )
    )
    .orderBy(asc(agents.createdAt))
  const assistant = mine.find((row) => row.id === storedAgentId) ?? mine[0]
  if (!assistant) return { agentId: null, voiceDeviceId: null }
  const [device] = await exec
    .select({ kind: devices.kind })
    .from(devices)
    .where(and(eq(devices.userId, userId), eq(devices.deviceId, assistant.deviceId)))
    .limit(1)
  return { agentId: assistant.id, voiceDeviceId: device?.kind === `desktop` ? assistant.deviceId : null }
}

/** Re-derive the mirror columns and nudge the old and the new voice device.
 *  A changed device or agent stops a running meeting (a wider scope needs a
 *  new consent, meeting.json); no device or agent clears the switches
 *  (voice.json `clearOnNone`). */
export async function syncVoiceMirror(exec: Exec, userId: string): Promise<void> {
  const [row] = await exec.select().from(userVoiceSettings).where(eq(userVoiceSettings.userId, userId)).limit(1)
  const next = await voiceAssistantOf(exec, userId, row?.agentId ?? null)
  if (row && row.agentId === next.agentId && row.voiceDeviceId === next.voiceDeviceId) return
  // Nothing stored and nothing to mirror (a member who never used voice).
  if (!row && next.agentId === null) return
  const now = new Date()
  const cleared = next.agentId === null || next.voiceDeviceId === null
  const moved = row !== undefined && (row.agentId !== next.agentId || row.voiceDeviceId !== next.voiceDeviceId)
  const set = {
    agentId: next.agentId,
    voiceDeviceId: next.voiceDeviceId,
    ...(cleared ? { wakeWord: false } : {}),
    ...(cleared || moved ? { meetingStartedAt: null } : {}),
    updatedAt: now,
  }
  await exec
    .insert(userVoiceSettings)
    .values({ userId, ...set })
    .onConflictDoUpdate({ target: userVoiceSettings.userId, set })
  const config = getSteerRelayConfig()
  if (!config) return
  for (const deviceId of new Set([row?.voiceDeviceId ?? null, next.voiceDeviceId])) {
    if (deviceId) void relayPostNudge(config, userId, deviceId).catch(() => {})
  }
}

