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
export type CallSpeech = `ok` | `preparing` | `noGpu` | `failed`

/** NEW-99 (`speechCases`): the voice device's doctor row `speech` (group
 *  voice) state as a caller reads it; null = no row yet. */
export function callSpeechOf(state: string | null | undefined): CallSpeech | null {
  if (state === null || state === undefined) return null
  if (state === `ok`) return `ok`
  if (state === `missing`) return `noGpu`
  if (state === `error`) return `failed`
  return `preparing`
}

export interface CallAvailabilityInput {
  /** NEW-99: the member has a voice Assistant (`voice.get` agentId). */
  agent: boolean
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
  if (!input.agent) return off(reasons.noAgent)
  if (input.device === null) return off(reasons.noVoiceDevice)
  if (!input.online) return off(reasons.offline)
  if (input.meeting) return off(reasons.meeting)
  if (input.platform === `desktop` && !input.here) return off(reasons.notHere)
  if (input.speech === null) return off(reasons.preparing)
  if (input.speech === `failed`) return off(reasons.voiceFailed)
  if (input.speech === `noGpu`) return off(reasons.noGpu)
  if (input.busy) return off(reasons.busy)
  return { enabled: true, reason: null }
}

/** #EXP-1878 (`tapCases`): what a press on the telephone does: call, or
 *  (a phone's tap on a disabled one) toast the reason; web and desktop show
 *  it as the disabled control's tooltip, their click does nothing. */
export interface CallTap {
  call: boolean
  toast: string | null
}

export function callTap(platform: CallPlatform, availability: CallAvailability): CallTap {
  if (availability.enabled) return { call: true, toast: null }
  return { call: false, toast: platform === `phone` ? availability.reason : null }
}

/** EXP-1857: one of the member's own desktops as `callTarget` reads it. */
export interface CallTargetDevice {
  id: string
  name: string
  online: boolean
  /** Its doctor row `speech` through `callSpeechOf` (null = none yet). */
  speech: CallSpeech | null
  /** A meeting runs on it. */
  meeting?: boolean
}

export interface CallTargetInput {
  platform: CallPlatform
  /** THIS client's own desktop (null on phones and web). */
  here: string | null
  /** The called agent's Runs on device. */
  agentDevice: string | null
  /** The member's own desktops, newest `lastSeenAt` first. */
  desktops: CallTargetDevice[]
  /** EXP-1858: the member's own servers (CLI daemons): a candidate only as
   *  the agent's device, never here nor a fallback. Absent = none. */
  servers?: CallTargetDevice[]
  /** A call this client does not run already runs on the target. */
  busy: boolean
}

export interface CallTarget extends CallAvailability {
  /** The desktop whose speech runs the call (null = disabled). */
  device: string | null
  name: string | null
  /** The target is not the agent's own device. */
  fallback: boolean
  /** The target is this client's own desktop: the IDE runs the call. */
  local: boolean
}

/** A desktop that can run a call now (a downloading voice rings until ready). */
function speaks(device: CallTargetDevice): boolean {
  return device.online && !device.meeting && (device.speech === `ok` || device.speech === `preparing`)
}

/** EXP-1857 (`targetCases`): where a call's speech runs. The first desktop
 *  that speaks of: the agent's device (one of the member's own desktops,
 *  EXP-1858: or servers), the desktop you're at, the other desktops newest
 *  first. None speaks = disabled with the first candidate's reason. */
export function callTarget(input: CallTargetInput): CallTarget {
  const byId = (id: string | null) => (id === null ? undefined : input.desktops.find((device) => device.id === id))
  const agentDevice =
    input.agentDevice === null
      ? undefined
      : (byId(input.agentDevice) ?? input.servers?.find((device) => device.id === input.agentDevice))
  const candidates = [agentDevice, byId(input.here), ...input.desktops].filter(
    (device): device is CallTargetDevice => device !== undefined
  )
  const target = candidates.find(speaks)
  const none = { device: null, name: null, fallback: false, local: false }
  if (!target) {
    const first = candidates[0]
    if (!first) return { ...none, enabled: false, reason: fixture.copy.reasons.noDesktop }
    const { reason } = callAvailability({
      agent: true,
      platform: input.platform,
      device: first.name,
      online: first.online,
      here: true,
      speech: first.speech,
      busy: false,
      meeting: first.meeting,
    })
    return { ...none, enabled: false, reason }
  }
  if (input.busy) {
    return { ...none, enabled: false, reason: fixture.copy.reasons.busy.replace(`{device}`, target.name) }
  }
  return {
    enabled: true,
    reason: null,
    device: target.id,
    name: target.name,
    fallback: target.id !== input.agentDevice,
    local: target.id === input.here,
  }
}

