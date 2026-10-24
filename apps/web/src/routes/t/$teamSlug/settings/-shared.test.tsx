import { existsSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"
import {
  SETTINGS_NAV_PHONE_ROWS,
  SETTINGS_NAV_ROWS,
} from "@exp/domain-contract/settings-nav"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import {
  flattenSettingsNav,
  NEW_AGENT_LABEL,
  SETTINGS_AGENTS_GROUP,
  SETTINGS_NAV,
  settingsRouteOf,
  settingsSectionLabel,
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

  // EXP-862: "API keys" became "Security" (keys + passkeys).
  it(`follows every team group with Account, Notifications, and Security`, () => {
    expect(SETTINGS_NAV[SETTINGS_NAV.length - 2]).toBe(personal)
    expect(
      SETTINGS_NAV.slice(0, SETTINGS_NAV.indexOf(personal)).map((g) => g.group)
    ).toEqual([`Team`, `Boards`, `Agents`, `Apps`, `Features`])
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
// rows and "New board" are injected at render, so nothing is static (SLOP-5
// moved "Archived boards" to Advanced, EXP-1293 Repositories to Agents).
describe(`SETTINGS_NAV Boards group (EXP-862)`, () => {
  const boardsGroup = SETTINGS_NAV.find(
    (group) => group.group === `Boards`
  )!

  it(`has no static entries, and no Boards list page`, () => {
    expect(boardsGroup.items).toEqual([])
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

// EXP-1293: team agents are managed like boards — the Agents group follows
// Boards; one row per agent plus the owner-only "New agent" are injected at
// render off the synced sessions shape, then Repositories closes the group.
describe(`SETTINGS_NAV Agents group (EXP-1293)`, () => {
  const agentsGroup = SETTINGS_NAV.find(
    (group) => group.group === SETTINGS_AGENTS_GROUP
  )!

  it(`sits right after Boards with Repositories and Prompts as its static entries`, () => {
    const names = SETTINGS_NAV.map((group) => group.group)
    expect(names.indexOf(`Agents`)).toBe(names.indexOf(`Boards`) + 1)
    expect(agentsGroup.items.map((item) => item.label)).toEqual([`Repositories`, `Prompts`])
    expect(agentsGroup.items[0].to).toBe(`/t/$teamSlug/settings/repositories`)
    expect(agentsGroup.items[1].to).toBe(`/t/$teamSlug/settings/prompts`)
  })

  // NEW-2: every member reads the team's prompts; writes are owner-only
  // inside the section.
  it(`shows Prompts to owners and plain members alike`, () => {
    const team: SettingsNavContext = { isCloud: false }
    const prompts = agentsGroup.items.find((item) => item.label === `Prompts`)!
    expect(prompts.visible(permissionsFor(`owner`), team)).toBe(true)
    expect(prompts.visible(permissionsFor(`member`), team)).toBe(true)
  })

  it(`labels the create entry with the fixture copy`, () => {
    expect(NEW_AGENT_LABEL).toBe(`New agent`)
  })
})

// #EXP-1381: settings-nav.json (the phones' Team-page rows) is DERIVED from
// SETTINGS_NAV, so the two cannot drift: every row is a SETTINGS_NAV page
// with the same label, or one of the injected lists (Boards, Agents: their
// SETTINGS_NAV group) or the Devices page; and every row resolves to a real
// route file, so no phone row opens a 404.
describe(`settings-nav.json rows vs SETTINGS_NAV`, () => {
  const here = dirname(fileURLToPath(import.meta.url))
  const routeExists = (segments: string) =>
    [`${segments}.tsx`, join(segments, `index.tsx`)].some((file) => existsSync(join(here, file)))

  it.each(SETTINGS_NAV_ROWS.map((row) => [row.id, row] as const))(`%s maps to a page`, (id, row) => {
    const item = items.find((entry) => entry.to === `/t/$teamSlug/settings/${id}`)
    if (id === `devices`) {
      expect(existsSync(join(here, `..`, `devices.tsx`))).toBe(true)
      return
    }
    if (id === `boards` || id === `agents`) {
      expect(SETTINGS_NAV.some((group) => group.group === row.label)).toBe(true)
    } else {
      expect(item?.label).toBe(row.label)
      expect(item?.icon).toBeDefined()
    }
    expect(routeExists(id)).toBe(true)
    expect(settingsSectionLabel(id)).toBe(row.label)
  })

  it(`lists every SETTINGS_NAV team/agents/features page, in order`, () => {
    const derived = SETTINGS_NAV.filter((group) =>
      [`Team`, `Boards`, `Agents`, `Features`].includes(group.group)
    ).flatMap((group) => [
      ...(group.items.length === 0 || group.group === `Agents` ? [group.group.toLowerCase()] : []),
      ...flattenSettingsNav(group.items).map((entry) => entry.to.split(`/settings/`)[1]),
    ])
    expect(SETTINGS_NAV_ROWS.map((row) => row.id).filter((id) => id !== `devices`)).toEqual(derived)
  })

  it(`phoneRows are rows flagged phone, in order`, () => {
    expect(SETTINGS_NAV_PHONE_ROWS).toEqual(SETTINGS_NAV_ROWS.filter((row) => row.phone).map((row) => row.id))
  })

  it(`titles a phone settings page from its path`, () => {
    expect(settingsRouteOf(`/t/acme/settings/boards/b1`)).toEqual({ section: `boards`, sub: `b1` })
    expect(settingsRouteOf(`/t/acme/settings/labels`)).toEqual({ section: `labels`, sub: null })
    expect(settingsSectionLabel(`boards`, `archived`)).toBe(`Archived boards`)
    expect(settingsSectionLabel(`account`)).toBe(`Account`)
    expect(settingsSectionLabel(`apps`)).toBe(`Apps`)
  })
})
