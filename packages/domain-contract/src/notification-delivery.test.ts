import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/notification-delivery.json" with { type: "json" }
import {
  androidChannel,
  dismissMatches,
  dismissPayload,
  notificationPresentation,
  notificationSource,
  notificationThread,
  settingsRows,
  sourceAllowed,
  toggleSourcePref,
  type PresentationInput,
  type ReadRow,
} from "./notification-delivery"

/** The fixture's `rowsOfRule`. */
function rowsOf(input: { count: number; threads?: string[]; threadCount?: number }): ReadRow[] {
  return Array.from({ length: input.count }, (_, i) => ({
    id: `n${i}`,
    thread: input.threads ? input.threads[i % input.threads.length] : `t${i % (input.threadCount ?? 1)}`,
  }))
}

describe(`notificationThread`, () => {
  for (const entry of fixture.threadCases) {
    test(entry.name, () => expect(notificationThread(entry.input)).toBe(entry.expected))
  }
})

describe(`notificationPresentation`, () => {
  for (const entry of fixture.presentCases) {
    test(entry.name, () =>
      expect(notificationPresentation(entry.input as PresentationInput)).toBe(entry.expected as never)
    )
  }
})

describe(`dismissPayload`, () => {
  for (const entry of fixture.dismissCases) {
    test(entry.name, () => {
      const input = entry.input as {
        rows?: ReadRow[]
        rowsOf?: { count: number; threads?: string[]; threadCount?: number }
        all: boolean
        teamId: string | null
      }
      const rows = input.rows ?? rowsOf(input.rowsOf!)
      expect(dismissPayload({ rows, all: input.all, teamId: input.teamId })).toEqual(entry.expected as never)
    })
  }
})

describe(`dismissMatches`, () => {
  for (const entry of fixture.matchCases) {
    test(entry.name, () =>
      expect(dismissMatches(entry.input.payload as Record<string, string>, entry.input.delivered)).toBe(entry.expected)
    )
  }
})

describe(`notificationSource`, () => {
  for (const entry of fixture.sourceCases) {
    test(entry.name, () => expect(notificationSource(entry.input)).toBe(entry.expected))
  }
})

describe(`sourceAllowed`, () => {
  for (const entry of fixture.allowCases) {
    test(entry.name, () => expect(sourceAllowed(entry.input.prefs as never, entry.input.source)).toBe(entry.expected))
  }
})

describe(`toggleSourcePref`, () => {
  for (const entry of fixture.toggleCases) {
    test(entry.name, () =>
      expect(toggleSourcePref(entry.input.prefs as never, entry.input.key, entry.input.on)).toEqual(entry.expected as never)
    )
  }
})

describe(`settingsRows`, () => {
  for (const entry of fixture.settingsCases) {
    test(entry.name, () => expect(settingsRows(entry.input.teams, entry.input.agents)).toEqual(entry.expected))
  }
})

describe(`androidChannel`, () => {
  for (const entry of fixture.channelCases) {
    test(entry.name, () => expect(androidChannel(entry.input.source, entry.input.sourceName)).toEqual(entry.expected))
  }
})
