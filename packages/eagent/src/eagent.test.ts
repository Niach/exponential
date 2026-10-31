// The fixtures are the contract; this TypeScript reference replays them
// (eagent-hub-core does the same in Rust, the server its own pure parts).
import { describe, expect, test } from "bun:test"

import { isValidId } from "@exp/ecore"
import bus from "../fixtures/bus.json" with { type: "json" }
import incidents from "../fixtures/incidents.json" with { type: "json" }
import reuse from "../fixtures/reuse.json" with { type: "json" }
import tree from "../fixtures/tree.json" with { type: "json" }
import wakeups from "../fixtures/wakeups.json" with { type: "json" }
import watchdog from "../fixtures/watchdog.json" with { type: "json" }
import { silentMinutes } from "./copy"
import { cleanLiveSessionIds, reduce, reuseRefusal } from "./reduce"
import { pick, replay, type FixtureCase } from "./replay"

const SCENARIOS = { tree, bus, wakeups, watchdog, reuse, incidents } as const

for (const [file, fixture] of Object.entries(SCENARIOS)) {
  describe(`${file}.json`, () => {
    for (const entry of fixture.cases as unknown as FixtureCase[]) {
      test(entry.name, () => {
        const run = replay(entry, reduce)
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

describe(`reuse.json refusalCases`, () => {
  for (const entry of reuse.refusalCases) {
    test(entry.name, () =>
      expect(reuseRefusal(entry.root as never, entry.children, entry.now, entry.awaitsCaller ?? false)).toBe(entry.expected)
    )
  }
})

describe(`watchdog.json pure cases`, () => {
  for (const entry of watchdog.silentCases) {
    test(`silent ${entry.alive} → ${entry.expected}`, () => expect(silentMinutes(entry.alive ?? NaN, entry.now)).toBe(entry.expected))
  }
  for (const entry of watchdog.liveIdCases) {
    test(`live ids ${JSON.stringify(entry.ids)}`, () => expect(cleanLiveSessionIds(entry.ids)).toEqual(entry.expected))
  }
})

test(`every id a fixture names follows eCore's id rule`, () => {
  const ids = new Set<string>()
  const walk = (value: unknown, key?: string) => {
    if (Array.isArray(value)) return value.forEach((item) => walk(item, key))
    if (value && typeof value === `object`) return Object.entries(value).forEach(([k, v]) => walk(v, k))
    if (typeof value === `string` && key && /^(id|sessionId|parentId|rootId|callerId|watcherId|freshId|deviceId|conversationId|agentId|messageId)$/.test(key)) ids.add(value)
  }
  walk(SCENARIOS)
  for (const id of ids) expect({ id, valid: isValidId(id) }).toEqual({ id, valid: true })
})
