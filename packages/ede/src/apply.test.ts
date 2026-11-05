import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/apply.json" with { type: "json" }
import { hydrate, reduce, type ApplyEvent, type ApplyState, type Effect } from "./apply"

/** Compares `state` (a partial per document) against the reducer's state. */
function pick(state: ApplyState, expected: Record<string, Record<string, unknown>>): Record<string, Record<string, unknown>> {
  const out: Record<string, Record<string, unknown>> = {}
  for (const [name, fields] of Object.entries(expected)) {
    const d = state.documents[name] as unknown as Record<string, unknown> | undefined
    out[name] = Object.fromEntries(Object.keys(fields).map((k) => [k, d?.[k]]))
  }
  return out
}

describe(`apply.json`, () => {
  for (const entry of fixture.cases) {
    test(entry.name, () => {
      let state = hydrate(entry.given as Parameters<typeof hydrate>[0])
      for (const step of entry.steps) {
        const outcome = reduce(state, step.event as ApplyEvent)
        if (step.effects) expect(outcome.effects).toEqual(step.effects as Effect[])
        if (step.state) expect(pick(outcome.state, step.state as Record<string, Record<string, unknown>>)).toEqual(step.state)
        state = outcome.state
      }
    })
  }
})
