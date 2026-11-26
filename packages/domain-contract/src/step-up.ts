// #EXP-2143 follow-up: "Confirm it's you" — the step-up sheet that answers a
// SESSION_NOT_FRESH refusal in place (fixtures/step-up.json is the spec).
// The TS reference of the reducer every client replays (web lib/step-up,
// desktop domain::step_up, iOS ExpCore StepUp, Android domain/StepUp) and of
// the server's method order (lib/auth/step-up.ts).
import fixture from "../fixtures/step-up.json" with { type: "json" }

export type StepUpMethod = `passkey` | `email` | `password`
export type StepUpSheet = `closed` | `open` | `busy`
export type StepUpAction = `idle` | `waiting` | `resume` | `dropped` | `failed`

export type StepUpState = {
  sheet: StepUpSheet
  method: StepUpMethod | null
  methods: StepUpMethod[]
  error: string | null
  action: StepUpAction
  retried: boolean
}

export type StepUpEvent =
  | { on: `refused`; code: string; methods: StepUpMethod[] }
  | { on: `choose`; method: StepUpMethod }
  | { on: `submit` }
  | { on: `proved` }
  | { on: `rejected`; code: string }
  | { on: `cancel` }
  | { on: `done` }

export const STEP_UP_NOT_FRESH = `SESSION_NOT_FRESH`
/** The person dismissed the system passkey prompt: no error. */
export const STEP_UP_CANCELLED = `CANCELLED`
export const STEP_UP_COPY = fixture.copy
export const STEP_UP_ERRORS: Record<string, string> = fixture.errors

export const STEP_UP_INITIAL: StepUpState = {
  sheet: `closed`,
  method: null,
  methods: [],
  error: null,
  action: `idle`,
  retried: false,
}

/** The sentence for a refused proof (`CANCELLED` = none). */
export function stepUpError(code: string): string | null {
  if (code === STEP_UP_CANCELLED) return null
  return STEP_UP_ERRORS[code] ?? STEP_UP_ERRORS.default!
}

export function stepUpReduce(state: StepUpState, event: StepUpEvent): StepUpState {
  switch (event.on) {
    case `refused`: {
      if (event.code !== STEP_UP_NOT_FRESH) {
        return { ...state, sheet: `closed`, action: `failed`, error: null }
      }
      if (state.retried || event.methods.length === 0) {
        return {
          ...state,
          sheet: `closed`,
          action: `failed`,
          error: STEP_UP_COPY.notFresh,
          methods: state.retried ? state.methods : [],
        }
      }
      return {
        ...state,
        sheet: `open`,
        method: event.methods[0]!,
        methods: [...event.methods],
        error: null,
        action: `waiting`,
      }
    }
    case `choose`:
      if (state.sheet !== `open` || !state.methods.includes(event.method)) return state
      return { ...state, method: event.method, error: null }
    case `submit`:
      if (state.sheet !== `open`) return state
      return { ...state, sheet: `busy`, error: null }
    case `proved`:
      if (state.sheet !== `busy`) return state
      return { ...state, sheet: `closed`, error: null, action: `resume`, retried: true }
    case `rejected`:
      if (state.sheet !== `busy`) return state
      return { ...state, sheet: `open`, error: stepUpError(event.code) }
    case `cancel`:
      if (state.sheet === `closed`) return state
      return { ...state, sheet: `closed`, error: null, action: `dropped` }
    case `done`:
      return { ...state, action: `idle`, retried: false }
  }
}

/** The server's order of an account's ways to prove it is them. */
export function stepUpMethods(
  account: { passkeys: number; password: boolean },
  instance: { passkey: boolean; email: boolean; password: boolean }
): StepUpMethod[] {
  const methods: StepUpMethod[] = []
  if (instance.passkey && account.passkeys > 0) methods.push(`passkey`)
  if (instance.email) methods.push(`email`)
  if (instance.password && account.password) methods.push(`password`)
  return methods
}
