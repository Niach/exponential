// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/steer-enet.json" with { type: "json" }
import {
  encodeSteerEnvelope,
  parseSteerEnvelope,
  type SteerCarrierAction,
  type SteerCarrierEvent,
  type SteerCarrierState,
  steerCarrierStep,
  steerConnAggregate,
  steerConnEventValid,
  steerViewErrorClass,
  steerStartingRetryCapMs,
  type SteerConnRow,
  steerBatches,
  steerOfflineRedial,
  steerNoFeedState,
  steerNoFeedInputs,
  steerRowWakes,
  type SteerWakeRow,
  type SteerNoFeedInputRow,
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
  for (const c of fixture.noFeed) {
    test(`noFeed: ${c.name}`, () => {
      expect(steerNoFeedState(c)).toBe(c.state)
    })
  }
  for (const c of fixture.noFeedInputCases) {
    test(`noFeedInputs: ${c.name}`, () => {
      expect(steerNoFeedInputs(c.messages as SteerNoFeedInputRow[], c.max).map((m) => m.id)).toEqual(c.expected)
    })
  }
  for (const c of fixture.rowWakeCases) {
    test(`rowWake: ${c.name}`, () => {
      expect(steerRowWakes(c.from as SteerWakeRow | null, c.to as SteerWakeRow)).toBe(c.wake)
    })
  }
  test(`encode round-trips`, () => {
    const frames = [{ t: `join`, channel: `activity` }]
    expect(parseSteerEnvelope(encodeSteerEnvelope(`s1`, frames))?.frames).toEqual(frames)
  })
})

describe(`sealed relay fallback + connection analytics (fixtures/steer-enet.json)`, () => {
  for (const c of fixture.carrierCases) {
    test(`carrier: ${c.name}`, () => {
      expect(steerCarrierStep(c.state as SteerCarrierState, c.event as SteerCarrierEvent)).toEqual({
        state: c.next as SteerCarrierState,
        actions: c.actions as SteerCarrierAction[],
      })
    })
  }
  for (const c of fixture.startingRetryCases) {
    test(`starting retry cap after ${c.retries}`, () => {
      expect(steerStartingRetryCapMs(c.retries)).toBe(c.capMs)
    })
  }
  for (const c of fixture.connStats.validCases) {
    test(`connStats valid: ${c.name}`, () => {
      expect(steerConnEventValid(c.event)).toBe(c.valid)
    })
  }
  for (const c of fixture.connStats.errorClassCases) {
    test(`connStats errorClass: ${c.name}`, () => {
      expect(steerViewErrorClass(c) ?? null).toBe(c.errorClass)
    })
  }
  for (const c of fixture.connStats.aggregateCases) {
    test(`connStats aggregate: ${c.name}`, () => {
      const { views, successRate, p50FirstFrameMs, p95FirstFrameMs, fallbackRate } = c
      expect(steerConnAggregate(c.rows as SteerConnRow[])).toEqual({
        views,
        successRate,
        p50FirstFrameMs,
        p95FirstFrameMs,
        fallbackRate,
      })
    })
  }
})
