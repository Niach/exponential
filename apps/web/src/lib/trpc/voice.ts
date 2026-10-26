// NEW-67 (voice plan v3, V1): the member's ONE voice settings row
// (`fixtures/voice.json`). Server-only like the notification prefs: read with
// `get`, written with `update`, handed to the member's devices on every
// heartbeat (devices.heartbeat `voice`). A write nudges the old and the new
// voice device so a switch lands at once instead of on the next beat.

import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"

import { VOICE_DEFAULTS, VOICE_SHORTCUTS } from "@exp/domain-contract/voice"

import type { db as Database } from "@/db/connection"
import { agents, devices, teamMembers, userVoiceSettings } from "@/db/schema"
import { authedProcedure, router } from "@/lib/trpc"
import { getSteerRelayConfig, relayPostNudge } from "@/lib/steer"

type Db = typeof Database

export interface VoiceSettings {
  voiceDeviceId: string | null
  agentId: string | null
  speakReplies: boolean
  pushToTalk: boolean
  pttShortcut: string
  wakeWord: boolean
  wakePhrase: string
}

const SHORTCUT_VALUES = VOICE_SHORTCUTS.map((shortcut) => shortcut.value) as [string, ...string[]]

/** The member's settings, the fixture's defaults when no row exists. */
export async function voiceSettingsOf(database: Db, userId: string): Promise<VoiceSettings> {
  const [row] = await database
    .select()
    .from(userVoiceSettings)
    .where(eq(userVoiceSettings.userId, userId))
    .limit(1)
  if (!row) return { ...VOICE_DEFAULTS }
  return {
    voiceDeviceId: row.voiceDeviceId,
    agentId: row.agentId,
    speakReplies: row.speakReplies,
    pushToTalk: row.pushToTalk,
    pttShortcut: row.pttShortcut,
    wakeWord: row.wakeWord,
    wakePhrase: row.wakePhrase,
  }
}

/** voice.json `clearOnNone`: with no voice device or no agent, nothing
 *  listens and nothing speaks. */
export function withClearOnNone(settings: VoiceSettings): VoiceSettings {
  if (settings.voiceDeviceId === null || settings.agentId === null) {
    return { ...settings, speakReplies: false, pushToTalk: false, wakeWord: false }
  }
  return settings
}

// Best-effort like devices.ts `nudgeDevice` (not imported: devices.ts reads
// `voiceSettingsOf` for the heartbeat, and a cycle would be one too many).
function nudge(ownerId: string, deviceId: string): void {
  const config = getSteerRelayConfig()
  if (!config) return
  void relayPostNudge(config, ownerId, deviceId).catch(() => {})
}

export const voiceRouter = router({
  get: authedProcedure.query(({ ctx }) => voiceSettingsOf(ctx.db, ctx.session.user.id)),

  update: authedProcedure
    .input(
      z.object({
        voiceDeviceId: z.string().min(1).max(128).nullable().optional(),
        agentId: z.string().uuid().nullable().optional(),
        speakReplies: z.boolean().optional(),
        pushToTalk: z.boolean().optional(),
        pttShortcut: z.enum(SHORTCUT_VALUES).optional(),
        wakeWord: z.boolean().optional(),
        wakePhrase: z.string().max(64).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const db = ctx.db
      const before = await voiceSettingsOf(db, userId)
      // The voice device: one of the member's OWN desktops (phones and
      // servers never run the voice; `voice.json` voiceDeviceOptions).
      if (input.voiceDeviceId) {
        const [device] = await db
          .select({ kind: devices.kind })
          .from(devices)
          .where(and(eq(devices.userId, userId), eq(devices.deviceId, input.voiceDeviceId)))
          .limit(1)
        if (!device || device.kind !== `desktop`) {
          throw new TRPCError({ code: `BAD_REQUEST`, message: `That device can't run the voice` })
        }
      }
      // The agent: one of the member's teams' (#NEW-64 folded in).
      if (input.agentId) {
        const memberTeams = db
          .select({ teamId: teamMembers.teamId })
          .from(teamMembers)
          .where(eq(teamMembers.userId, userId))
        const [agent] = await db
          .select({ id: agents.id })
          .from(agents)
          .where(and(eq(agents.id, input.agentId), inArray(agents.teamId, memberTeams)))
          .limit(1)
        if (!agent) {
          throw new TRPCError({ code: `BAD_REQUEST`, message: `That agent isn't in your teams` })
        }
      }
      const next = withClearOnNone({
        ...before,
        ...Object.fromEntries(Object.entries(input).filter(([, value]) => value !== undefined)),
        wakePhrase: input.wakePhrase === undefined ? before.wakePhrase : input.wakePhrase.trim(),
      } as VoiceSettings)
      const now = new Date()
      await db
        .insert(userVoiceSettings)
        .values({ userId, ...next, updatedAt: now })
        .onConflictDoUpdate({ target: userVoiceSettings.userId, set: { ...next, updatedAt: now } })
      // The old and the new voice device pick the change up at once.
      for (const deviceId of new Set([before.voiceDeviceId, next.voiceDeviceId])) {
        if (deviceId) nudge(userId, deviceId)
      }
      return next
    }),
})
