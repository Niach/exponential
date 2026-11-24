// #EXP-2109 (settings-nav.json `nav.phone`): a team's phone settings list.
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/settings-nav.json"
import { SETTINGS_NAV_ROWS, settingsNavVisible } from "@exp/domain-contract/settings-nav"
import { teamPageRows } from "./phone-settings"

const idsFor = (isOwner: boolean) =>
  teamPageRows(
    (row) => settingsNavVisible(row, { isOwner, isCloud: false }),
    () => true
  ).map((entry) => (entry.kind === `row` ? entry.row.id : `page:${entry.item.id}`))

describe(`a team's phone settings rows`, () => {
  it.each([true, false])(`no Apps, Devices, personal or injected rows (owner %s)`, (isOwner) => {
    const ids = idsFor(isOwner)
    expect(ids.filter((id) => id.includes(`app`) || id.includes(`@`))).toEqual([])
    expect(ids).not.toContain(`devices`)
    for (const id of fixture.nav.phone.settings) expect(ids).not.toContain(`page:${id}`)
  })

  it.each([true, false])(`the phones' rows first, the web-only rows after them (owner %s)`, (isOwner) => {
    const expected = fixture.nav.phone.team.filter((id) =>
      settingsNavVisible(SETTINGS_NAV_ROWS.find((row) => row.id === id)!, { isOwner, isCloud: false })
    )
    expect(idsFor(isOwner).slice(0, expected.length)).toEqual(expected)
  })
})
