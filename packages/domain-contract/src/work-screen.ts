// NEW-59: THE work screen — the display coding agents drive with computer
// use. Pure and byte-locked by `fixtures/work-screen.json`, whose `$comment`
// holds the rules; this is the reference. Mirrors: desktop epilot crate
// `work_screen` (gate, current, overlay) + ui device_settings (options), iOS
// `ExpCore/Sources/Domain/WorkScreen.swift` and Android
// `domain/WorkScreen.kt` (copy + options). #EXP-1514: the agent space
// (`space`, the launcher's `place`, the gate's `spaceOnly`).

import fixture from "../fixtures/work-screen.json" with { type: "json" }

export const WORK_SCREEN_COPY = fixture.copy
export const WORK_SCREEN_TEST_IDS = fixture.testIds
export const WORK_SCREEN_INPUT_TOOLS: readonly string[] = fixture.inputTools
/** The tools that press keys into the focused window (`keysElsewhere`). */
export const WORK_SCREEN_KEY_TOOLS: readonly string[] = fixture.keyTools
/** #FEED-164: a string-keyed picker's keys (web, the IDE, Android). */
export const WORK_SCREEN_PICKER_KEYS = fixture.pickerKeys
/** #EXP-1514: the reserved pick `value` (agents work in an agent space only)
 * and the picker's words for it, apart from `copy` (both phones lock that
 * key set). */
export const WORK_SCREEN_SPACE = fixture.space

/** One display as the device reports it in `doctor.displays`. */
export interface WorkScreenDisplay {
  /** #FEED-161: stable across reconnects and reorders (the stored value). */
  id: string
  /** 1-based position, for the label only. */
  number: number
  width: number
  height: number
  primary: boolean
}

export interface WorkScreenOption {
  /** The stored `launch_defaults.computerUseDisplayId`; null = Auto. */
  value: string | null
  label: string
}

const SEP = ` · `

function displayLabel(number: number): string {
  return WORK_SCREEN_COPY.display.replace(`{number}`, String(number))
}

/** The Work screen picker's options: Auto, the agent space, the reported
 * displays, then a stored display the device no longer reports.
 * #EXP-1514: while the device offers a space (`spaceOffered`, its doctor's
 * `agent_space` row is `ok`) Auto says it lands there and Agent space only
 * follows it; a stored `space` stays listed in that place, never Not
 * connected. */
export function workScreenOptions(
  displays: readonly WorkScreenDisplay[] | null | undefined,
  stored: string | null | undefined,
  spaceOffered = false
): WorkScreenOption[] {
  const auto = spaceOffered ? `${WORK_SCREEN_COPY.auto}${SEP}${WORK_SCREEN_SPACE.label}` : WORK_SCREEN_COPY.auto
  const out: WorkScreenOption[] = [{ value: null, label: auto }]
  if (spaceOffered || stored === WORK_SCREEN_SPACE.value) {
    out.push({ value: WORK_SCREEN_SPACE.value, label: WORK_SCREEN_SPACE.only })
  }
  for (const display of displays ?? []) {
    let label = `${displayLabel(display.number)}${SEP}${display.width} × ${display.height}`
    if (display.primary) label += `${SEP}${WORK_SCREEN_COPY.primary}`
    out.push({ value: display.id, label })
  }
  if (stored != null && !out.some((option) => option.value === stored)) {
    out.push({ value: stored, label: WORK_SCREEN_COPY.notConnected })
  }
  return out
}

/** #FEED-164: the picker key of an option value (null = Auto): Auto's key,
 * #EXP-1514 the agent space's key, else the display prefix + the id, so no
 * id can read as Auto or as the space. */
export function workScreenPickerKey(value: string | null): string {
  if (value == null) return WORK_SCREEN_PICKER_KEYS.auto
  if (value === WORK_SCREEN_SPACE.value) return WORK_SCREEN_PICKER_KEYS.space
  return `${WORK_SCREEN_PICKER_KEYS.display}${value}`
}

/** #FEED-164: the stored value (the bare display id, #EXP-1514 the space's
 * value, null = Auto) of a picker key. */
