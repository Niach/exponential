import { describe, expect, it } from "bun:test"
import fixture from "../fixtures/settings-nav.json" with { type: "json" }
import {
  SETTINGS_NAV_GROUPS,
  SETTINGS_NAV_PHONE_ROWS,
  SETTINGS_NAV_ROWS,
  settingsDesktopNavs,
  settingsNavGroups,
  settingsNavPage,
  settingsNavPhoneRows,
  settingsNavVisible,
  type SettingsNavPlatform,
} from "./settings-nav"

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
  for (const c of fixture.nav.navCases) {
    it(`navCases: ${c.name}`, () => {
      const viewer = { isOwner: c.role === `owner`, isCloud: c.cloud }
      expect(settingsNavGroups(c.platform as SettingsNavPlatform, viewer)).toEqual(c.expected)
    })
  }
  it(`every nav page id resolves`, () => {
    const ids = [...SETTINGS_NAV_GROUPS.flatMap((g) => g.items), ...Object.values(fixture.nav.children).flat()]
    for (const id of ids.filter((item) => !item.startsWith(`@`))) expect(settingsNavPage(id)).toBeDefined()
  })
  for (const c of fixture.nav.desktopNavCases) {
    it(`desktopNavCases: ${c.name}`, () => {
      expect(
        settingsDesktopNavs({ platform: c.platform as SettingsNavPlatform, isOwner: c.role === `owner`, isCloud: c.cloud })
      ).toEqual(c.expected)
    })
  }
  it(`every desktop nav page id resolves`, () => {
    const navs = fixture.nav.desktopNavs
    const ids = [...navs.personal.flatMap((g) => g.items), ...navs.team.flatMap((g) => g.items), ...navs.device]
    for (const id of ids) expect(settingsNavPage(id)).toBeDefined()
  })
})
