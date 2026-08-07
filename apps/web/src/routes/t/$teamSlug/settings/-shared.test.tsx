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
    canManageRepos: isOwner,
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