export type CallPhase = `hidden` | `ringing` | `listening` | `thinking` | `speaking`

export interface CallState {
  phase: CallPhase
  muted: boolean
  /** Phones: the loudspeaker (false = the earpiece). */
  speaker: boolean
  device: string
  /** EXP-1857: the device is not the agent's own (`copy.via` shows it). */
  fallback: boolean
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
  | { t: `start`; device: string; language?: string; fallback?: boolean }
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
  fallback: false,
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
    return {
      ...CALL_IDLE,
      phase: `ringing`,
      device: event.device,
      fallback: event.fallback ?? false,
      greeting: callGreeting(event.language),
    }
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
  /** EXP-1857: the device that speaks while the call falls back (a second
   *  status line); null otherwise. */
  via: string | null
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
  const via = state.fallback ? copy.via.replace(`{device}`, state.device) : null
  const base = { via, muted: state.muted, speaker: state.speaker }
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

/** EXP-1929 (`livenessCases`): an end of a call that got a `call_alive`
 *  (`alive`) and has heard nothing from the relay for `silentMs` treats the
 *  path as dead and ends the call as dropped; before the first beat (an
 *  older relay) there is no deadline. */
export function callPathDead(alive: boolean, silentMs: number): boolean {
  return alive && silentMs >= fixture.constants.idleTimeoutMs
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

// ── #EXP-1941: the call seal's handshake steps (`seal`, `seal.modeCases`) ────
//
// One pure step for both ends of a call the server marked sealed. It decides
// WHAT happens; the end does it: the key source, `finish` and the frames live
// in the caller and the host (the sealer is @exp/domain-contract/call-seal).
// The message order is Noise NNpsk2's: the caller's hello, the host's hello
// with its tag (the host's proof, opened inside the caller's `finish`), then
// the caller's first frame that opens (the caller's proof).

export const VOICE_CALL_SEAL = fixture.seal

export type CallSealRole = `caller` | `host`
/** `clear` = a call the server did not mark sealed: today's wire, not stepped here. */
export type CallSealPhase = `idle` | `clear` | `told` | `offered` | `keying` | `sealed` | `over`

export interface CallSealState {
  phase: CallSealPhase
  /** Sealed audio frames in a row that did not open. */
  failures: number
}

export const CALL_SEAL_IDLE: CallSealState = { phase: `idle`, failures: 0 }

export type CallSealEvent =
  /** What the server told this end (`startCall` / `callHostTicket`): the host's socket opens here. */
  | { t: `start`; seal: boolean; required?: boolean }
  | { t: `ready` }
  /** A hello of the other end; `ok` = well-formed (its length, version and role), checked before any `finish`. */
  | { t: `hello`; ok: boolean }
  /** The key source gave up: root or epoch still differ or the rotation is still due after asking again, an epoch below the highest held, no key holder. */
  | { t: `noKey` }
  | { t: `key` }
  | { t: `finished`; ok: boolean }
  /** A binary frame; `opens` = the sealer opened it. */
  | { t: `frame`; opens: boolean }
  /** A `sealed` event; `opens` = the sealer opened it (its ctr was the next). */
  | { t: `sealed`; opens: boolean }
  /** Clear content: a `call_event` that is neither `hello`, `sealed` nor `failed`, or a `call_mute`. */
  | { t: `clear` }
  /** A clear `failed {code}`, or the code of the relay's `error` / `bye`. */
  | { t: `failed`; code: string }
  /** The socket closed, or `call_end`, `bye`, `error` without a code that has copy. */
  | { t: `closed` }
  | { t: `timeout`; timer: `handshake` | `maxCall` }

/**
 * What the end does now, in order:
 * `timer` start the `handshakeMs` timer (it is never restarted) ·
 * `hello` the caller sends its hello frame, again every `helloEveryMs`; the host sends its hello ONCE ·
 * `helloStop` · `key` resolve root and epoch and get the secret (the key source) ·
 * `finish` call the handshake's `finish`, once · `go` the caller starts its microphone, the host steps `Ready` ·
 * `count` drop the frame and count it · `end` / `end:<reason>` send `call_end` and close ·
 * `fail:<code>` the caller's toast · `abandon` stop the key source and drop what it still answers.
 */
export type CallSealAction = string

export interface CallSealStepped {
  state: CallSealState
  actions: CallSealAction[]
}

/** A wire code about the caller's OWN device is never taken from the wire: it reads `dropped`. */
export function callSealWireCode(code: string): string {
  return fixture.seal.ownDeviceCodes.includes(code) ? fixture.hangUpRules.failedCode : code
}

export function callSealStep(role: CallSealRole, state: CallSealState, event: CallSealEvent): CallSealStepped {
  const { noKey, seal } = fixture.seal.endReasons
  const caller = role === `caller`
  const stay: CallSealStepped = { state, actions: [] }
  const to = (phase: CallSealPhase, ...actions: CallSealAction[]): CallSealStepped => ({ state: { phase, failures: 0 }, actions })
  /** Every seal failure reads `dropped` on the caller; the host just ends. */
  const over = (reason: string): CallSealStepped =>
    to(`over`, `end:${reason}`, ...(caller ? [`fail:${fixture.hangUpRules.failedCode}`] : []), `abandon`)
  const count: CallSealStepped = { state, actions: [`count`] }

  if (event.t === `start`) {
    if (state.phase !== `idle`) return stay
    if (event.seal) return to(`told`, ...(caller ? [] : [`timer`]))
    if (!(event.required ?? fixture.seal.required)) return to(`clear`)
    return caller ? to(`over`, `fail:${fixture.hangUpRules.failedCode}`) : to(`over`, `end`)
  }
  // The host ends a call it answered at `maxCallMs` itself, sealed or clear.
  if (event.t === `timeout` && event.timer === `maxCall`) {
    return !caller && state.phase !== `idle` && state.phase !== `over` ? to(`over`, `end`, `abandon`) : stay
  }
  if (state.phase === `idle` || state.phase === `clear` || state.phase === `over`) return stay
  const sealed = state.phase === `sealed`

  switch (event.t) {
    case `failed`:
      return to(`over`, `fail:${caller ? callSealWireCode(event.code) : event.code}`, `abandon`)
    case `closed`:
      return to(`over`, `abandon`)
    case `timeout`:
      return sealed ? stay : over(noKey)
    case `ready`:
      return caller && state.phase === `told` ? to(`offered`, `timer`, `hello`) : stay
    case `hello`: {
      // The first well-formed hello is taken on arrival, once; another changes nothing.
      const waiting = caller ? state.phase === `offered` : state.phase === `told`
      if (waiting && event.ok) return caller ? to(`keying`, `helloStop`, `key`) : to(`keying`, `key`)
      return !event.ok && !sealed && (waiting || !caller) ? count : stay
    }
    case `noKey`:
      return state.phase === `keying` ? over(noKey) : stay
    case `key`:
      return state.phase === `keying` ? { state, actions: [`finish`] } : stay
    case `finished`:
      if (state.phase !== `keying`) return stay
      if (!event.ok) return over(caller ? seal : noKey)
      return caller ? to(`sealed`, `go`) : to(`offered`, `hello`)
    case `frame`:
      if (sealed) {
        if (event.opens) return state.failures === 0 ? stay : to(`sealed`)
        const failures = state.failures + 1
        return failures >= fixture.seal.maxOpenFailures ? over(seal) : { state: { phase: `sealed`, failures }, actions: [`count`] }
      }
      // A caller gets no binary frame before it is sealed; a host's first frame that opens is the caller's proof.
      if (caller) return state.phase === `told` ? stay : over(seal)
      return state.phase === `offered` && event.opens ? to(`sealed`, `go`) : count
    case `sealed`:
      if (!caller) return stay
      if (sealed) return event.opens ? stay : over(seal)
      return state.phase === `told` ? stay : over(seal)
    case `clear`:
      if (sealed) return over(seal)
      if (caller) return state.phase === `told` ? stay : over(seal)
      return count
  }
}
