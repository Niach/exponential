import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/notification-actions.json" with { type: "json" }
import { askTextOptions, notificationActions } from "./notification-actions"

describe(`notificationActions`, () => {
  for (const entry of fixture.actionCases) {
    test(entry.name, () =>
      expect(notificationActions(entry.input as Parameters<typeof notificationActions>[0])).toEqual(
        entry.expected as never
      )
    )
  }
})

describe(`askTextOptions`, () => {
  for (const entry of fixture.optionCases) {
    test(entry.name, () => expect(askTextOptions(entry.input)).toEqual(entry.expected))
  }
})
