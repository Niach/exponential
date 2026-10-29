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
//
// NEW-57 (meeting mode, `fixtures/meeting.json`): the row also carries the
// Desktop audio choice and the running meeting; `startMeeting {consent: true}`
// is the ONLY writer of `meeting_started_at`, `stopMeeting` and choosing None
// for the device or the agent clear it.
//
// S-3 (voice.json): speak-back is always on and push to talk is a key
// binding (`pttShortcut`, `shortcutGrammar`, empty = off). `speakReplies` and
// `pushToTalk` stay in the answer for OLD clients only (true / a key bound)
// and in the input as legacy writes (pushToTalk false clears the key, true
// without one binds `ctrl+space`; speakReplies is ignored).
// FEED-163: `meetingAutoStopHours` (one of `autoStop.hoursValues`) bounds the
// meeting; `stopMeeting {reason}` (the device's auto-stop or restart) clears
// only the stamp it read and, when it did, posts `stopNote` once into the
// member's conversation with the voice agent (never waking it).

import { randomUUID } from "node:crypto"
import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNotNull, sql } from "drizzle-orm"

import { VOICE_DEFAULTS, VOICE_LANGUAGES, VOICE_LIMITS, VOICE_SHORTCUT_GRAMMAR, shortcutCanonical, wakePhraseError } from "@exp/domain-contract/voice"
import { callAvailability } from "@exp/domain-contract/voice-call"
import {
  autoStopHours,
  DESKTOP_AUDIO_VALUES,
  MEETING_AUTO_STOP,
  MEETING_LIMITS,
  meetingExpired,
  type MeetingStopReason,
  type AudioApp,
  type DesktopAudio,
} from "@exp/domain-contract/meeting"

import type { db as Database } from "@/db/connection"
import { agents, devices, teamMembers, userVoiceSettings } from "@/db/schema"
import { authedProcedure, router } from "@/lib/trpc"
import { getSteerRelayConfig, mintSteerTicket, relayPostNudge } from "@/lib/steer"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { callSpeechOf } from "@/lib/voice-call/availability"
import { meetingAgentIsPrivate, MEETING_SHARED_REFUSED, postMeetingStopNote } from "@/lib/sessions/meeting-access"
import { voiceAssistantOf } from "@/lib/agents/assistant"

type Db = typeof Database

export interface VoiceSettings {
  voiceDeviceId: string | null
  agentId: string | null
  /** S-3 legacy shim for old clients: always true. */
  speakReplies: boolean
  /** S-3 legacy shim for old clients: a key is bound. */
  pushToTalk: boolean
  /** S-3: `shortcutGrammar`'s canonical text; empty = push to talk off. */
  pttShortcut: string
  wakeWord: boolean
  wakePhrase: string
  language: string
  voiceFile: string
  voiceTranscript: string
  /** NEW-57: meeting.json `desktopAudioValues`. */
  desktopAudio: DesktopAudio
  /** NEW-57: the apps `include` picks / `exclude` leaves out. */
  desktopAudioApps: AudioApp[]
  /** NEW-57: ISO; null = no meeting runs. */
  meetingStartedAt: string | null
  /** FEED-163: meeting.json `autoStop.hoursValues`. */
  meetingAutoStopHours: number
}

/** voice.json `defaults` plus the meeting.json settings' defaults. */
export const VOICE_SETTINGS_DEFAULTS: VoiceSettings = {
  ...VOICE_DEFAULTS,
  speakReplies: true,
  pushToTalk: VOICE_DEFAULTS.pttShortcut !== ``,
  desktopAudio: `off`,
  desktopAudioApps: [],
  meetingStartedAt: null,
  meetingAutoStopHours: MEETING_AUTO_STOP.defaultHours,
}

const DESKTOP_AUDIO = DESKTOP_AUDIO_VALUES.map((option) => option.value) as [DesktopAudio, ...DesktopAudio[]]

