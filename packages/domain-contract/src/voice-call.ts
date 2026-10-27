// NEW-55: caller mode. Pure and byte-locked by `fixtures/voice-call.json`
// (its `$comment` is the spec); the desktop mirror `domain::voice_call`
// replays the same cases, the phones and web replay them with V5 / V6.

import fixture from "../fixtures/voice-call.json" with { type: "json" }

export const VOICE_CALL_COPY = fixture.copy
export const VOICE_CALL_TEST_IDS = fixture.testIds
export const VOICE_CALL_ICONS = fixture.icons
export const VOICE_CALL_CONSTANTS = fixture.constants
export const VOICE_CALL_WIRE = fixture.wire

export type CallPlatform = `desktop` | `phone` | `web`
/** The voice device's doctor row `speech` as a caller reads it. */
export type CallSpeech = `ok` | `preparing` | `noGpu`

export interface CallAvailabilityInput {
  platform: CallPlatform
  /** The voice device's name; null = none picked. */
  device: string | null
  speakReplies: boolean
  online: boolean
  /** THIS client is the voice device. */
  here: boolean
  /** null = no `speech` row yet. */
  speech: CallSpeech | null
  /** A call already runs on the voice device. */
  busy: boolean
}

export interface CallAvailability {
  enabled: boolean
  reason: string | null
}

/** Whether the telephone is enabled, and the reason when it is not. */
export function callAvailability(input: CallAvailabilityInput): CallAvailability {
  const reasons = fixture.copy.reasons
  const off = (reason: string): CallAvailability => ({
    enabled: false,
    reason: reason.replace(`{device}`, input.device ?? ``),
  })
  if (input.device === null) return off(reasons.noVoiceDevice)
  if (!input.speakReplies) return off(reasons.speakOff)
  if (!input.online) return off(reasons.offline)
  if (input.platform === `desktop` && !input.here) return off(reasons.notHere)
  if (input.speech === null || input.speech === `preparing`) return off(reasons.preparing)
  if (input.speech === `noGpu`) return off(reasons.noGpu)
  if (input.busy) return off(reasons.busy)
  return { enabled: true, reason: null }
}

export type CallPhase = `hidden` | `connecting` | `listening` | `thinking` | `speaking`

export interface CallState {
  phase: CallPhase
  muted: boolean
  device: string
  /** The last utterance, quoted (what Thinking shows). */
  said: string
  line: string
  toast: string | null
}

export type CallEvent =
  | { t: `start`; device: string }
  | { t: `connected` }
  | { t: `partial`; text: string }
  | { t: `said`; text: string }
  | { t: `speechStarted` }
  | { t: `queued`; count: number; text: string }
  | { t: `replyStarted`; text: string }
  | { t: `replyFinished` }
  | { t: `mute` }
  | { t: `unmute` }
  | { t: `end` }
  | { t: `failed`; code: string }

export const CALL_IDLE: CallState = { phase: `hidden`, muted: false, device: ``, said: ``, line: ``, toast: null }

function quoted(text: string): string {
  return `“${text}”`
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
  if (event.t === `start`) return { ...CALL_IDLE, phase: `connecting`, device: event.device }
  if (state.phase === `hidden`) return state
  switch (event.t) {
    case `connected`:
      return state.phase === `connecting` ? at(`listening`, ``) : state
    case `partial`: {
      const text = event.text.trim()
      return state.phase === `listening` && text ? at(`listening`, quoted(text)) : state
    }
    case `said`: {
      const text = event.text.trim()
      if (!text) return state
      return at(`thinking`, quoted(text), { said: quoted(text) })
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
      return at(`speaking`, event.text.trim())
    case `replyFinished`:
      return state.phase === `speaking` ? at(`listening`, ``) : state
    case `mute`:
      return { ...state, muted: true }
    case `unmute`:
      return { ...state, muted: false }
    case `end`:
      return CALL_IDLE
    case `failed`: {
      const failed = fixture.copy.failed as Record<string, string>
      const toast = (failed[event.code] ?? failed.dropped).replace(`{device}`, state.device)
      return { ...CALL_IDLE, toast }
    }
  }
  return state
}

export interface CallView {
  activity: `listening` | `thinking` | `speaking` | `muted`
  state: string
  line: string
  /** The mute pill's label (Mute / Unmute); End call is always beside it. */
  mute: string
}

/** What the call surface shows; null = no surface. */
export function callView(state: CallState): CallView | null {
  const copy = fixture.copy
  const mute = state.muted ? copy.unmute : copy.mute
  switch (state.phase) {
    case `hidden`:
      return null
    case `connecting`:
      return { activity: `thinking`, state: copy.connecting, line: state.line, mute }
    case `listening`:
      return state.muted
        ? { activity: `muted`, state: copy.muted, line: copy.mutedLine, mute }
        : { activity: `listening`, state: copy.listening, line: state.line, mute }
    case `thinking`:
      return { activity: `thinking`, state: copy.thinking, line: state.line, mute }
    case `speaking`:
      return { activity: `speaking`, state: copy.speaking, line: state.line, mute }
  }
}
