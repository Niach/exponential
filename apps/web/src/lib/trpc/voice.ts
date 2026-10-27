// NEW-67 (voice plan v3, V1): the member's ONE voice settings row
// (`fixtures/voice.json`). Server-only like the notification prefs: read with
// `get`, written with `update`, handed to the member's devices on every
// heartbeat (devices.heartbeat `voice`). A write nudges the old and the new
// voice device so a switch lands at once instead of on the next beat.
//
// NEW-55 (voice V4, caller mode): `startCall` checks the call's availability
// (`fixtures/voice-call.json` `callAvailability`) and mints the CALLER's relay
// ticket; `callHostTicket` mints the voice device's answer. The relay pairs
// them (apps/steer-relay hub.ts, call rooms) and checks the host's account.

import { randomUUID } from "node:crypto"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray } from "drizzle-orm"

import { VOICE_DEFAULTS, VOICE_LANGUAGES, VOICE_LIMITS, VOICE_SHORTCUTS } from "@exp/domain-contract/voice"
import { callAvailability } from "@exp/domain-contract/voice-call"

import type { db as Database } from "@/db/connection"
import { agents, devices, teamMembers, userVoiceSettings } from "@/db/schema"
import { authedProcedure, router } from "@/lib/trpc"
import { getSteerRelayConfig, mintSteerTicket, relayPostNudge } from "@/lib/steer"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { callSpeechOf } from "@/lib/voice-call/availability"

type Db = typeof Database

export interface VoiceSettings {
  voiceDeviceId: string | null
  agentId: string | null
  speakReplies: boolean
  pushToTalk: boolean
  pttShortcut: string
  wakeWord: boolean
  wakePhrase: string
  language: string
  voiceFile: string
  voiceTranscript: string
}

const SHORTCUT_VALUES = VOICE_SHORTCUTS.map((shortcut) => shortcut.value) as [string, ...string[]]
const LANGUAGE_VALUES = VOICE_LANGUAGES.map((language) => language.value) as [string, ...string[]]

// #NEW-71/#NEW-73: one line of text; control characters and newlines never.
const oneLine = (max: number, what: string) =>
  z
    .string()
    .max(max)
    .refine((value) => !/\p{Cc}/u.test(value), `The ${what} can't contain control characters`)

/** The member's settings, the fixture's defaults when no row exists. */
export async function voiceSettingsOf(database: Db, userId: string): Promise<VoiceSettings> {
  const [row] = await database
    .select()
    .from(userVoiceSettings)
    .where(eq(userVoiceSettings.userId, userId))
    .limit(1)
  if (!row) return { ...VOICE_DEFAULTS }
  // #NEW-71: a deleted agent nulls `agent_id` (FK set null) but leaves the
  // switches stored as they were; read them through clearOnNone so they never
  // come back on silently when a new agent is picked.
  return withClearOnNone({
    voiceDeviceId: row.voiceDeviceId,
    agentId: row.agentId,
    speakReplies: row.speakReplies,
    pushToTalk: row.pushToTalk,
    pttShortcut: row.pttShortcut,
    wakeWord: row.wakeWord,
    wakePhrase: row.wakePhrase,
    language: row.language,
    voiceFile: row.voiceFile,
    voiceTranscript: row.voiceTranscript,
  })
}

/** voice.json `clearOnNone`: with no voice device or no agent, nothing
 *  listens and nothing speaks. */
export function withClearOnNone(settings: VoiceSettings): VoiceSettings {
  if (settings.voiceDeviceId === null || settings.agentId === null) {
    return { ...settings, speakReplies: false, pushToTalk: false, wakeWord: false }
  }
  return settings
}

/** The agent, when it is one of the member's teams' (#NEW-64 folded in):
 *  the agents a member can talk to. */
async function memberAgentExists(database: Db, userId: string, agentId: string): Promise<boolean> {
  const memberTeams = database
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))
  const [agent] = await database
    .select({ id: agents.id })
    .from(agents)
    .where(and(eq(agents.id, agentId), inArray(agents.teamId, memberTeams)))
    .limit(1)
  return agent !== undefined
}

