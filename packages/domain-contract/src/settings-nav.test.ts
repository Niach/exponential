import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/settings-nav.json" with { type: "json" }
import { SETTINGS_NAV_PHONE_ROWS, SETTINGS_NAV_ROWS, settingsNavVisible } from "./settings-nav"

describe(`settings-nav.json`, () => {
  it(`phoneRows are the rows marked phone, in row order`, () => {
    expect(SETTINGS_NAV_PHONE_ROWS).toEqual(fixture.rows.filter((r) => r.phone).map((r) => r.id))
  })
  it(`ids are unique`, () => {
    expect(new Set(SETTINGS_NAV_ROWS.map((r) => r.id)).size).toBe(SETTINGS_NAV_ROWS.length)
  })
  it(`visibility follows the viewer`, () => {
    const general = SETTINGS_NAV_ROWS.find((r) => r.id === `general`)!
    const billing = SETTINGS_NAV_ROWS.find((r) => r.id === `billing`)!
    const member = { canManageTeam: false, canManageWidgets: false, isCloud: true }
    const owner = { canManageTeam: true, canManageWidgets: true, isCloud: false }
    expect(settingsNavVisible(general, member)).toBe(false)
    expect(settingsNavVisible(general, owner)).toBe(true)
    expect(settingsNavVisible(billing, owner)).toBe(false)
    expect(settingsNavVisible(billing, { ...owner, isCloud: true })).toBe(true)
  })
})
