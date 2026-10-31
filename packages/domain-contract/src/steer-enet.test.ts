// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/steer-enet.json" with { type: "json" }
import {
  encodeSteerEnvelope,
  parseSteerEnvelope,
  steerBatches,
  steerOfflineRedial,
  steerShowConnectionLost,
  type SteerRedialPhase,
  type SteerRedialTrigger,
  steerViewerAllowed,
} from "./steer-enet"

describe(`steer over eNet (fixtures/steer-enet.json)`, () => {
  for (const c of fixture.access) {
    test(`access: ${c.name}`, () => {
      expect(steerViewerAllowed(c)).toBe(c.allowed)
    })
  }
  for (const c of fixture.parse) {
    test(`parse: ${c.name}`, () => {
      const body = `bodyText` in c ? (c.bodyText as string) : JSON.stringify(c.body)
      const envelope = parseSteerEnvelope(new TextEncoder().encode(body))
      expect(envelope !== null).toBe(c.ok)
      if (envelope) {
        expect(envelope.sessionId).toBe(c.sessionId as string)
        expect(envelope.frames.map((f) => f.t)).toEqual(c.types as string[])
      }
    })
  }
  for (const c of fixture.batch) {
    test(`batch: ${c.name}`, () => {
      expect(steerBatches(c.sizes, c)).toEqual({ groups: c.groups, dropped: c.dropped })
    })
  }
  for (const c of fixture.offlineRedial) {
    test(`offlineRedial: ${c.name}`, () => {
      expect(steerOfflineRedial(c.phase as SteerRedialPhase, c.trigger as SteerRedialTrigger)).toBe(c.redial)
    })
  }
  for (const c of fixture.lostBannerCases) {
    test(`lostBanner: ${c.name}`, () => {
      expect(steerShowConnectionLost(c.downMs, c.failedRedials)).toBe(c.show)
    })
  }
  test(`encode round-trips`, () => {
    const frames = [{ t: `join`, channel: `activity` }]
    expect(parseSteerEnvelope(encodeSteerEnvelope(`s1`, frames))?.frames).toEqual(frames)
  })
})
