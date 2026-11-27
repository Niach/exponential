import { describe, expect, test } from "bun:test"
import fixture from "../fixtures/system-settings.json" with { type: "json" }
import { SETTINGS_MODULES, settingsGrid, settingsNavigate, type SettingsEvent } from "./settings"

describe(`system-settings.json grid (#EDE-273)`, () => {
  test(`has its cases`, () => expect(fixture.grid.length).toBeGreaterThanOrEqual(8))
  for (const c of fixture.grid) test(c.name, () => expect(settingsGrid(c.query, c.installed)).toEqual(c.expected as ReturnType<typeof settingsGrid>))
})

describe(`system-settings.json navigate (#EDE-273)`, () => {
  test(`has its cases`, () => expect(fixture.navigate.length).toBeGreaterThanOrEqual(20))
  for (const c of fixture.navigate) test(c.name, () => expect(settingsNavigate(c.state, c.event as SettingsEvent, c.installed)).toEqual(c.expected as ReturnType<typeof settingsNavigate>))
})

describe(`system-settings.json catalogue`, () => {
  test(`ids are unique and aliases name modules`, () => {
    const ids = SETTINGS_MODULES.map((m) => m.id)
    expect(new Set(ids).size).toBe(ids.length)
    for (const m of SETTINGS_MODULES) if (m.opens) expect(ids).toContain(m.opens)
    for (const target of Object.values(fixture.aliases)) expect(ids).toContain(target)
    for (const m of SETTINGS_MODULES) if (m.kind === `launch`) expect(m.command).toBeTruthy()
  })
})
