// VAPP-260: an app instance's lifecycle as one pure reducer. Locked by
// `fixtures/lifecycle.json`; screens resolve through genUI's host
// (`resolveScreen` for the app's own taps, `crossAppTarget` for links from outside).

import { crossAppTarget, resolveScreen } from "@exponential_app/eui/host"

export type LifecyclePhase = `stopped` | `foreground` | `background`

export interface LifecycleState {
  phase: LifecyclePhase
  screen: string | null
  params: Record<string, string> | null
}

export type LifecycleEvent =
  | { type: `launch` }
  | { type: `foreground` }
  | { type: `background` }
  | { type: `terminate` }
  | { type: `online` }
  | { type: `deepLink`; screen?: string | null; params?: Record<string, unknown> | null }
  | { type: `notificationTap`; app: string; screen?: string | null; params?: Record<string, unknown> | null }

export type LifecycleEffect =
  | { effect: `open`; screen: string; params: Record<string, string> }
  | { effect: `flush` }
  | { effect: `sources`; live: boolean }
  | { effect: `automations`; on: boolean }
  | { effect: `persist`; screen: string; params: Record<string, string> }
  | { effect: `refused`; reason: string }

export interface LifecyclePackage {
  id: string
  screens?: unknown
}

export interface LifecycleContext {
  pkg: LifecyclePackage
  /** The screen the instance persisted last (null = none). */
  lastScreen: { screen: string; params: Record<string, unknown> } | null
}

export const STOPPED: LifecycleState = { phase: `stopped`, screen: null, params: null }

type Target = { screen: string; params: Record<string, string> }

/** The screen a plain launch opens: the persisted one if the package still has it, else the first navigation screen. */
function plainTarget(cx: LifecycleContext): Target | null {
  const last = cx.lastScreen
  const resolved = resolveScreen(cx.pkg, last?.screen ?? null, last?.params ?? null)
  return resolved ? { screen: resolved.screen, params: resolved.params } : null
}

/** Where a link or tap goes, or why it is refused. */
function linkTarget(event: LifecycleEvent, cx: LifecycleContext): Target | { error: string } | null {
  if (event.type === `deepLink`) return crossAppTarget(cx.pkg, event.screen ?? null, event.params ?? null)
  if (event.type === `notificationTap`) {
    if (event.app !== cx.pkg.id) return { error: `otherApp` }
    const resolved = resolveScreen(cx.pkg, event.screen ?? null, event.params ?? null)
    return resolved ? { screen: resolved.screen, params: resolved.params } : { error: `no such screen` }
  }
  return null
}

const ignored = (state: LifecycleState) => ({ state, effects: [] as LifecycleEffect[] })

function coldLaunch(target: Target | null, effects: LifecycleEffect[]): { state: LifecycleState; effects: LifecycleEffect[] } {
  effects.push({ effect: `automations`, on: true })
  if (target) effects.push({ effect: `open`, screen: target.screen, params: target.params })
  effects.push({ effect: `sources`, live: true }, { effect: `flush` })
  return { state: { phase: `foreground`, screen: target?.screen ?? null, params: target?.params ?? null }, effects }
}

/** One lifecycle event → the next state and its effects, in order. */
export function lifecycleStep(state: LifecycleState, event: LifecycleEvent, cx: LifecycleContext): { state: LifecycleState; effects: LifecycleEffect[] } {
  const running = state.phase !== `stopped`
  const persist = (): LifecycleEffect[] => (state.screen !== null ? [{ effect: `persist`, screen: state.screen, params: { ...(state.params ?? {}) } }] : [])
  switch (event.type) {
    case `launch`:
      return running ? ignored(state) : coldLaunch(plainTarget(cx), [])
    case `foreground`:
      if (state.phase !== `background`) return ignored(state)
      return { state: { ...state, phase: `foreground` }, effects: [{ effect: `sources`, live: true }, { effect: `flush` }] }
    case `background`:
      if (state.phase !== `foreground`) return ignored(state)
      return { state: { ...state, phase: `background` }, effects: [...persist(), { effect: `sources`, live: false }] }
    case `terminate`:
      if (!running) return ignored(state)
      return {
        state: STOPPED,
        effects: [...persist(), ...(state.phase === `foreground` ? [{ effect: `sources`, live: false } as LifecycleEffect] : []), { effect: `automations`, on: false }],
      }
    case `online`:
      return state.phase === `foreground` ? { state, effects: [{ effect: `flush` }] } : ignored(state)
    case `deepLink`:
    case `notificationTap`: {
      const target = linkTarget(event, cx)
      if (!target) return ignored(state)
      if (`error` in target) {
        const refused: LifecycleEffect = { effect: `refused`, reason: target.error }
        return running ? { state, effects: [refused] } : coldLaunch(plainTarget(cx), [refused])
      }
      if (!running) return coldLaunch(target, [])
      const open: LifecycleEffect = { effect: `open`, screen: target.screen, params: target.params }
      const next: LifecycleState = { phase: `foreground`, screen: target.screen, params: target.params }
      return { state: next, effects: state.phase === `background` ? [open, { effect: `sources`, live: true }, { effect: `flush` }] : [open] }
    }
  }
}
