// NEW-55: caller mode. Pure and byte-locked by `fixtures/voice-call.json`
// (its `$comment` is the spec); the desktop mirror `etalk::call`
// replays the same cases, the phones and web replay them with V5 / V6.

import fixture from "../fixtures/voice-call.json" with { type: "json" }

export const VOICE_CALL_COPY = fixture.copy
export const VOICE_CALL_TEST_IDS = fixture.testIds
export const VOICE_CALL_ICONS = fixture.icons
export const VOICE_CALL_CONSTANTS = fixture.constants
export const VOICE_CALL_WIRE = fixture.wire
export const VOICE_CALL_HANG_UP = fixture.hangUpRules
export const VOICE_CALL_PILL = fixture.pill

export type CallPlatform = `desktop` | `phone` | `web`
/** The voice device's doctor row `speech` as a caller reads it. */
export type CallSpeech = `ok` | `preparing` | `noGpu`

export interface CallAvailabilityInput {
  platform: CallPlatform
  /** The voice device's name; null = none picked. */
  device: string | null
  online: boolean
  /** THIS client is the voice device. */
  here: boolean
  /** null = no `speech` row yet. */
  speech: CallSpeech | null
  /** A call this client does not run already runs on the voice device. */
  busy: boolean
  /** #NEW-57: a meeting runs on the voice device. */
  meeting?: boolean
}

export interface CallAvailability {
  enabled: boolean
  reason: string | null
}

/** Whether the telephone is enabled, and the reason when it is not. S-3:
 *  a voice that is still downloading rings until it is ready. */
export function callAvailability(input: CallAvailabilityInput): CallAvailability {
  const reasons = fixture.copy.reasons
  const off = (reason: string): CallAvailability => ({
    enabled: false,
    reason: reason.replace(`{device}`, input.device ?? ``),
  })
  if (input.device === null) return off(reasons.noVoiceDevice)
  if (!input.online) return off(reasons.offline)
  if (input.meeting) return off(reasons.meeting)
  if (input.platform === `desktop` && !input.here) return off(reasons.notHere)
  if (input.speech === null) return off(reasons.preparing)
  if (input.speech === `noGpu`) return off(reasons.noGpu)
  if (input.busy) return off(reasons.busy)
  return { enabled: true, reason: null }
}

export type CallPhase = `hidden` | `ringing` | `listening` | `thinking` | `speaking`

export interface CallState {
  phase: CallPhase
  muted: boolean
  /** Phones: the loudspeaker (false = the earpiece). */
  speaker: boolean
  device: string
  /** The greeting the device says once answered. */
  greeting: string
  /** The ring's earliest answer (`callRingMs`) passed. */
  rang: boolean
  /** The worker confirmed it listens (`CallListening`). */
  ready: boolean
  /** The microphone stayed silent during the ring. */
  noSound: boolean
  /** The last utterance, quoted (what Thinking shows). */
  said: string
  line: string
  toast: string | null
}

export type CallEvent =
  | { t: `start`; device: string; language?: string }
  | { t: `rang` }
  | { t: `ready` }
  | { t: `noSound` }
  | { t: `partial`; text: string }
  | { t: `said`; text: string }
  | { t: `speechStarted` }
  | { t: `queued`; count: number; text: string }
  | { t: `replyStarted`; text: string }
  | { t: `replyFinished` }
  | { t: `mute` }
  | { t: `unmute` }
  | { t: `speaker`; on: boolean }
  | { t: `end` }
  /** The other side's `call_end` (`hangUpRules`): a reason fails it. */
  | { t: `hungUp`; reason?: string | null }
  | { t: `failed`; code: string }

export const CALL_IDLE: CallState = {
  phase: `hidden`,
  muted: false,
  speaker: true,
  device: ``,
  greeting: ``,
  rang: false,
  ready: false,
  noSound: false,
  said: ``,
  line: ``,
  toast: null,
}

/** The greeting in `language` (English for an unknown one). */
export function callGreeting(language: string | undefined): string {
  const greetings = fixture.copy.greeting as Record<string, string>
  return greetings[language ?? `en`] ?? greetings.en
}

function quoted(text: string): string {
  return `“${text}”`
}

function failedState(state: CallState, code: string): CallState {
  const failed = fixture.copy.failed as Record<string, string>
  const toast = (failed[code] ?? failed.dropped).replace(`{device}`, state.device)
  return { ...CALL_IDLE, toast }
}

/** Rang and ready: the device speaks its greeting (not an agent turn). */
function answered(state: CallState): CallState {
  return { ...state, phase: `speaking`, line: state.greeting, toast: null }
}