export function workScreenPickedId(key: string): string | null {
  if (key === WORK_SCREEN_PICKER_KEYS.space) return WORK_SCREEN_SPACE.value
  return key.startsWith(WORK_SCREEN_PICKER_KEYS.display)
    ? key.slice(WORK_SCREEN_PICKER_KEYS.display.length)
    : null
}

/** A device doctor as far as the space offer reads it (device-doctor.json). */
export interface WorkScreenDoctor {
  items?: readonly { key: string; state: string }[] | null
}

/** device-doctor.json's agent space row (#EXP-1514). */
const AGENT_SPACE_ITEM = `agent_space`

/** #EXP-1514: whether the device offers an agent space, read by every
 * client's picker: its doctor carries the `agent_space` item in state `ok`. */
export function spaceOffered(doctor: WorkScreenDoctor | null | undefined): boolean {
  return doctor?.items?.some((item) => item.key === AGENT_SPACE_ITEM && item.state === `ok`) ?? false
}

/** #EXP-1514: the Work screen pick as the launcher reads it: Auto, the agent
 * space only, or a display. */
export type WorkScreenPick = `auto` | `space` | `display`

/** #EXP-1514: the device's answer to "is there an agent space": `no` = a
 * stable reason; `unknown` = an Exponential Desktop that did not answer. */
export type WorkScreenSpaceOffer = `offered` | `no` | `unknown`

/** #EXP-1514: where a run's computer use happens. */
export type WorkScreenPlace = `none` | `person` | `space`

export interface WorkScreenPlaceInput {
  /** The device's Computer use switch, the gate. */
  switch: boolean
  /** `computerUse` false = the session opted out; `place` = the session's
   * own (option B), `auto` unless sessions carry one; an unknown value reads
   * as `auto`. */
  session: { computerUse: boolean; place?: string | null }
  pick: WorkScreenPick
  offer: WorkScreenSpaceOffer
  /** Whether the person's screen can be driven. */
  person: boolean
}

/** Where a side lets a run work. */
interface Allowed {
  space: boolean
  person: boolean
}

const ANYWHERE: Allowed = { space: true, person: true }
const SPACE_ONLY: Allowed = { space: true, person: false }
const PERSON_ONLY: Allowed = { space: false, person: true }

/** The device's side, off its Work screen pick. */
function pickAllows(pick: WorkScreenPick): Allowed {
  if (pick === `space`) return SPACE_ONLY
  if (pick === `display`) return PERSON_ONLY
  return ANYWHERE
}

/** The session's side, off its place (anything else reads as `auto`). */
function sessionAllows(place: string | null | undefined): Allowed {
  if (place === `space`) return SPACE_ONLY
  if (place === `person`) return PERSON_ONLY
  return ANYWHERE
}

/** #EXP-1514 (`placeCases`, desktop only): the launcher's answer at every
 * launch. The device allows {space, person} for Auto, {space} for the space
 * pick, {person} for a display; the session likewise. `space` when both allow
 * a space and the offer is not `no` (the offer is asked only then; `unknown`
 * is never the person's screen); else `person` when both allow the screen and
 * it can be driven; else `none`. */
export function place(input: WorkScreenPlaceInput): WorkScreenPlace {
  if (!input.switch || !input.session.computerUse) return `none`
  const device = pickAllows(input.pick)
  const session = sessionAllows(input.session.place)
  if (device.space && session.space && input.offer !== `no`) return `space`
  if (device.person && session.person && input.person) return `person`
  return `none`
}

export interface WorkScreenState {
  count: number
  /** 1-based number of the display whose id is stored; 0 = that id is not
   * connected; null = Auto. */
  picked: number | null
  /** 0-based displays holding a fullscreen window. */
  fullscreen: readonly number[]
  /** 0-based display of the focused window. */
  active: number | null
  activeFullscreen: boolean
  /** false = the OS window list could not be read; absent = true. */
  windowsRead?: boolean
  /** #EXP-1514: the stored pick is the agent space only; absent = false. */
  spaceOnly?: boolean
}

/** The 0-based displays agents may drive: none with the pick Agent space
 * only, the picked one while connected, none while the picked one is gone
 * (never a wider set), else (Auto) every display without a fullscreen app. */
