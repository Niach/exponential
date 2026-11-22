import { describe, expect, it } from "vitest"
import { settingsSidebarScope } from "@/components/team/settings-sidebar"

// #EXP-2027: the settings panel reads its nav off the URL.
describe(`settingsSidebarScope`, () => {
  it(`opens the Personal nav on team-less settings`, () => {
    expect(settingsSidebarScope(`/settings`)).toEqual({ kind: `personal` })
    expect(settingsSidebarScope(`/settings/assistant`)).toEqual({ kind: `personal` })
  })

  it(`opens a team's nav on its settings`, () => {
    expect(settingsSidebarScope(`/t/acme/settings`)).toEqual({ kind: `team`, teamSlug: `acme` })
    expect(settingsSidebarScope(`/t/acme/settings/labels`)).toEqual({ kind: `team`, teamSlug: `acme` })
  })

  it(`is null elsewhere`, () => {
    expect(settingsSidebarScope(`/home`)).toBeNull()
    expect(settingsSidebarScope(`/t/acme/boards/web/issues`)).toBeNull()
  })
})
