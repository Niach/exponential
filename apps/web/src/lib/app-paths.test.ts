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

describe(`phonePathFor (#EXP-2027)`, () => {
  it(`forwards a team-less page into the default team`, () => {
    expect(phonePathFor(`/home`, `acme`)).toBe(`/t/acme/agent`)
    expect(phonePathFor(`/chats`, `acme`)).toBe(`/t/acme/chats`)
    expect(phonePathFor(`/devices/d-1/settings`, `acme`)).toBe(`/t/acme/devices`)
    expect(phonePathFor(`/settings/security`, `acme`)).toBe(`/t/acme/settings/security`)
    expect(phonePathFor(`/t/acme/boards/web`, `acme`)).toBeNull()
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
