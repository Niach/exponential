import { describe, expect, it } from "vitest"
import { isPersonalSettingsPath, phonePathFor, teamSettingsSlug, widePathFor } from "./app-paths"

describe(`widePathFor (#EXP-2027)`, () => {
  it(`moves the old team-scoped pages to their team-less ones`, () => {
    expect(widePathFor(`/t/acme/agent`)).toBe(`/home`)
    expect(widePathFor(`/t/acme/chats/`)).toBe(`/chats`)
    expect(widePathFor(`/t/acme/needs-you`)).toBe(`/needs-you`)
    expect(widePathFor(`/t/acme/drafts`)).toBe(`/drafts`)
    expect(widePathFor(`/t/acme/devices`)).toBe(`/home`)
    expect(widePathFor(`/t/acme/settings/account`)).toBe(`/settings/account`)
    expect(widePathFor(`/t/acme/settings/assistant`)).toBe(`/settings/assistant`)
    expect(widePathFor(`/t/acme/settings/voice`)).toBe(`/settings/assistant`)
    expect(widePathFor(`/t/acme/settings/agents/new`)).toBe(`/t/acme/new/agent`)
    expect(widePathFor(`/`)).toBe(`/home`)
  })

  it(`leaves what belongs to a team`, () => {
    expect(widePathFor(`/t/acme/boards/web`)).toBeNull()
    expect(widePathFor(`/t/acme/drafts/d-1`)).toBeNull()
    expect(widePathFor(`/t/acme/settings/general`)).toBeNull()
    expect(widePathFor(`/t/acme/sessions/s-1`)).toBeNull()
    expect(widePathFor(`/home`)).toBeNull()
  })
})

describe(`phonePathFor (#EXP-2109)`, () => {
  it(`forwards an old team-scoped page to its team-less one`, () => {
    expect(phonePathFor(`/`)).toBe(`/chats`)
    expect(phonePathFor(`/t/acme`)).toBe(`/t/acme/team`)
    expect(phonePathFor(`/t/acme/apps`)).toBe(`/t/acme/team`)
    expect(phonePathFor(`/t/acme/chats`)).toBe(`/chats`)
    expect(phonePathFor(`/t/acme/agent`)).toBe(`/home`)
    expect(phonePathFor(`/t/acme/devices`)).toBe(`/devices`)
    expect(phonePathFor(`/t/acme/inbox`)).toBe(`/inbox`)
    expect(phonePathFor(`/t/acme/settings/security`)).toBe(`/settings/security`)
  })
  it(`keeps a team's pages and the team-less ones`, () => {
    for (const path of [`/t/acme/team`, `/t/acme/boards/web`, `/t/acme/settings`, `/t/acme/settings/general`, `/t/acme/settings/agents/new`, `/home`, `/chats`, `/devices/d-1`]) {
      expect(phonePathFor(path)).toBeNull()
    }
  })
})

describe(`settings paths`, () => {
  it(`tells personal from team settings`, () => {
    expect(isPersonalSettingsPath(`/settings/account`)).toBe(true)
    expect(isPersonalSettingsPath(`/t/acme/settings/general`)).toBe(false)
    expect(teamSettingsSlug(`/t/acme/settings/general`)).toBe(`acme`)
    expect(teamSettingsSlug(`/t/acme/boards/web`)).toBeNull()
  })
})
