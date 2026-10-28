import { TRPCError } from "@trpc/server"
import { and, asc, desc, eq } from "drizzle-orm"
import {
  ASSISTANT,
  ASSISTANT_COPY,
  assistantScheduleOf,
  assistantScheduleTriggers,
  type AgentTriggerLike,
} from "@exp/domain-contract/agent-settings"
import type { AgentTrigger } from "@exp/db-schema/domain"
import { db } from "@/db/connection"
import { agents, devices, userVoiceSettings, type Agent } from "@/db/schema"
import { getTeamMember } from "@/lib/auth/membership"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

// S-3 (agent-settings.json `assistant`): every member's ONE Assistant per
// team (`agents.role = assistant`, unique on (team_id, owner_id)). Created
// on first use on the member's own newest desktop (else their newest
// device); owned and edited by that member only; never deleted. Its device
// is the member's VOICE device: `user_voice_settings.voice_device_id` /
// `agent_id` stay as a server-kept MIRROR of the voice Assistant (old
// clients, the heartbeat and the voice runtime read them unchanged).

type Exec = Pick<typeof db, `select` | `insert` | `update`>

/** The member's Assistant in a team, if it exists. */
export async function findAssistant(exec: Exec, teamId: string, ownerId: string): Promise<Agent | null> {
  const [row] = await exec
    .select()
    .from(agents)
    .where(and(eq(agents.teamId, teamId), eq(agents.ownerId, ownerId), eq(agents.role, `assistant`)))
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
 *  device to run it on. */
export async function ensureAssistant(userId: string, teamId: string, exec: Exec = db): Promise<Agent | null> {
  if (!(await getTeamMember(userId, teamId))) {
    throw new TRPCError({ code: `FORBIDDEN`, message: `Not a member of this team` })
  }
  const existing = await findAssistant(exec, teamId, userId)
  if (existing) return existing
  const deviceId = await assistantDevice(exec, userId)
  if (!deviceId) return null
  const [created] = await exec
    .insert(agents)
    .values({
      teamId,
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
  const assistant = created ?? (await findAssistant(exec, teamId, userId))
  if (assistant) await syncVoiceMirror(exec, userId)
  return assistant
}

/** `agents.update` on an Assistant: exactly one schedule preset, never an
 *  event (agent-settings.json `assistant.scheduleCases`). */
export function assertAssistantTriggers(triggers: AgentTriggerLike[]): void {
  if (assistantScheduleOf(triggers) === null) {
    throw new TRPCError({ code: `BAD_REQUEST`, message: ASSISTANT_COPY.scheduleRefused })
  }
}

/** The voice Assistant + device a member's voice runs on: the stored
 *  `agent_id` while it is one of their Assistants, else their oldest
 *  Assistant; the device = its device when that is one of their own
 *  desktops (voice.json: phones and servers never run the voice). */
export async function voiceAssistantOf(
  exec: Exec,
  userId: string,
  storedAgentId: string | null
): Promise<{ agentId: string | null; voiceDeviceId: string | null }> {
  const mine = await exec
    .select({ id: agents.id, deviceId: agents.deviceId })
    .from(agents)
    .where(and(eq(agents.ownerId, userId), eq(agents.role, `assistant`)))
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
  const now = new Date()
  const cleared = next.agentId === null || next.voiceDeviceId === null
  const moved = row !== undefined && (row.agentId !== next.agentId || row.voiceDeviceId !== next.voiceDeviceId)
  const set = {
    agentId: next.agentId,
    voiceDeviceId: next.voiceDeviceId,
    ...(cleared ? { speakReplies: false, pushToTalk: false, wakeWord: false } : {}),
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

