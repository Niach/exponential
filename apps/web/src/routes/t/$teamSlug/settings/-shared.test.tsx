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

describe(`SETTINGS_NAV General visibility`, () => {
  const solo: SettingsNavContext = { isCloud: false, solo: true }
  const team: SettingsNavContext = { isCloud: false, solo: false }

  it(`stays visible for a solo owner so the Danger Zone is reachable`, () => {
    expect(general.visible(permissionsFor(`owner`), solo)).toBe(true)
  })

  it(`is visible for an owner with teammates`, () => {
    expect(general.visible(permissionsFor(`owner`), team)).toBe(true)
  })

  it(`stays hidden for plain members`, () => {
    expect(general.visible(permissionsFor(`member`), solo)).toBe(false)
    expect(general.visible(permissionsFor(`member`), team)).toBe(false)
  })

  it(`makes General the /settings landing section for a solo owner`, () => {
    expect(firstVisible(permissionsFor(`owner`), solo)?.to).toBe(
      `/t/$teamSlug/settings/general`
    )
    expect(firstVisible(permissionsFor(`member`), solo)?.to).toBe(
      `/t/$teamSlug/settings/members`
    )
  })
})