export function workDisplays(state: WorkScreenState): number[] {
  if (state.spaceOnly) return []
  if (state.picked != null) {
    return state.picked >= 1 && state.picked <= state.count ? [state.picked - 1] : []
  }
  const out: number[] = []
  for (let index = 0; index < state.count; index++) {
    if (!state.fullscreen.includes(index)) out.push(index)
  }
  return out
}

export type WorkScreenRefusalKind = keyof typeof fixture.copy.refusals

export interface WorkScreenCall {
  tool: string
  /** `activate_window`'s window id. */
  id?: number
  /** `screenshot`'s 0-based `display` argument. */
  display?: number
  /** `activate_window`: the 0-based display holding the listed window's
   * centre; absent/null = list_windows does not name the id. */
  windowDisplay?: number | null
}

export type WorkScreenVerdict = `allow` | { kind: WorkScreenRefusalKind; text: string }

interface RefusalFields {
  display?: number
  picked?: number | null
  id?: number
}

function refuse(
  kind: WorkScreenRefusalKind,
  fields: RefusalFields,
  work: readonly number[]
): WorkScreenVerdict {
  const text = WORK_SCREEN_COPY.refusals[kind]
    .replace(`{display}`, String(fields.display ?? ``))
    .replace(`{picked}`, String(fields.picked ?? ``))
    .replace(`{id}`, String(fields.id ?? ``))
    .replace(`{work}`, work.join(`, `))
  return { kind, text }
}

/** Whether the hub runs `call` now, else the `wrong_display` refusal. */
export function workScreenGate(state: WorkScreenState, call: WorkScreenCall): WorkScreenVerdict {
  const work = workDisplays(state)
  if (call.tool === `wait`) return `allow`
  // #EXP-1514: the pick Agent space only, checked first.
  if (state.spaceOnly) return refuse(`spaceOnly`, {}, work)
  if (state.picked != null && work.length === 0) {
    return refuse(`pickedGone`, {}, work)
  }
  if (work.length === 0) return refuse(`noWorkScreen`, {}, work)
  const input = WORK_SCREEN_INPUT_TOOLS.includes(call.tool)
  // #FEED-161: Auto without a window list knows no display to be free.
  if (state.windowsRead === false && state.picked == null) return refuse(`noWindowList`, {}, work)
  if (state.windowsRead === false && (input || call.tool === `list_windows`)) {
    return refuse(`noWindowList`, {}, work)
  }
  if (call.tool === `screenshot` && call.display != null && !work.includes(call.display)) {
    return refuse(`notWorkScreen`, { display: call.display }, work)
  }
  if (call.tool === `activate_window`) {
    if (call.windowDisplay == null) return refuse(`unknownWindow`, { id: call.id }, work)
    if (!work.includes(call.windowDisplay)) {
      return refuse(`windowElsewhere`, { display: call.windowDisplay }, work)
    }
  }
  if (input && state.active != null) {
    if (state.activeFullscreen && !work.includes(state.active)) {
      return refuse(`gaming`, { display: state.active }, work)
    }
    if (WORK_SCREEN_KEY_TOOLS.includes(call.tool) && !work.includes(state.active)) {
      return refuse(`keysElsewhere`, { display: state.active }, work)
    }
  }
  return `allow`
}

/** The run's current display: its previous pick while it stays a work
 * display, else the primary when it is one, else the first work display. */
export function workScreenCurrent(
  work: readonly number[],
  primary: number,
  previous: number | null
): number | null {
  if (previous != null && work.includes(previous)) return previous
  if (work.includes(primary)) return primary
  return work[0] ?? null
}

export interface WorkScreenOverlayState {
  count: number
  pointer: number
  fullscreen: readonly number[]
  work: readonly number[]
}

/** The display the voice overlay opens on; null = stays hidden. */
export function overlayDisplay(state: WorkScreenOverlayState): number | null {
  const free = (index: number) => !state.fullscreen.includes(index)
  if (free(state.pointer)) return state.pointer
  const work = state.work.find(free)
  if (work != null) return work
  for (let index = 0; index < state.count; index++) {
    if (free(index)) return index
  }
  return null
}
