// NEW-57: meeting mode on the member's voice device. Pure and byte-locked by
// `fixtures/meeting.json`, whose `$comment` holds the rules; this is the
// reference. Mirrors: desktop `domain::meeting` (every case), iOS
// `ExpCore/Sources/Domain/Meeting.swift` and Android `domain/Meeting.kt`
// (copy, testIds and the option / who / consent builders).

import fixture from "../fixtures/meeting.json" with { type: "json" }

export const MEETING_COPY = fixture.copy
export const MEETING_TEST_IDS = fixture.testIds
export const MEETING_LIMITS = fixture.limits
export const MEETING_FLUSH = fixture.flush
export const MEETING_PAUSE = fixture.pause
export const DESKTOP_AUDIO_VALUES = fixture.desktopAudioValues

export type DesktopAudio = `off` | `all` | `include` | `exclude`
export type DesktopAudioCapability = `none` | `system` | `per_app`

export interface AudioApp {
  id: string
  name: string
}

/** The Desktop audio row's values for the voice device's capability. */
export function desktopAudioOptions(capability: DesktopAudioCapability): DesktopAudio[] {
  if (capability === `per_app`) return [`off`, `all`, `include`, `exclude`]
  if (capability === `system`) return [`off`, `all`]
  return [`off`]
}

/** The Apps row's options: the live apps by name, then stored ones no longer
 * playing, in their stored order. */
export function appOptions(
  live: readonly AudioApp[],
  stored: readonly AudioApp[]
): { value: string; label: string }[] {
  const byName = [...live].sort((a, b) => (a.name < b.name ? -1 : a.name > b.name ? 1 : 0))
  const out = byName.map((app) => ({ value: app.id, label: app.name }))
  for (const app of stored) {
    if (!out.some((option) => option.value === app.id)) out.push({ value: app.id, label: app.name })
  }
  return out
}

/** The setting as the device can honour it: no capability = off, system
 * only = include/exclude read as all. */
function effective(desktopAudio: DesktopAudio, capability: DesktopAudioCapability): DesktopAudio {
  if (capability === `none`) return `off`
  if (capability === `system` && desktopAudio !== `off`) return `all`
  return desktopAudio
}

function joinNames(names: readonly string[]): string {
  if (names.length <= 1) return names.join(``)
  return `${names.slice(0, -1).join(`, `)} ${MEETING_COPY.and} ${names[names.length - 1]}`
}

/** Who a meeting transcribes, in words. */
export function who(
  desktopAudio: DesktopAudio,
  apps: readonly AudioApp[],
  capability: DesktopAudioCapability
): string {
  const mode = effective(desktopAudio, capability)
  const you = MEETING_COPY.you
  if (mode === `all`) return joinNames([you, MEETING_COPY.whoAll])
  if (mode === `exclude`) {
    const except = MEETING_COPY.whoExcept.replace(`{apps}`, joinNames(apps.map((app) => app.name)))
    return joinNames([you, apps.length ? except : MEETING_COPY.whoAll])
  }
  if (mode === `include`) return joinNames([you, ...apps.map((app) => app.name)])
  return you
}

/** THE consent dialog's body. */
export function consentBody(whoText: string, agent: string, days: number): string {
  return MEETING_COPY.consentBody
    .replace(`{who}`, whoText)
    .replace(`{agent}`, agent)
    .replace(`{days}`, String(days))
}

export interface MeetingChannel {
  channel: string
  label: string
}

/** The channels a meeting listens on: the microphone, then the apps. */
export function channels(
  desktopAudio: DesktopAudio,
  apps: readonly AudioApp[],
  capability: DesktopAudioCapability,
  playing: readonly AudioApp[]
): MeetingChannel[] {
  const out: MeetingChannel[] = [{ channel: `you`, label: MEETING_COPY.you }]
  const mode = effective(desktopAudio, capability)
  if (mode === `off`) return out
  if (capability === `system`) return [...out, { channel: `desktop`, label: MEETING_COPY.desktop }]
  const picked = new Set(apps.map((app) => app.id))
  for (const app of playing) {
    const listened =
      mode === `all` || (mode === `include` ? picked.has(app.id) : !picked.has(app.id))
    if (listened) out.push({ channel: app.id, label: app.name })
  }
  return out
}

export type FlushEvent = `segment` | `tick` | `wake` | `ptt` | `turnDone`

export interface FlushState {
  /** `at` of every queued segment, oldest first. */
  queued: number[]
  inFlightSince: number | null
  /** A wake/ptt arrived while a turn was in flight. */
  urgent: boolean
}

export const FLUSH_START: FlushState = { queued: [], inFlightSince: null, urgent: false }

/** One flush step: the next state and how many queued segments go out now
 * (null = nothing sent). */
export function flushStep(
  state: FlushState,
  event: FlushEvent,
  at: number
): { state: FlushState; send: number | null } {
  let { queued, inFlightSince, urgent } = state
  if (inFlightSince != null && at - inFlightSince >= MEETING_FLUSH.turnTimeoutMs) inFlightSince = null
  if (event === `segment`) queued = [...queued, at]
  if (event === `turnDone`) inFlightSince = null
  const trigger = event === `wake` || event === `ptt`
  if (trigger && queued.length > 0) urgent = true
  let send: number | null = null
  if (inFlightSince == null && queued.length > 0) {
    const due =
      urgent ||
      queued.length >= MEETING_FLUSH.maxSegments ||
      at - queued[0]! >= MEETING_FLUSH.maxAgeMs
    if (due) {
      send = queued.length
      queued = []
      urgent = false
      inFlightSince = at
    }
  }
  return { state: { queued, inFlightSince, urgent }, send }
}

export interface MeetingSegment {
  channel: string
  label: string
  text: string
  /** The first word's start, epoch ms. */
  startMs: number
}

function pad(value: number): string {
  return String(value).padStart(2, `0`)
}

/** The message a flush sends: a header and one fenced line per segment. */
export function meetingMessage(segments: readonly MeetingSegment[]): string {
  const ordered = [...segments].sort((a, b) => {
    if (a.startMs !== b.startMs) return a.startMs - b.startMs
    if (a.channel === `you` || b.channel === `you`) return a.channel === `you` ? -1 : 1
    return a.label < b.label ? -1 : a.label > b.label ? 1 : 0
  })
  const first = ordered[0] ? new Date(ordered[0].startMs) : new Date(0)
  const date = `${first.getUTCFullYear()}-${pad(first.getUTCMonth() + 1)}-${pad(first.getUTCDate())}`
  const lines = ordered.map((segment) => {
    const at = new Date(segment.startMs)
    const time = `${pad(at.getUTCHours())}:${pad(at.getUTCMinutes())}:${pad(at.getUTCSeconds())}`
    return `${time} ${segment.label}: ${segment.text}`
  })
  return [MEETING_COPY.messageHeader.replace(`{date}`, date), "```text", ...lines, "```"].join(`\n`)
}
