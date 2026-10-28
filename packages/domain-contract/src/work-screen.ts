// NEW-59: THE work screen — the display coding agents drive with computer
// use. Pure and byte-locked by `fixtures/work-screen.json`, whose `$comment`
// holds the rules; this is the reference. Mirrors: desktop computer crate
// `work_screen` (gate, current, overlay) + ui device_settings (options), iOS
// `ExpCore/Sources/Domain/WorkScreen.swift` and Android
// `domain/WorkScreen.kt` (copy + options).

import fixture from "../fixtures/work-screen.json" with { type: "json" }

export const WORK_SCREEN_COPY = fixture.copy
export const WORK_SCREEN_TEST_IDS = fixture.testIds
export const WORK_SCREEN_INPUT_TOOLS: readonly string[] = fixture.inputTools

/** One display as the device reports it in `doctor.displays`. */
export interface WorkScreenDisplay {
  number: number
  width: number
  height: number
  primary: boolean
}

export interface WorkScreenOption {
  /** The stored `launch_defaults.computerUseDisplay`; null = Auto. */
  value: number | null
  label: string
}

const SEP = ` · `

function displayLabel(number: number): string {
  return WORK_SCREEN_COPY.display.replace(`{number}`, String(number))
}

/** The Work screen picker's options: Auto, the reported displays, then a
 * stored display the device no longer reports. */
export function workScreenOptions(
  displays: readonly WorkScreenDisplay[] | null | undefined,
  stored: number | null | undefined
): WorkScreenOption[] {
  const out: WorkScreenOption[] = [{ value: null, label: WORK_SCREEN_COPY.auto }]
  for (const display of displays ?? []) {
    let label = `${displayLabel(display.number)}${SEP}${display.width} × ${display.height}`
    if (display.primary) label += `${SEP}${WORK_SCREEN_COPY.primary}`
    out.push({ value: display.number, label })
  }
  if (stored != null && !out.some((option) => option.value === stored)) {
    out.push({
      value: stored,
      label: `${displayLabel(stored)}${SEP}${WORK_SCREEN_COPY.notConnected}`,
    })
  }
  return out
}

export interface WorkScreenState {
  count: number
  /** 1-based; null = Auto. */
  picked: number | null
  /** 0-based displays holding a fullscreen window. */
  fullscreen: readonly number[]
  /** 0-based display of the focused window. */
  active: number | null
  activeFullscreen: boolean
}

/** The 0-based displays agents may drive. */
export function workDisplays(state: WorkScreenState): number[] {
  if (state.picked != null && state.picked >= 1 && state.picked <= state.count) {
    return [state.picked - 1]
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
  /** `screenshot`'s 0-based `display` argument. */
  display?: number
  /** `activate_window`: the 0-based display holding the window's centre. */
  windowDisplay?: number
}

export type WorkScreenVerdict = `allow` | { kind: WorkScreenRefusalKind; text: string }

function refuse(
  kind: WorkScreenRefusalKind,
  number: number | null,
  work: readonly number[]
): WorkScreenVerdict {
  const text = WORK_SCREEN_COPY.refusals[kind]
    .replace(`{number}`, String(number ?? ``))
    .replace(`{work}`, work.map((index) => index + 1).join(`, `))
  return { kind, text }
}

/** Whether the hub runs `call` now, else the `wrong_display` refusal. */
export function workScreenGate(state: WorkScreenState, call: WorkScreenCall): WorkScreenVerdict {
  const work = workDisplays(state)
  if (call.tool === `wait`) return `allow`
  if (work.length === 0) return refuse(`noWorkScreen`, null, work)
  if (call.tool === `screenshot` && call.display != null && !work.includes(call.display)) {
    return refuse(`notWorkScreen`, call.display + 1, work)
  }
  if (
    call.tool === `activate_window` &&
    call.windowDisplay != null &&
    !work.includes(call.windowDisplay)
  ) {
    return refuse(`windowElsewhere`, call.windowDisplay + 1, work)
  }
  if (WORK_SCREEN_INPUT_TOOLS.includes(call.tool) && state.active != null) {
    const gaming = state.activeFullscreen && !work.includes(state.active)
    if (gaming) return refuse(`gaming`, state.active + 1, work)
    if ((call.tool === `type` || call.tool === `key`) && !work.includes(state.active)) {
      return refuse(`keysElsewhere`, state.active + 1, work)
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
