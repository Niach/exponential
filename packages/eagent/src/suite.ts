// The scenario replay every implementation runs (this package's TS reference,
// eagent-hub-core through its WASM build): one replayer, one set of assertions.
import { describe, expect, test } from "bun:test"

import bus from "../fixtures/bus.json" with { type: "json" }
import incidents from "../fixtures/incidents.json" with { type: "json" }
import reuse from "../fixtures/reuse.json" with { type: "json" }
import tree from "../fixtures/tree.json" with { type: "json" }
import wakeups from "../fixtures/wakeups.json" with { type: "json" }
import watchdog from "../fixtures/watchdog.json" with { type: "json" }
import type { HubEvent, HubState, Outcome } from "./model"
import { pick, replay, type FixtureCase } from "./replay"

export const SCENARIOS = { tree, bus, wakeups, watchdog, reuse, incidents } as const

export function replayScenarios(
  reduce: (state: HubState, event: HubEvent) => Outcome,
  hydrate?: (given: NonNullable<FixtureCase[`given`]>) => HubState
) {
  for (const [file, fixture] of Object.entries(SCENARIOS)) {
    describe(`${file}.json`, () => {
      for (const entry of fixture.cases as unknown as FixtureCase[]) {
        test(entry.name, () => {
          const run = replay(entry, reduce, hydrate)
          for (const errors of run.envelopeErrors) expect(errors).toEqual([])
          entry.steps.forEach((step, index) => {
            const outcome = run.outcomes[index]!
            const label = `step ${index + 1} (${step.event.kind})`
            if (step.failure) {
              expect({ label, failure: outcome.failure }).toEqual({ label, failure: step.failure })
              return
            }
            expect({ label, failure: outcome.failure }).toEqual({ label, failure: undefined })
            if (step.effects) expect({ label, effects: outcome.effects }).toEqual({ label, effects: step.effects })
            if (step.result) expect({ label, result: outcome.result }).toEqual({ label, result: step.result })
          })
          for (const [id, want] of Object.entries(entry.expect?.sessions ?? {})) {
            expect({ id, row: pick(run.state.sessions[id] as never, want) }).toEqual({ id, row: want })
          }
          for (const [id, want] of Object.entries(entry.expect?.messages ?? {})) {
            expect({ id, row: pick(run.state.messages.find((m) => m.id === id) as never, want) }).toEqual({ id, row: want })
          }
          for (const [id, want] of Object.entries(entry.expect?.agents ?? {})) {
            expect({ id, row: pick(run.state.agents[id] as never, want) }).toEqual({ id, row: want })
          }
        })
      }
    })
  }
}
