// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/steer-enet.json" with { type: "json" }
import { encodeSteerEnvelope, parseSteerEnvelope, steerBatches, steerViewerAllowed } from "./steer-enet"

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
        expect(envelope.frames.map((f) => f.type)).toEqual(c.types as string[])
      }
    })
  }
  for (const c of fixture.batch) {
    test(`batch: ${c.name}`, () => {
      expect(steerBatches(c.sizes, c)).toEqual({ groups: c.groups, dropped: c.dropped })
    })
  }
  test(`encode round-trips`, () => {
    const frames = [{ type: `join`, channel: `activity` }]
    expect(parseSteerEnvelope(encodeSteerEnvelope(`s1`, frames))?.frames).toEqual(frames)
  })
})
