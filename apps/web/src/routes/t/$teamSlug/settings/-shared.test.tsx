import { describe, expect, it } from "vitest"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import {
  flattenSettingsNav,
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

const items = SETTINGS_NAV.flatMap((group) => flattenSettingsNav(group.items))
const general = items.find((item) => item.label === `General`)!

// Mirrors settings/index.tsx: bare /settings lands on the first visible item.
const firstVisible = (
  permissions: TeamPermissions,
  context: SettingsNavContext
) => items.find((item) => item.visible(permissions, context))

// EXP-630: Issues sits in the Team group after Members with Labels and
// Statuses (EXP-314) as its sub-pages; all three are visible to every member
// (the routers gate writes).
describe(`SETTINGS_NAV Issues entry`, () => {
  const team: SettingsNavContext = { isCloud: false }
  const teamGroup = SETTINGS_NAV.find((group) => group.group === `Team`)!
  const issues = teamGroup.items.find((item) => item.label === `Issues`)!

  it(`follows Members in the Team group and nests Labels then Statuses`, () => {
    const members = teamGroup.items.findIndex((item) => item.label === `Members`)
    expect(teamGroup.items.indexOf(issues)).toBe(members + 1)
    expect(issues.to).toBe(`/t/$teamSlug/settings/issues`)
    expect(issues.children?.map((item) => [item.label, item.to])).toEqual([
      [`Labels`, `/t/$teamSlug/settings/labels`],
      [`Statuses`, `/t/$teamSlug/settings/statuses`],
    ])
    // Neither is a top-level entry any more.
    expect(teamGroup.items.some((item) => item.label === `Labels`)).toBe(false)
    expect(teamGroup.items.some((item) => item.label === `Statuses`)).toBe(false)
  })

  it(`flattens with the sub-pages right after their parent`, () => {
    const flat = flattenSettingsNav(teamGroup.items).map((item) => item.label)
    expect(flat.slice(flat.indexOf(`Issues`), flat.indexOf(`Issues`) + 3)).toEqual([`Issues`, `Labels`, `Statuses`])
  })

  it(`is visible to owners and plain members alike, sub-pages included`, () => {
    for (const item of [issues, ...(issues.children ?? [])]) {
      expect(item.visible(permissionsFor(`owner`), team)).toBe(true)
      expect(item.visible(permissionsFor(`member`), team)).toBe(true)
    }
  })
})

// SLOP-5: the admin-ish, owner-only sections (Storage, Import, Archived
// boards) close the nav as one Advanced group — after Personal, so the index
// redirect keeps landing on a team section.
describe(`SETTINGS_NAV Advanced group (SLOP-5)`, () => {
  const team: SettingsNavContext = { isCloud: false }
  const advanced = SETTINGS_NAV.find((group) => group.group === `Advanced`)!

  it(`is the last group with Storage, Import and Archived boards`, () => {
    expect(SETTINGS_NAV[SETTINGS_NAV.length - 1]).toBe(advanced)
    expect(advanced.items.map((item) => [item.label, item.to])).toEqual([
      [`Storage`, `/t/$teamSlug/settings/storage`],
      [`Import`, `/t/$teamSlug/settings/import`],
      [`Archived boards`, `/t/$teamSlug/settings/boards/archived`],
    ])
    // None of them is a Team or Boards entry any more.
    for (const group of SETTINGS_NAV.filter((g) => g !== advanced)) {
      expect(
        group.items.some((item) =>
          [`Storage`, `Import`, `Archived boards`].includes(item.label)
        ),
        group.group
      ).toBe(false)
    }
  })

  it(`is owner-only`, () => {
    for (const entry of advanced.items) {
      expect(entry.visible(permissionsFor(`owner`), team)).toBe(true)
      expect(entry.visible(permissionsFor(`member`), team)).toBe(false)
    }
  })
})

// EXP-238: the Personal group merges account settings into the one settings
// surface. Always visible, and after every team group — the index redirect
// must keep landing on a team section, never a personal one (SLOP-5: only the
// owner-only Advanced group follows it).
describe(`SETTINGS_NAV Personal group`, () => {
  const team: SettingsNavContext = { isCloud: false }
  const personal = SETTINGS_NAV.find((group) => group.group === `Personal`)!

  // EXP-862: "API keys" became "Security" (keys + passkeys); /api-keys is a
  // redirect now, not a nav entry.
  it(`follows every team group with Account, Notifications, and Security`, () => {
    expect(SETTINGS_NAV[SETTINGS_NAV.length - 2]).toBe(personal)
    expect(
      SETTINGS_NAV.slice(0, SETTINGS_NAV.indexOf(personal)).map((g) => g.group)
    ).toEqual([`Team`, `Boards`, `Features`])
    expect(personal.items.map((item) => item.label)).toEqual([
      `Account`,
      `Notifications`,
      `Security`,
    ])
    expect(personal.items.map((item) => item.to)).toEqual([
      `/t/$teamSlug/settings/account`,
      `/t/$teamSlug/settings/notifications`,
      `/t/$teamSlug/settings/security`,
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

// SLOP-4: the helpdesk is gone — the Features group carries the ONE widget
// page (owner-only) and MCP servers (EXP-792: member-VISIBLE, because every
// member picks servers for a run; only the write controls inside the page
// are owner-only).
describe(`SETTINGS_NAV Features group (SLOP-4, EXP-792)`, () => {
  const team: SettingsNavContext = { isCloud: false }
  const features = SETTINGS_NAV.find((group) => group.group === `Features`)!

  it(`lists Widget and MCP servers`, () => {
    expect(features.items.map((item) => item.label)).toEqual([
      `Widget`,
      `MCP servers`,
    ])
    expect(features.items.map((item) => item.to)).toEqual([
      `/t/$teamSlug/settings/widget`,
      `/t/$teamSlug/settings/mcp-servers`,
    ])
  })

  it(`keeps the widget page owner-only`, () => {
    for (const item of features.items) {
      expect(item.visible(permissionsFor(`owner`), team)).toBe(true)
      expect(item.visible(permissionsFor(`member`), team)).toBe(
        item.label === `MCP servers`
      )
    }
  })
})

// EXP-862: the Boards group FLATTENS like the desktop nav — the per-board
// rows and "New board" are injected at render, so only Repositories is static
// (SLOP-5 moved the owner-gated "Archived boards" entry to the Advanced group).
describe(`SETTINGS_NAV Boards group (EXP-862)`, () => {
  const boardsGroup = SETTINGS_NAV.find(
    (group) => group.group === `Boards`
  )!

  it(`lists Repositories alone, and no Boards list page`, () => {
    expect(boardsGroup.items.map((item) => item.label)).toEqual([`Repositories`])
    expect(boardsGroup.items[0].to).toBe(`/t/$teamSlug/settings/repositories`)
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