/** S-3: the binding a legacy pushToTalk true binds when no key is. */
const VOICE_SHORTCUT_MAX = VOICE_SHORTCUT_GRAMMAR.maxLength
const LEGACY_PTT_SHORTCUT = `ctrl+space`
const LANGUAGE_VALUES = VOICE_LANGUAGES.map((language) => language.value) as [string, ...string[]]
const AUTO_STOP_HOURS = z
  .number()
  .int()
  .refine((hours) => MEETING_AUTO_STOP.hoursValues.includes(hours), `Pick one of the offered durations`)

// #NEW-71/#NEW-73: one line of text; control characters and newlines never.
const oneLine = (max: number, what: string) =>
  z
    .string()
    .max(max)
    .refine((value) => !/\p{Cc}/u.test(value), `The ${what} can't contain control characters`)

// NEW-57: a stored apps list is trusted only as far as its shape.
function storedApps(value: unknown): AudioApp[] {
  if (!Array.isArray(value)) return []
  return value
    .filter((app): app is AudioApp => typeof app?.id === `string` && typeof app?.name === `string`)
    .slice(0, MEETING_LIMITS.apps)
    .map((app) => ({ id: app.id, name: app.name }))
}

/** The member's settings, the fixture's defaults when no row exists. */
export async function voiceSettingsOf(database: Db, userId: string): Promise<VoiceSettings> {
  const [row] = await database
    .select()
    .from(userVoiceSettings)
    .where(eq(userVoiceSettings.userId, userId))
    .limit(1)
  // S-3 (agent-settings.json `assistant`): the voice device and agent are
  // the member's Assistant's, derived (the stored columns are its mirror).
  const voice = await voiceAssistantOf(database, userId, row?.agentId ?? null)
  if (!row) return withClearOnNone({ ...VOICE_SETTINGS_DEFAULTS, ...voice })
  const meetingAutoStopHours = autoStopHours(row.meetingAutoStopHours)
  // #NEW-71: a deleted agent nulls `agent_id` (FK set null) but leaves the
  // switches stored as they were; read them through clearOnNone so they never
  // come back on silently when a new agent is picked.
  return withClearOnNone({
    voiceDeviceId: voice.voiceDeviceId,
    agentId: voice.agentId,
    speakReplies: true,
    pushToTalk: row.pttShortcut !== ``,
    pttShortcut: row.pttShortcut,
    wakeWord: row.wakeWord,
    wakePhrase: row.wakePhrase,
    language: row.language,
    voiceFile: row.voiceFile,
    voiceTranscript: row.voiceTranscript,
    desktopAudio: (DESKTOP_AUDIO as string[]).includes(row.desktopAudio)
      ? (row.desktopAudio as DesktopAudio)
      : `off`,
    desktopAudioApps: storedApps(row.desktopAudioApps),
    // NEW-57 AUTO-STOP: an expired meeting (FEED-163: by the member's
    // hours) reads as none everywhere, even before the watchdog clears it.
    meetingStartedAt:
      row.meetingStartedAt &&
      !meetingExpired(new Date(row.meetingStartedAt).toISOString(), Date.now(), meetingAutoStopHours)
        ? new Date(row.meetingStartedAt).toISOString()
        : null,
    meetingAutoStopHours,
  })
}

/** The row's columns for a settings object (the meeting stamp as a Date;
 *  the S-3 legacy columns mirror what old clients read). */
function rowOf(settings: VoiceSettings) {
  return {
    ...settings,
    speakReplies: true,
    pushToTalk: settings.pttShortcut !== ``,
    meetingStartedAt: settings.meetingStartedAt ? new Date(settings.meetingStartedAt) : null,
  }
}

/** Write the member's row (upsert: a member without one gets it now).
 *  FEED-163: `meeting: false` leaves a stored `meeting_started_at` as it is
 *  (an expired stamp reads as null but is the watchdog's / stopMeeting's to
 *  clear, so its stop note is posted once). */
async function writeSettings(
  database: Db,
  userId: string,
  next: VoiceSettings,
  { meeting = true }: { meeting?: boolean } = {}
): Promise<void> {
  const now = new Date()
  const row = rowOf(next)
  const { meetingStartedAt: _stamp, ...withoutMeeting } = row
  await database
    .insert(userVoiceSettings)
    .values({ userId, ...row, updatedAt: now })
    .onConflictDoUpdate({
      target: userVoiceSettings.userId,
      set: { ...(meeting ? row : withoutMeeting), updatedAt: now },
    })
}

