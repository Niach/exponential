// #EXP-1381 (owner 2026-10-24): the phones' Team-page settings rows, each
// opening its OWN page. Locked by `fixtures/settings-nav.json` (its
// `$comment` holds the rules); mirrors: iOS ExpCore `SettingsNav` and Android
// `domain.SettingsNav`, both generated from the fixture.

import fixture from "../fixtures/settings-nav.json" with { type: "json" }

export type SettingsNavVisibility = `all` | `owner` | `ownerCloud`
export type SettingsNavCount = `members` | `boards` | `agents`

export interface SettingsNavRow {
  id: string
  label: string
  icon: string
  group: string
  visible: SettingsNavVisibility
  phone: boolean
  count?: SettingsNavCount
}

export const SETTINGS_NAV_ROWS = fixture.rows as SettingsNavRow[]
export const SETTINGS_NAV_PHONE_ROWS = fixture.phoneRows
export const SETTINGS_NAV_TEST_IDS = fixture.testIds

export function settingsNavRow(id: string): SettingsNavRow | undefined {
  return SETTINGS_NAV_ROWS.find((row) => row.id === id)
}

/** `visible`, the same on every client (`visibleCases`): `isOwner` =
 *  teamRole owner, `cloud` = a cloud instance. */
export function settingsNavVisible(
  row: SettingsNavRow,
  viewer: { isOwner: boolean; isCloud: boolean }
): boolean {
  switch (row.visible) {
    case `all`:
      return true
    case `owner`:
      return viewer.isOwner
    case `ownerCloud`:
      return viewer.isOwner && viewer.isCloud
  }
}

/** The phone rows a viewer sees, in order. */
export function settingsNavPhoneRows(viewer: { isOwner: boolean; isCloud: boolean }): string[] {
  return SETTINGS_NAV_PHONE_ROWS.filter((id) => {
    const row = settingsNavRow(id)
    return row !== undefined && settingsNavVisible(row, viewer)
  })
}
