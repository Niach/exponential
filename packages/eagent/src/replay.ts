// Replays one scenario case of fixtures/*.json against a reducer: each step
// is an eCore envelope (`id` defaults to `e<n>`, `source` = `fixture`).
// Shared by this package's test and the server's (apps/web eagent-spec).

import { envelopeErrors, ENVELOPE_VERSION, type Failure } from "@exp/ecore"
import { hydrate } from "./reduce"
import type { Effect, HubEvent, HubState, Outcome, StepResult } from "./model"

export interface FixtureStep {
  event: { kind: string; at: number; id?: string; data?: Record<string, unknown> }
  effects?: Effect[]
  result?: StepResult
  failure?: Failure
}

export interface FixtureCase {
  name: string
  given?: Parameters<typeof hydrate>[0]
  steps: FixtureStep[]
  expect?: {
    sessions?: Record<string, Record<string, unknown> | null>
    messages?: Record<string, Record<string, unknown>>
    agents?: Record<string, Record<string, unknown>>
  }
}

export function envelope(step: FixtureStep, index: number): HubEvent {
  const data = step.event.data ?? {}
  return {
    v: ENVELOPE_VERSION,
    id: step.event.id ?? `e${index + 1}`,
    kind: step.event.kind,
    at: step.event.at,
    source: `fixture`,
    sessionId: typeof data.sessionId === `string` ? data.sessionId : null,
    data,
  }
}

export interface Replayed {
  envelopeErrors: string[][]
  outcomes: Outcome[]
  state: HubState
}

/** `hydrateGiven` defaults to the TS reference's; the WASM replay passes eagent-hub-core's. */
export function replay(
  entry: FixtureCase,
  reduce: (state: HubState, event: HubEvent) => Outcome,
  hydrateGiven: (given: NonNullable<FixtureCase[`given`]>) => HubState = hydrate
): Replayed {
  let state = hydrateGiven(entry.given ?? {})
  const outcomes: Outcome[] = []
  const errors: string[][] = []
  entry.steps.forEach((step, index) => {
    const event = envelope(step, index)
    errors.push(envelopeErrors(event))
    const outcome = reduce(state, event)
    outcomes.push(outcome)
    state = outcome.state
  })
  return { envelopeErrors: errors, outcomes, state }
}

/** The fields of `actual` that `expected` names (null = the row must not exist). */
export function pick(actual: Record<string, unknown> | undefined, expected: Record<string, unknown> | null): Record<string, unknown> | null {
  if (expected === null || actual === undefined) return actual ?? null
  return Object.fromEntries(Object.keys(expected).map((key) => [key, actual[key]]))
}