/** One step of the call (the same machine on the voice device and a caller). */
export function callStep(state: CallState, event: CallEvent): CallState {
  const at = (phase: CallPhase, line: string, patch: Partial<CallState> = {}): CallState => ({
    ...state,
    phase,
    line,
    toast: null,
    ...patch,
  })
  if (event.t === `start`) {
    return { ...CALL_IDLE, phase: `ringing`, device: event.device, greeting: callGreeting(event.language) }
  }
  if (state.phase === `hidden`) return state
  const ringing = state.phase === `ringing`
  switch (event.t) {
    case `rang`:
      if (!ringing) return state
      return state.ready ? answered({ ...state, rang: true }) : { ...state, rang: true }
    case `ready`:
      if (!ringing) return state
      return state.rang ? answered({ ...state, ready: true }) : { ...state, ready: true }
    case `noSound`:
      return { ...state, noSound: true }
    case `partial`: {
      const text = event.text.trim()
      return state.phase === `listening` && text ? at(`listening`, quoted(text), { noSound: false }) : state
    }
    case `said`: {
      const text = event.text.trim()
      if (ringing || !text) return state
      return at(`thinking`, quoted(text), { said: quoted(text), noSound: false })
    }
    case `speechStarted`:
      return state.phase === `speaking` ? at(`listening`, ``) : state
    case `queued`:
      if (state.phase !== `thinking` && state.phase !== `speaking`) return state
      if (event.count <= 0) return at(state.phase, state.phase === `thinking` ? state.said : state.line)
      return at(
        state.phase,
        fixture.copy.queued.replace(`{count}`, String(event.count)).replace(`{text}`, quoted(event.text.trim()))
      )
    case `replyStarted`:
      return ringing ? state : at(`speaking`, event.text.trim())
    case `replyFinished`:
      return state.phase === `speaking` ? at(`listening`, ``) : state
    case `mute`:
      return { ...state, muted: true }
    case `unmute`:
      return { ...state, muted: false }
    case `speaker`:
      return { ...state, speaker: event.on }
    case `end`:
      return CALL_IDLE
    case `hungUp`:
      return event.reason ? failedState(state, fixture.hangUpRules.failedCode) : CALL_IDLE
    case `failed`:
      return failedState(state, event.code)
  }
  return state
}

export type CallActivity = `ringing` | `listening` | `thinking` | `speaking` | `muted`
export type CallButton = `mute` | `speaker` | `end`

export interface CallView {
  activity: CallActivity
  /** The line under the name; null = the call's timer (`callTimer`). */
  status: string | null
  caption: string
  muted: boolean
  speaker: boolean
  /** The round buttons, in order. */
  buttons: CallButton[]
  /** The end button's label (shown under it while ringing). */
  end: string
}

/** What the call surface shows; null = no surface. */
export function callView(state: CallState, platform: CallPlatform = `phone`): CallView | null {
  const copy = fixture.copy
  if (state.phase === `hidden`) return null
  const base = { muted: state.muted, speaker: state.speaker }
  if (state.phase === `ringing`) {
    return {
      ...base,
      activity: `ringing`,
      status: state.rang ? copy.gettingReady : copy.calling,
      caption: ``,
      buttons: [`end`],
      end: copy.cancel,
    }
  }
  const buttons: CallButton[] = platform === `phone` ? [`mute`, `speaker`, `end`] : [`mute`, `end`]
  const inCall = { ...base, status: null, buttons, end: copy.endCall }
  switch (state.phase) {
    case `listening`:
      if (state.muted) return { ...inCall, activity: `muted`, caption: copy.muted }
      return { ...inCall, activity: `listening`, caption: state.line || (state.noSound ? copy.noSound : ``) }
    case `thinking`:
      return { ...inCall, activity: `thinking`, caption: state.line }
    case `speaking`:
      return { ...inCall, activity: `speaking`, caption: state.line }
  }
}

/** EXP-1423: the earliest answer of a ring, ms after it started:
 *  `minRingMs` + floor(clamp(random, 0, 1) × `ringJitterMs`). The HOST draws
 *  `random` once per call (`Math.random()`); the caller never draws. */
export function callRingMs(random: number): number {
  const { minRingMs, ringJitterMs } = fixture.constants
  const unit = Math.min(1, Math.max(0, random))
  return minRingMs + Math.floor(unit * ringJitterMs)
}

/** EXP-1423: when the ring ends (answered), ms after it started: the later
 *  of the earliest answer and the warm-up's `ready`; null = never answered
 *  (no ready, or one at or past `maxRingMs`: failed `unavailable`). */
export function callAnswerMs(random: number, readyMs: number | null): number | null {
  if (readyMs === null || readyMs >= fixture.constants.maxRingMs) return null
  return Math.max(callRingMs(random), readyMs)
}

/** EXP-1423: whether the ring tone sounds `ms` after the ring started (the
 *  shared cadence: `ringToneMs` on, `ringGapMs` off, looping). */
export function callRingToneOn(ms: number): boolean {
  const { ringToneMs, ringGapMs } = fixture.constants
  return ms >= 0 && ms % (ringToneMs + ringGapMs) < ringToneMs
}

/** The call's timer since it was answered: mm:ss, h:mm:ss from an hour. */
export function callTimer(ms: number): string {
  const total = Math.max(0, Math.floor(ms / 1000))
  const hours = Math.floor(total / 3600)
  const minutes = Math.floor((total % 3600) / 60)
  const seconds = total % 60
  const pad = (n: number) => String(n).padStart(2, `0`)
  return hours > 0 ? `${hours}:${pad(minutes)}:${pad(seconds)}` : `${pad(minutes)}:${pad(seconds)}`
}