/** FEED-163: clear THE meeting started at `stamp` (a conditional update on
 *  the stamp read): true only for the call that actually cleared it, which
 *  alone posts `stopNote(reason, hours)` (no reason = a manual stop, none). */
export async function clearMeeting(
  database: Db,
  userId: string,
  stamp: Date,
  reason: MeetingStopReason | null
): Promise<boolean> {
  const cleared = await database
    .update(userVoiceSettings)
    .set({ meetingStartedAt: null, updatedAt: new Date() })
    .where(
      and(
        eq(userVoiceSettings.userId, userId),
        // Whole milliseconds, as JS read the stamp.
        sql`date_trunc('milliseconds', ${userVoiceSettings.meetingStartedAt}) = ${stamp.toISOString()}::timestamptz`
      )
    )
    .returning({
      agentId: userVoiceSettings.agentId,
      voiceDeviceId: userVoiceSettings.voiceDeviceId,
      hours: userVoiceSettings.meetingAutoStopHours,
    })
  const row = cleared[0]
  if (!row) return false
  if (row.voiceDeviceId) nudge(userId, row.voiceDeviceId)
  if (reason && row.agentId) {
    await postMeetingStopNote(database, userId, row.agentId, reason, autoStopHours(row.hours)).catch(
      (error: unknown) => console.error(`[meeting] stop note failed`, error)
    )
  }
  return true
}

/** voice.json `clearOnNone`: with no voice device or no agent the wake word
 *  is off; NEW-57: and no meeting runs. S-3: a bound key stays bound. */
export function withClearOnNone(settings: VoiceSettings): VoiceSettings {
  if (settings.voiceDeviceId === null || settings.agentId === null) {
    return { ...settings, wakeWord: false, meetingStartedAt: null }
  }
  return settings
}

/** S-3: the binding after an update, old clients' `pushToTalk` included. */
export function pttShortcutOf(before: string, input: { pttShortcut?: string; pushToTalk?: boolean }): string {
  const key = input.pttShortcut ?? before
  if (input.pushToTalk === false) return ``
  if (input.pushToTalk === true && key === ``) return LEGACY_PTT_SHORTCUT
  return key
}

/** NEW-57 (meeting.json): an update that CHANGES the meeting's scope (the
 *  voice device, the agent, Desktop audio or its apps) while one runs stops
 *  it: a wider scope needs a new consent. */
export function withMeetingScope(before: VoiceSettings, next: VoiceSettings): VoiceSettings {
  if (!next.meetingStartedAt) return next
  // The apps scope is a set: reordering the same apps widens nothing.
  const appIds = (settings: VoiceSettings) =>
    [...new Set(settings.desktopAudioApps.map((app) => app.id))].sort().join(`\n`)
  const widened =
    before.voiceDeviceId !== next.voiceDeviceId ||
    before.agentId !== next.agentId ||
    before.desktopAudio !== next.desktopAudio ||
    appIds(before) !== appIds(next)
  return widened ? { ...next, meetingStartedAt: null } : next
}

/** NEW-57 (meeting.json SHARING): after a device share, a running meeting
 *  whose voice agent is no longer private stops (a shared machine never
 *  holds a meeting; every batch would be refused while it kept listening). */
export async function stopMeetingUnlessPrivate(database: Db, userId: string): Promise<boolean> {
  const before = await voiceSettingsOf(database, userId)
  if (!before.meetingStartedAt || !before.agentId) return false
  if (await meetingAgentIsPrivate(database, userId, before.agentId)) return false
  await writeSettings(database, userId, { ...before, meetingStartedAt: null })
  if (before.voiceDeviceId) nudge(userId, before.voiceDeviceId)
  return true
}

/** The agent, when it is one of the member's teams' (#NEW-64 folded in):
 *  the agents a member can talk to. S-3: another member's Assistant is
 *  theirs alone (`loadAgentForUser`'s rule). */
