// NEW-67 (voice plan v3, V1): the member's ONE voice settings row
// (`fixtures/voice.json`). Server-only like the notification prefs: read with
// `get`, written with `update`, handed to the member's devices on every
// heartbeat (devices.heartbeat `voice`). A write nudges the old and the new
// voice device so a switch lands at once instead of on the next beat.
//
// NEW-55 (voice V4, caller mode): `startCall` picks where the call speaks
// (`fixtures/voice-call.json` `callTarget`) and mints the CALLER's relay
// ticket; `callHostTicket` mints the voice device's answer. The relay pairs
// them (apps/steer-relay hub.ts, call rooms) and checks the host's account.
//
// NEW-57 (meeting mode, `fixtures/meeting.json`): the row also carries the
// Desktop audio choice and the running meeting; `startMeeting {consent: true}`
// is the ONLY writer of `meeting_started_at`, `stopMeeting` and choosing None
// for the device or the agent clear it.
//
// S-3 (voice.json): speak-back is always on and push to talk is a key
// binding (`pttShortcut`, `shortcutGrammar`, empty = off).
// FEED-163: `meetingAutoStopHours` (one of `autoStop.hoursValues`) bounds the
// meeting; `stopMeeting {reason}` (the device's auto-stop or restart) clears
// only the stamp it read and, when it did, posts `stopNote` once into the
// member's conversation with the voice agent (never waking it).

import { z } from "zod"
import { TRPCError } from "@trpc/server"
import { and, eq, inArray, isNotNull, or, sql } from "drizzle-orm"

import { VOICE_DEFAULTS, VOICE_LANGUAGES, VOICE_LIMITS, VOICE_SHORTCUT_GRAMMAR, shortcutCanonical, wakePhraseError } from "@exp/etalk/voice"
import { callTarget, type CallTargetDevice } from "@exp/etalk/voice-call"
import {
  autoStopHours,
  DESKTOP_AUDIO_VALUES,
  MEETING_AUTO_STOP,
  MEETING_LIMITS,
  meetingExpired,
  type MeetingStopReason,
  type AudioApp,
  type DesktopAudio,
} from "@exp/etalk/meeting"

import type { db as Database } from "@/db/connection"
import { agents, devices, teamMembers, teams, userVoiceSettings } from "@/db/schema"
import { authedProcedure, generateTxId, router } from "@/lib/trpc"
import { conversationInTx, loadAgentForUser } from "@/lib/agents/conversation"
import { createTreeKeyInTx, loadRoot, lockTreeKeys } from "@/lib/sessions/tree-keys"
import { getSteerRelayConfig, mintSteerTicket, relayPostNudge } from "@/lib/steer"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { callSpeechOf } from "@/lib/voice-call/availability"
import { mintCallId, verifyCallId } from "@/lib/voice-call-id"
import { meetingAgentIsPrivate, MEETING_SHARED_REFUSED, postMeetingStopNote } from "@/lib/sessions/meeting-access"
import { voiceAssistantOf } from "@/lib/agents/assistant"
import { endCallSession, startCallSession } from "@/lib/sessions/call"

type Db = typeof Database

export interface VoiceSettings {
  voiceDeviceId: string | null
  agentId: string | null
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
  desktopAudio: `off`,
  desktopAudioApps: [],
  meetingStartedAt: null,
  meetingAutoStopHours: MEETING_AUTO_STOP.defaultHours,
}

const DESKTOP_AUDIO = DESKTOP_AUDIO_VALUES.map((option) => option.value) as [DesktopAudio, ...DesktopAudio[]]

const VOICE_SHORTCUT_MAX = VOICE_SHORTCUT_GRAMMAR.maxLength
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

