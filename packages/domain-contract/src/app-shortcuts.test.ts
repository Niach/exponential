import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/app-shortcuts.json" with { type: "json" }
import { appShortcuts, chatEntries, launcherShortcutId, parseAppShortcut, type AppShortcutsInput } from "./app-shortcuts"

describe(`appShortcuts`, () => {
  for (const entry of fixture.listCases) {
    test(entry.name, () => expect(appShortcuts(entry.input as AppShortcutsInput)).toEqual(entry.expected))
  }
})

describe(`chatEntries`, () => {
  for (const entry of fixture.chatEntryCases) {
    test(entry.name, () => expect(chatEntries(entry.input as AppShortcutsInput)).toEqual(entry.expected))
  }
})

describe(`parseAppShortcut`, () => {
  for (const entry of fixture.parseCases) {
    test(entry.name, () => expect(parseAppShortcut(entry.input)).toEqual(entry.expected as never))
  }
})

describe(`launcherShortcutId`, () => {
  for (const entry of fixture.launcherCases) {
    test(entry.name, () => expect(launcherShortcutId(entry.input)).toEqual(entry.expected))
  }
})
