import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/settings-nav.json" with { type: "json" }
import { SETTINGS_NAV_PHONE_ROWS, SETTINGS_NAV_ROWS, settingsNavPhoneRows, settingsNavVisible } from "./settings-nav"

describe(`settings-nav.json`, () => {
  it(`phoneRows are the rows marked phone, in row order`, () => {
    expect(SETTINGS_NAV_PHONE_ROWS).toEqual(fixture.rows.filter((r) => r.phone).map((r) => r.id))
  })
  it(`ids are unique`, () => {
    expect(new Set(SETTINGS_NAV_ROWS.map((r) => r.id)).size).toBe(SETTINGS_NAV_ROWS.length)
  })
  for (const c of fixture.visibleCases) {
    it(`visibleCases: ${c.name}`, () => {
      const viewer = { isOwner: c.role === `owner`, isCloud: c.cloud }
      expect(SETTINGS_NAV_ROWS.filter((r) => settingsNavVisible(r, viewer)).map((r) => r.id)).toEqual(c.expected)
      expect(settingsNavPhoneRows(viewer)).toEqual(c.expectedPhone)
    })
  }
})