/** The row's columns for a settings object (the meeting stamp as a Date). */
function rowOf(settings: VoiceSettings) {
  return {
    ...settings,
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

/** The agent's team is one of the member's: joined, or (#EXP-1984) their
 *  personal team, where their Assistant lives. */
function inMemberTeams(database: Db, userId: string) {
  return or(
    inArray(agents.teamId, database.select({ teamId: teamMembers.teamId }).from(teamMembers).where(eq(teamMembers.userId, userId))),
    inArray(
      agents.teamId,
      database.select({ id: teams.id }).from(teams).where(and(eq(teams.ownerId, userId), eq(teams.kind, `personal`)))
    )
  )
}

/** The agent, when it is one of the member's teams' (#NEW-64 folded in):
 *  the agents a member can talk to, with its Runs on device. S-3: another
 *  member's Assistant is theirs alone (`loadAgentForUser`'s rule). */
async function memberAgent(database: Db, userId: string, agentId: string): Promise<{ deviceId: string | null } | null> {
  const [agent] = await database
    .select({ role: agents.role, ownerId: agents.ownerId, deviceId: agents.deviceId })
    .from(agents)
    .where(and(eq(agents.id, agentId), inMemberTeams(database, userId)))
    .limit(1)
  if (agent === undefined || (agent.role === `assistant` && agent.ownerId !== userId)) return null
  return { deviceId: agent.deviceId ?? null }
}

/** Every agent the member may call (`memberAgent`'s rule, as a set). */
async function memberAgentIds(database: Db, userId: string): Promise<string[]> {
  const rows = await database
    .select({ id: agents.id, role: agents.role, ownerId: agents.ownerId })
    .from(agents)
    .where(inMemberTeams(database, userId))
  return rows.filter((agent) => !(agent.role === `assistant` && agent.ownerId !== userId)).map((agent) => agent.id)
}

/** #EXP-1941: the mode of a call the SERVER minted for this member and
 *  device (lib/voice-call-id.ts), or null for a ring it did not make. A host
 *  that names the ring's agent must name the id's agent, in both modes; the
 *  sealed candidate is tried only for a host that names its own device and
 *  that agent. A host that names no agent (a build before it sealed) gets
 *  the clear candidate only, tried for each agent the member may call. */
export async function vouchedCall(
  database: Db,
  userId: string,
  call: { callId: string; deviceId: string; namedDevice: boolean; agentId?: string }
): Promise<{ sealed: boolean; tried: string } | { sealed: null; tried: string }> {
  const binding = { userId, deviceId: call.deviceId }
  if (call.agentId !== undefined) {
    const named = { ...binding, agentId: call.agentId }
    if (call.namedDevice && verifyCallId(call.callId, { ...named, sealed: true })) return { sealed: true, tried: `sealed` }
    if (verifyCallId(call.callId, { ...named, sealed: false })) return { sealed: false, tried: `clear` }
    return { sealed: null, tried: call.namedDevice ? `sealed and clear for the named agent` : `clear for the named agent` }
  }
  const agentIds = await memberAgentIds(database, userId)
  const match = agentIds.some((agentId) => verifyCallId(call.callId, { ...binding, agentId, sealed: false }))
  return match ? { sealed: false, tried: `clear` } : { sealed: null, tried: `clear for ${agentIds.length} agents (the host named none)` }
}

/** EXP-1857 (voice-call.json `callTarget`): the member's OWN desktops,
 *  newest first, as a call's possible speech devices (EXP-1858: and their
 *  own servers, a candidate only as the agent's device). A meeting runs on
 *  the voice device only. */
export async function callDevicesOf(
  database: Db,
  userId: string,
  settings: Pick<VoiceSettings, `voiceDeviceId` | `meetingStartedAt`>,
  now: Date
): Promise<{ desktops: CallTargetDevice[]; servers: CallTargetDevice[] }> {
  const rows = await database
    .select({ deviceId: devices.deviceId, kind: devices.kind, label: devices.label, lastSeenAt: devices.lastSeenAt, doctor: devices.doctor })
    .from(devices)
    .where(and(eq(devices.userId, userId), inArray(devices.kind, [`desktop`, `server`])))
  const sorted = [...rows].sort((a, b) => new Date(b.lastSeenAt ?? 0).getTime() - new Date(a.lastSeenAt ?? 0).getTime())
  const of = (kind: string) =>
    sorted
      .filter((row) => row.kind === kind)
      .map((row) => ({
        id: row.deviceId,
        name: row.label ?? row.deviceId,
        online: deviceRowIsOnline(row.lastSeenAt, now),
        speech: callSpeechOf(row.doctor),
        meeting: Boolean(settings.meetingStartedAt) && row.deviceId === settings.voiceDeviceId,
      }))
  return { desktops: of(`desktop`), servers: of(`server`) }
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
        // Stays non-strict: floor clients still send the retired `voiceDeviceId`.
        agentId: z.string().uuid().nullable().optional(),
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
      // S-3: the voice device is the Assistant's (Runs on); an unknown key is
      // stripped. `agentId` may pick which of the member's Assistants (one per
      // team) holds the voice, nothing else.
      const { agentId: pickedAgent, ...changes } = input
      const voice =
        pickedAgent === undefined || pickedAgent === null
          ? { voiceDeviceId: before.voiceDeviceId, agentId: before.agentId }
          : await voiceAssistantOf(db, userId, pickedAgent)
      if (pickedAgent && voice.agentId !== pickedAgent) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `That agent isn't your Assistant` })
      }
      const next = withMeetingScope(before, withClearOnNone({
        ...before,
        ...Object.fromEntries(Object.entries(changes).filter(([, value]) => value !== undefined)),
        ...voice,
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

  // NEW-55: start a STREAMED call with one agent (phones V5, web V6).
  // EXP-1857: the call rings the AGENT's device when it is one of the
  // member's desktops and can speak, else falls back (`callTarget`: the
  // desktop the caller is at, then the newest that speaks); refused with
  // the target's reason (voice-call.json `copy.reasons`). `busy` is the
  // relay's to say (it alone knows a running call). Answers the CALLER's
  // relay ticket and the target it rings. A desktop whose target is itself
  // runs the call locally and never asks. (`call` is a reserved word in
  // tRPC routers, hence `startCall`.)
  startCall: authedProcedure
    .input(
      z.object({
        agentId: z.string().uuid(),
        platform: z.enum([`phone`, `web`, `desktop`]),
        /** The calling client's own steer deviceId (desktops): the desktop
         *  you're at, the first fallback. */
        deviceId: z.string().min(1).max(128).optional(),
        /** #EXP-1941: the caller can seal this call (it holds a key holder). */
        seal: z.boolean().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      const db = ctx.db
      const agent = await memberAgent(db, userId, input.agentId)
      if (!agent) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `That agent isn't in your teams` })
      }
      const settings = await voiceSettingsOf(db, userId)
      const target = callTarget({
        here: input.platform === `desktop` ? (input.deviceId ?? null) : null,
        agentDevice: agent.deviceId,
        ...(await callDevicesOf(db, userId, settings, new Date())),
        busy: false,
      })
      if (!target.enabled || !target.device) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: target.reason ?? `` })
      }
      if (target.local) {
        throw new TRPCError({ code: `BAD_REQUEST`, message: `This call runs on this desktop` })
      }
      const config = getSteerRelayConfig()
      if (!config) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      // #EXP-1941: ONE boolean for the id and for the answer, so the host is
      // never told another mode than the caller. It will be the caller's
      // claim (`input.seal`) AND the target's `call-seal` cap; until the
      // hosts are released and sealing is turned on it is the constant false.
      const sealed = false
      // The id vouches for this member, device, agent and mode
      // (lib/voice-call-id.ts). Without the secret it could vouch for
      // nothing: no call.
      const callId = mintCallId({ userId, deviceId: target.device, agentId: input.agentId, sealed })
      if (!callId) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      const minted = mintSteerTicket(config, {
        kind: `caller`,
        userId,
        callId,
        agentId: input.agentId,
        deviceId: target.device,
      })
      if (`disabled` in minted) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      return {
        callId,
        ticket: minted.ticket,
        url: minted.url,
        relayUrl: config.url,
        deviceId: target.device,
        device: target.name ?? target.device,
        fallback: target.fallback,
        seal: sealed,
      }
    }),

  // #EXP-1941: THE CHAT OF A FIRST CALL. A sealed call is keyed with the
  // chat's tree secret, and a first call to an agent has no chat yet: the
  // answering host creates it, with its key, before it sends its hello. A
  // text-less `sessions.send` is refused by its schema, hence this op. In
  // this order: the call id must be a SEALED one the server minted for this
  // member, this device and THIS agent (the verify `callHostTicket` uses),
  // so it runs only inside a call the server vouched for; then
  // `loadAgentForUser`, the rule a send applies first; then the
  // conversation and, ONLY when its root has no key, the key, in one
  // transaction under the tree's key lock. A root somebody keyed a moment
  // before (a first message, a second ring) answers its STORED key, never
  // the stale-tree conflict: the host keeps its own secret only when the
  // answer names its own public key. It posts no message. A new row is
  // `queued` and its device is nudged, exactly as a first message does it.
  // Key writes stay bound to the caller's own install and to readers
  // (`createTreeKeyInTx`). Unreachable until `startCall` mints sealed ids.
  callConversation: authedProcedure
    .input(
      z.object({
        agentId: z.string().uuid(),
        callId: z.string().uuid(),
        deviceId: z.string().min(1).max(128),
        seal: z.object({
          installId: z.string().uuid(),
          publicKey: z.string().length(43),
          epoch: z.literal(1),
          wraps: z.array(z.object({ installId: z.string().uuid(), wrapped: z.string().min(1).max(256) })).min(1).max(200),
        }),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      if (!verifyCallId(input.callId, { userId, deviceId: input.deviceId, agentId: input.agentId, sealed: true })) {
        console.warn(`[voice] callConversation refused: member=${userId} device=${input.deviceId}: no sealed call the server minted`)
        throw new TRPCError({ code: `NOT_FOUND`, message: `No such call` })
      }
      const agent = await loadAgentForUser(userId, input.agentId)
      const outcome = await ctx.db.transaction(async (tx) => {
        const txid = await generateTxId(tx)
        const { session, created } = await conversationInTx(tx, agent, userId)
        // `conversationInTx` read an existing row BEFORE this lock: read the
        // root again under it, so "has no key" and the write are one step.
        await lockTreeKeys(tx, session.id)
        const root = await loadRoot(tx, session.id)
        if (root.sealEpoch === null || !root.sealPublicKey) {
          await createTreeKeyInTx(tx, userId, { ...input.seal, rootId: root.id })
          return { rootId: root.id, epoch: input.seal.epoch as number, publicKey: input.seal.publicKey, txid, created }
        }
        return { rootId: root.id, epoch: root.sealEpoch, publicKey: root.sealPublicKey, txid, created }
      })
      const { created, ...answer } = outcome
      if (created) nudge(created.ownerId, created.deviceId)
      return answer
    }),

  // #EXP-1961 (voice-call.json `callSession`): the HOST asks for the call's
  // own session while it rings (a desktop running the call itself too): a
  // child of the member's chat with the agent, started, or resumed within
  // `resumeMs`. Null = no chat yet or no room: the host keeps sending into
  // the chat (the old path).
  callSession: authedProcedure
    .input(z.object({ agentId: z.string().uuid(), callId: z.string().min(1).max(256).optional() }))
    .mutation(async ({ ctx, input }) => {
      const userId = ctx.session.user.id
      if (!(await memberAgent(ctx.db, userId, input.agentId))) {
        throw new TRPCError({ code: `NOT_FOUND`, message: `That agent isn't in your teams` })
      }
      return startCallSession({ userId, agentId: input.agentId, callId: input.callId ?? null })
    }),

  // #EXP-1961: the hang-up (either side, a drop too): the call session is
  // asked for its summary, which reaches the chat as a waking report card.
  callEnded: authedProcedure
    .input(z.object({ sessionId: z.string().uuid() }))
    .mutation(async ({ ctx, input }) => ({
      ended: await endCallSession({ userId: ctx.session.user.id, sessionId: input.sessionId }),
    })),

  // NEW-55: the voice device answers a `call_ring`. Minted for the member
  // alone; the relay admits it only into a ringing call on a device this
  // account owns. EXP-1857: any of the member's desktops may be rung; it
  // names itself (`deviceId`, one of the member's own devices); an old IDE
  // without it = the voice device. #EXP-1941: the relay writes the ring and
  // could invent one, so a ticket is minted ONLY for a call id `startCall`
  // minted for this member, this device and the agent the host names
  // (`agentId`, the ring's; `vouchedCall` has the rule for a host that names
  // none), within the id's life. The answer says `seal`, the mode the caller
  // was told. Stateless: an id is answered again while it lives.
  callHostTicket: authedProcedure
    .input(
      z.object({
        callId: z.string().uuid(),
        deviceId: z.string().min(1).max(128).optional(),
        agentId: z.string().uuid().optional(),
      })
    )
    .mutation(async ({ ctx, input }) => {
      const config = getSteerRelayConfig()
      if (!config) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      const userId = ctx.session.user.id
      let deviceId: string
      if (input.deviceId !== undefined) {
        const [own] = await ctx.db
          .select({ deviceId: devices.deviceId })
          .from(devices)
          .where(and(eq(devices.userId, userId), eq(devices.deviceId, input.deviceId)))
          .limit(1)
        if (!own) throw new TRPCError({ code: `NOT_FOUND`, message: `That device isn't yours` })
        deviceId = own.deviceId
      } else {
        // NEW-55 review: the ticket names the member's voice device; the
        // relay admits it only into that device's call room.
        const settings = await voiceSettingsOf(ctx.db, userId)
        if (!settings.voiceDeviceId) {
          throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `No voice device` })
        }
        deviceId = settings.voiceDeviceId
      }
      const vouched = await vouchedCall(ctx.db, userId, {
        callId: input.callId,
        deviceId,
        namedDevice: input.deviceId !== undefined,
        agentId: input.agentId,
      })
      if (vouched.sealed === null) {
        // A healthy call that fails here must be told from a forged ring:
        // who, where and which candidates, never an id or MAC bytes.
        console.warn(`[voice] callHostTicket refused: member=${userId} device=${deviceId} tried ${vouched.tried}`)
        throw new TRPCError({ code: `NOT_FOUND`, message: `No such call` })
      }
      const minted = mintSteerTicket(config, {
        kind: `call_host`,
        userId,
        callId: input.callId,
        deviceId,
      })
      if (`disabled` in minted) {
        throw new TRPCError({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
      }
      return { callId: input.callId, ticket: minted.ticket, url: minted.url, relayUrl: config.url, seal: vouched.sealed }
    }),
})