// NEW-55: `callSpeechOf` lives beside the web caller's availability
// (lib/voice-call/availability.ts) so the telephone reads the doctor row the
// same way this refusal does.
export { callSpeechOf }

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
        wakePhrase: oneLine(64, `wake phrase`).optional(),
        language: z.enum(LANGUAGE_VALUES).optional(),
        voiceFile: oneLine(VOICE_LIMITS.voiceFile, `voice file`).optional(),
        voiceTranscript: oneLine(VOICE_LIMITS.voiceTranscript, `transcript`).optional(),
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
      if (input.agentId && !(await memberAgentExists(db, userId, input.agentId))) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `That agent isn't in your teams` })
      }
      const next = withClearOnNone({
        ...before,
        ...Object.fromEntries(Object.entries(input).filter(([, value]) => value !== undefined)),
        wakePhrase: input.wakePhrase === undefined ? before.wakePhrase : input.wakePhrase.trim(),
        voiceFile: input.voiceFile === undefined ? before.voiceFile : input.voiceFile.trim(),
        voiceTranscript: input.voiceTranscript === undefined ? before.voiceTranscript : input.voiceTranscript.trim(),
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

  // NEW-55: start a STREAMED call with one agent (phones V5, web V6). Refused
  // with the availability reason (voice-call.json `copy.reasons`) unless the
  // member's voice device is picked, Speak replies is on, the device is
  // online and its `speech` doctor row is ok; `busy` is the relay's to say
  // (it alone knows a running call). Answers the CALLER's relay ticket.
  // (`call` is a reserved word in tRPC routers, hence `startCall`.)
  startCall: authedProcedure
    .input(
      z.object({
        agentId: z.string().uuid(),
        platform: z.enum([`phone`, `web`, `desktop`]),
        /** The calling client's own steer deviceId (desktops): whether it
         *  IS the voice device (`notHere` otherwise). */
        deviceId: z.string().min(1).max(128).optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const db = ctx.db
      if (!(await memberAgentExists(db, userId, input.agentId))) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `That agent isn't in your teams` })
      }
      const settings = await voiceSettingsOf(db, userId)
      const [device] = settings.voiceDeviceId
        ? await db
            .select({ label: devices.label, lastSeenAt: devices.lastSeenAt, doctor: devices.doctor })
            .from(devices)
            .where(and(eq(devices.userId, userId), eq(devices.deviceId, settings.voiceDeviceId)))
            .limit(1)
        : []
      const availability = callAvailability({
        platform: input.platform,
        // A voice device whose row is gone reads as none picked.
        device: device ? device.label : null,
        speakReplies: settings.speakReplies,
        online: device ? deviceRowIsOnline(device.lastSeenAt, new Date()) : false,
        here: input.deviceId !== undefined && input.deviceId === settings.voiceDeviceId,
        speech: device ? callSpeechOf(device.doctor) : null,
        busy: false,
      })
      if (!availability.enabled) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: availability.reason ?? `` })
      }
      const config = getSteerRelayConfig()
      if (!config) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      const callId = randomUUID()
      const minted = mintSteerTicket(config, {
        kind: `caller`,
        userId,
        callId,
        agentId: input.agentId,
        deviceId: settings.voiceDeviceId!,
      })
      if (`disabled` in minted) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      return { callId, ticket: minted.ticket, url: minted.url, relayUrl: config.url }
    }),

  // NEW-55: the voice device answers a `call_ring`. Minted for the member
  // alone; the relay admits it only into a ringing call on a device this
  // account owns, so the callId is not a grant of anything.
  callHostTicket: authedProcedure
    .input(z.object({ callId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => {
      const config = getSteerRelayConfig()
      if (!config) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      // NEW-55 review: the ticket names the member's voice device; the relay
      // admits it only into that device's call room.
      const settings = await voiceSettingsOf(ctx.db, ctx.session.user.id)
      if (!settings.voiceDeviceId) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `No voice device` })
      }
      const minted = mintSteerTicket(config, {
        kind: `call_host`,
        userId: ctx.session.user.id,
        callId: input.callId,
        deviceId: settings.voiceDeviceId,
      })
      if (`disabled` in minted) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      return { callId: input.callId, ticket: minted.ticket, url: minted.url, relayUrl: config.url }
    }),
})
