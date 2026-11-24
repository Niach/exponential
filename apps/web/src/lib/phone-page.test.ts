import { describe, expect, it } from "vitest"
import { phonePageOf, phonePagePlace } from "./phone-page"

describe(`phonePageOf (#EXP-2109)`, () => {
  it(`names the bar's pages`, () => {
    expect(phonePageOf(`/chats`)).toBe(`chats`)
    expect(phonePageOf(`/teams/`)).toBe(`teams`)
    expect(phonePageOf(`/t/acme/team`)).toBe(`team`)
    expect(phonePageOf(`/t/acme/boards/web`)).toBe(`board`)
    expect(phonePageOf(`/t/acme/settings`)).toBe(`team-settings`)
    expect(phonePageOf(`/t/acme/settings/boards/b-1`)).toBe(`team-settings`)
    expect(phonePageOf(`/assistant`)).toBe(`assistant`)
    expect(phonePageOf(`/settings/account`)).toBe(`settings-page`)
    expect(phonePageOf(`/devices/d-1`)).toBe(`device`)
  })
  it(`a detail shows no bar`, () => {
    for (const path of [`/home`, `/inbox`, `/needs-you`, `/t/acme/boards/web/issues/WEB-1`, `/t/acme/sessions/s`, `/t/acme/apps/crm`, `/t/acme/new/board`]) {
      expect(phonePagePlace(path)).toEqual({ tab: null, bar: false })
    }
    expect(phonePagePlace(`/t/acme/boards/web`)).toEqual({ tab: `teams`, bar: true })
  })
})
