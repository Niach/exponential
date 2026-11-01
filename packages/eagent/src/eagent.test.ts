// The fixtures are the contract; this TypeScript reference replays them
// (eagent-hub-core does the same in Rust, the server its own pure parts).
import { describe, expect, test } from "bun:test"

import { isValidId } from "@exp/ecore"
import reuse from "../fixtures/reuse.json" with { type: "json" }
import watchdog from "../fixtures/watchdog.json" with { type: "json" }
import { silentMinutes } from "./copy"
import { cleanLiveSessionIds, reduce, reuseRefusal } from "./reduce"
import { replayScenarios, SCENARIOS } from "./suite"

replayScenarios(reduce)

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
