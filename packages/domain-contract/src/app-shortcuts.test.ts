import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/app-shortcuts.json" with { type: "json" }
import { appShortcuts, parseAppShortcut } from "./app-shortcuts"

describe(`appShortcuts`, () => {
  for (const entry of fixture.listCases) {
    test(entry.name, () => expect(appShortcuts(entry.input)).toEqual(entry.expected))
  }
})

describe(`parseAppShortcut`, () => {
  for (const entry of fixture.parseCases) {
    test(entry.name, () => expect(parseAppShortcut(entry.input)).toEqual(entry.expected as never))
  }
})
