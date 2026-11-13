import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/message-delivery.json" with { type: "json" }
import {
  bubbleActions,
  deliveryState,
  outboxMerge,
  ownHandoff,
  type DeliveryState,
  type DeliveryStateInput,
  type HandoffEcho,
  type HandoffRow,
  type OutboxEntry,
  type OutboxRow,
} from "./message-delivery"

describe(`deliveryState`, () => {
  for (const entry of fixture.stateCases) {
    test(entry.name, () => {
      expect(deliveryState(entry.input as DeliveryStateInput)).toBe(entry.expected as DeliveryState)
    })
  }
})

describe(`outboxMerge`, () => {
  for (const entry of fixture.outboxCases) {
    test(entry.name, () => {
      const input = entry.input as { viewerId: string; outbox: OutboxEntry[]; rows: OutboxRow[] }
      expect(outboxMerge(input)).toEqual(entry.expected)
    })
  }
})

describe(`bubbleActions`, () => {
  for (const entry of fixture.actionCases) {
    test(entry.name, () => {
      expect(bubbleActions(entry.input as { state: DeliveryState; stored: boolean })).toEqual(entry.expected)
    })
  }
})

describe(`ownHandoff`, () => {
  for (const entry of fixture.handoffCases) {
    test(entry.name, () => {
      const input = entry.input as { viewerId: string; now: string; feedEmpty: boolean; rows: HandoffRow[]; echoes: HandoffEcho[] }
      expect(ownHandoff(input)).toEqual(entry.expected)
    })
  }
})

test(`every state has a mark and a label`, () => {
  for (const state of [`sending`, `sent`, `read`, `failed`] as const) {
    expect(fixture.marks[state].icon).toBeTruthy()
    expect(fixture.copy.status[state]).toBeTruthy()
  }
})