async function memberAgentExists(database: Db, userId: string, agentId: string): Promise<boolean> {
  const memberTeams = database
    .select({ teamId: teamMembers.teamId })
    .from(teamMembers)
    .where(eq(teamMembers.userId, userId))
  const [agent] = await database
    .select({ role: agents.role, ownerId: agents.ownerId })
    .from(agents)
    .where(and(eq(agents.id, agentId), inArray(agents.teamId, memberTeams)))
    .limit(1)
  return agent !== undefined && (agent.role !== `assistant` || agent.ownerId === userId)
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
        // S-3: legacy writes of old clients (see the header).
        speakReplies: z.boolean().optional(),
        pushToTalk: z.boolean().optional(),
        pttShortcut: z.string().max(VOICE_SHORTCUT_MAX).optional(),
        wakeWord: z.boolean().optional(),
        wakePhrase: oneLine(VOICE_LIMITS.wakePhrase, `wake phrase`).optional(),
        language: z.enum(LANGUAGE_VALUES).optional(),
        voiceFile: oneLine(VOICE_LIMITS.voiceFile, `voice file`).optional(),
        voiceTranscript: oneLine(VOICE_LIMITS.voiceTranscript, `transcript`).optional(),
        // FEED-163: applies to a running meeting at once, no new consent.
        meetingAutoStopHours: AUTO_STOP_HOURS.optional(),
        // NEW-57: never `meetingStartedAt` (startMeeting / stopMeeting only).
        desktopAudio: z.enum(DESKTOP_AUDIO).optional(),
        desktopAudioApps: z
          .array(
            z.object({
              id: oneLine(256, `app id`).pipe(z.string().trim().min(1)),
              name: oneLine(MEETING_LIMITS.appName, `app name`).pipe(z.string().trim().min(1)),
            })
          )
          .max(MEETING_LIMITS.apps)
          .optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const db = ctx.db
      // #NEW-56: a short wake phrase would fire on every segment.
      const tooShort = input.wakePhrase === undefined ? null : wakePhraseError(input.wakePhrase)
      if (tooShort) throw new TRPCError({ code: `BAD_REQUEST`, message: tooShort })
      // S-3: only a binding already in its canonical form is stored.
      if (input.pttShortcut !== undefined && shortcutCanonical(input.pttShortcut) !== input.pttShortcut) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `That key can't be a push-to-talk binding` })
      }
      const before = await voiceSettingsOf(db, userId)
      // S-3: the voice device is the Assistant's (Runs on): an old client's
      // `voiceDeviceId` is ignored; `agentId` may pick which of the member's
      // Assistants (one per team) holds the voice, nothing else.
      const { voiceDeviceId: _legacyDevice, agentId: pickedAgent, ...changes } = input
      const voice =
        pickedAgent === undefined || pickedAgent === null
          ? { voiceDeviceId: before.voiceDeviceId, agentId: before.agentId }
          : await voiceAssistantOf(db, userId, pickedAgent)
      if (pickedAgent && voice.agentId !== pickedAgent) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `That agent isn't your Assistant` })
      }
      const { speakReplies: _speakReplies, pushToTalk: _pushToTalk, ...writes } = changes
      const pttShortcut = pttShortcutOf(before.pttShortcut, input)
      const next = withMeetingScope(before, withClearOnNone({
        ...before,
        ...Object.fromEntries(Object.entries(writes).filter(([, value]) => value !== undefined)),
        ...voice,
        speakReplies: true,
        pttShortcut,
        pushToTalk: pttShortcut !== ``,
        wakePhrase: input.wakePhrase === undefined ? before.wakePhrase : input.wakePhrase.trim(),
        voiceFile: input.voiceFile === undefined ? before.voiceFile : input.voiceFile.trim(),
        voiceTranscript: input.voiceTranscript === undefined ? before.voiceTranscript : input.voiceTranscript.trim(),
        // NEW-57: one entry per app id, the first wins.
        desktopAudioApps:
          input.desktopAudioApps === undefined
            ? before.desktopAudioApps
            : input.desktopAudioApps.filter(
                (app, index, apps) => apps.findIndex((other) => other.id === app.id) === index
              ),
      } as VoiceSettings))
      // Only a scope / None clear writes the meeting stamp (FEED-163).
      await writeSettings(db, userId, next, { meeting: before.meetingStartedAt !== next.meetingStartedAt })
      // The old and the new voice device pick the change up at once.
      for (const deviceId of new Set([before.voiceDeviceId, next.voiceDeviceId])) {
        if (deviceId) nudge(userId, deviceId)
      }
      // FEED-163: a shorter Stop after applies at once: a meeting it already
      // outlived reads as none (the watchdog / the device clear the stamp).
      return next.meetingStartedAt && meetingExpired(next.meetingStartedAt, Date.now(), next.meetingAutoStopHours)
        ? { ...next, meetingStartedAt: null }
        : next
    }),

  // NEW-57: THE consent dialog's Start. Refused without `consent: true` (the
  // zod literal), without a voice device and an agent, and with
  // `copy.sharedRefused` unless that agent is private (`meetingAgentIsPrivate`:
  // the caller's own unshared device, an unshared conversation); a running meeting
  // keeps its start (a second Start from another client is a no-op).
  startMeeting: authedProcedure
    .input(z.object({ consent: z.literal(true) }))
    .mutation(async ({ ctx }) => {
      const userId = ctx.session.user.id
      const before = await voiceSettingsOf(ctx.db, userId)
      if (!before.voiceDeviceId || !before.agentId) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `Pick a voice device and an agent first` })
      }
      // Never into a team-visible conversation or onto a shared machine.
      if (!(await meetingAgentIsPrivate(ctx.db, userId, before.agentId))) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: MEETING_SHARED_REFUSED })
      }
      if (before.meetingStartedAt) return before
      // FEED-163: a stored stamp that reads as expired is the old meeting's
      // auto-stop: cleared (its stop note posted) before the new one starts.
      const [stored] = await ctx.db
        .select({ meetingStartedAt: userVoiceSettings.meetingStartedAt })
        .from(userVoiceSettings)
        .where(and(eq(userVoiceSettings.userId, userId), isNotNull(userVoiceSettings.meetingStartedAt)))
        .limit(1)
      if (stored?.meetingStartedAt) {
        await clearMeeting(ctx.db, userId, new Date(stored.meetingStartedAt), `autoStop`)
      }
      const next: VoiceSettings = { ...before, meetingStartedAt: new Date().toISOString() }
      await writeSettings(ctx.db, userId, next)
      nudge(userId, before.voiceDeviceId)
      return next
    }),

  // NEW-57: Meeting mode off (the row's toggle, the tray): at once, no dialog.
  // FEED-163: `reason` = the voice device stopped it itself (`autoStop`, or
  // `restart` = the IDE came back to a meeting it listened to); it clears the
  // STORED stamp even when it already reads as expired, and the call that
  // cleared it posts the stop note (a manual stop posts none).
  stopMeeting: authedProcedure
    // `nullish`: the desktop sends a `null` body for "no reason".
    .input(z.object({ reason: z.enum([`autoStop`, `restart`]).optional() }).nullish())
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const [stored] = await ctx.db
        .select({ meetingStartedAt: userVoiceSettings.meetingStartedAt })
        .from(userVoiceSettings)
        .where(and(eq(userVoiceSettings.userId, userId), isNotNull(userVoiceSettings.meetingStartedAt)))
        .limit(1)
      if (stored?.meetingStartedAt) {
        await clearMeeting(ctx.db, userId, new Date(stored.meetingStartedAt), input?.reason ?? null)
      }
      return voiceSettingsOf(ctx.db, userId)
    }),

  // NEW-55: start a STREAMED call with one agent (phones V5, web V6). Refused
  // with the availability reason (voice-call.json `copy.reasons`) unless the
  // member's voice device is picked, the device is online and has reported
  // its `speech` doctor row (S-3: a downloading voice rings until ready); `busy` is the relay's to say
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
