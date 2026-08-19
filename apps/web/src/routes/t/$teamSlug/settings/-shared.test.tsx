import { describe, expect, it } from "vitest"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import {
  SETTINGS_NAV,
  type SettingsNavContext,
} from "@/routes/t/$teamSlug/settings/-shared"

const permissionsFor = (role: `owner` | `member`): TeamPermissions => {
  const isOwner = role === `owner`
  return {
    isAuthed: true,
    isMember: true,
    isAdmin: false,
    isModerator: true,
    isOwner,
    canCreate: true,
    canMutateIssue: () => true,
    canManageTeam: isOwner,
    canDeleteBoard: isOwner,
    canManageMembers: isOwner,
    canManageWidgets: isOwner,
    plan: null,
    billingPlan: null,
    canAddMoreMembers: true,
    canAddMoreBoards: true,
    canAddMoreStorage: true,
  }
}

const items = SETTINGS_NAV.flatMap((group) => group.items)
const general = items.find((item) => item.label === `General`)!

// Mirrors settings/index.tsx: bare /settings lands on the first visible item.
const firstVisible = (
  permissions: TeamPermissions,
  context: SettingsNavContext
) => items.find((item) => item.visible(permissions, context))

// EXP-314: Statuses sits in the Team group right after Labels and, like
// Labels, is visible to every member (the router gates writes).
describe(`SETTINGS_NAV Statuses entry`, () => {
  const team: SettingsNavContext = { isCloud: false }

  it(`follows Labels in the Team group`, () => {
    const teamGroup = SETTINGS_NAV.find((group) => group.group === `Team`)!
    const labels = teamGroup.items.findIndex((item) => item.label === `Labels`)
    const statuses = teamGroup.items.findIndex(
      (item) => item.label === `Statuses`
    )
    expect(statuses).toBe(labels + 1)
    expect(teamGroup.items[statuses].to).toBe(`/t/$teamSlug/settings/statuses`)
  })

  it(`is visible to owners and plain members alike`, () => {
    const statuses = items.find((item) => item.label === `Statuses`)!
    expect(statuses.visible(permissionsFor(`owner`), team)).toBe(true)
    expect(statuses.visible(permissionsFor(`member`), team)).toBe(true)
  })
})

// EXP-238: the Personal group merges account settings into the one settings
// surface. Always visible, and LAST — the index redirect must keep landing
// on a team section, never a personal one.
describe(`SETTINGS_NAV Personal group`, () => {
  const team: SettingsNavContext = { isCloud: false }
  const personal = SETTINGS_NAV.find((group) => group.group === `Personal`)!

  it(`is the last group with Account, Notifications, and API keys`, () => {
    expect(SETTINGS_NAV[SETTINGS_NAV.length - 1]).toBe(personal)
    expect(personal.items.map((item) => item.label)).toEqual([
      `Account`,
      `Notifications`,
      `API keys`,
    ])
    expect(personal.items.map((item) => item.to)).toEqual([
      `/t/$teamSlug/settings/account`,
      `/t/$teamSlug/settings/notifications`,
      `/t/$teamSlug/settings/api-keys`,
    ])
  })

  it(`is visible to owners and plain members alike`, () => {
    for (const item of personal.items) {
      expect(item.visible(permissionsFor(`owner`), team)).toBe(true)
      expect(item.visible(permissionsFor(`member`), team)).toBe(true)
    }
  })
})

describe(`SETTINGS_NAV General visibility`, () => {
  const team: SettingsNavContext = { isCloud: false }

  it(`is visible for owners so the Danger Zone is reachable`, () => {
    expect(general.visible(permissionsFor(`owner`), team)).toBe(true)
  })

  it(`stays hidden for plain members`, () => {
    expect(general.visible(permissionsFor(`member`), team)).toBe(false)
  })

  it(`makes General the /settings landing section for owners`, () => {
    expect(firstVisible(permissionsFor(`owner`), team)?.to).toBe(
      `/t/$teamSlug/settings/general`
    )
    expect(firstVisible(permissionsFor(`member`), team)?.to).toBe(
      `/t/$teamSlug/settings/members`
    )
  })
})

// EXP-557 per-user sharing: every member manages their own GitHub connection
// in the Repositories section, so it is member-visible like Members/Labels.
describe(`SETTINGS_NAV Repositories visibility (EXP-557)`, () => {
  const team: SettingsNavContext = { isCloud: false }
  const repositories = items.find((item) => item.label === `Repositories`)!

  it(`is visible to owners and plain members alike`, () => {
    expect(repositories.visible(permissionsFor(`owner`), team)).toBe(true)
    expect(repositories.visible(permissionsFor(`member`), team)).toBe(true)
  })
})
