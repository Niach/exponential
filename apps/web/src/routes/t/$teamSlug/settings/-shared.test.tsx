import { existsSync } from "node:fs"
import { dirname, join } from "node:path"
import { fileURLToPath } from "node:url"
import { describe, expect, it } from "vitest"
import {
  SETTINGS_NAV_GROUPS,
  SETTINGS_NAV_PHONE_ROWS,
  SETTINGS_NAV_ROWS,
  settingsNavPage,
} from "@exp/domain-contract/settings-nav"
import settingsNavFixture from "@exp/domain-contract/fixtures/settings-nav.json"
import { conceptIcon } from "@exp/ui"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import {
  ASSISTANT_LABEL,
  flattenSettingsNav,
  NEW_AGENT_LABEL,
  SETTINGS_NAV,
  settingsNavListVisible,
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
const SETTINGS_NAV_CASES = settingsNavFixture.nav.navCases
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

// S-3 (settings-nav.json `nav`): the web nav IS the fixture's groups for
// platform `web`, in order, every page with the fixture's label + icon; the
// Features and Advanced groups and Personal > Voice are gone.
describe(`SETTINGS_NAV follows settings-nav.json nav (S-3)`, () => {
  it(`has the fixture's web groups in order`, () => {
    expect(SETTINGS_NAV.map((group) => [group.id, group.group])).toEqual(
      SETTINGS_NAV_GROUPS.filter((group) => !group.platforms || group.platforms.includes(`web`)).map(
        (group) => [group.id, group.label]
      )
    )
    expect(SETTINGS_NAV.map((group) => group.group)).toEqual([`Team`, `Boards`, `Agents`, `Apps`, `Personal`])
  })

  it(`orders every group's entries like the fixture, injected lists in place`, () => {
    const entries = Object.fromEntries(
      SETTINGS_NAV.map((group) => [
        group.id,
        group.entries.map((entry) => (entry.kind === `list` ? entry.list : entry.item.id)),
      ])
    )
    expect(entries).toEqual({
      team: [`general`, `members`, `issues`, `repositories`, `prompts`, `widget`, `storage`, `billing`, `import`],
      boards: [`@boards`, `@new-board`, `archived-boards`],
      agents: [`@assistant`, `@agents`, `@new-agent`, `mcp-servers`],
      apps: [`@apps`, `@new-app`],
      personal: [`account`, `notifications`, `security`],
    })
  })

  it(`labels and draws every page like the fixture`, () => {
    for (const item of items) {
      const page = settingsNavPage(item.id)!
      expect(item.label, item.id).toBe(page.label)
      expect(item.icon, item.id).toBe(conceptIcon(page.icon as Parameters<typeof conceptIcon>[0]))
    }
  })

  it.each(
    SETTINGS_NAV_CASES.filter((navCase) => navCase.platform === `web`).map((navCase) => [navCase.name, navCase] as const)
  )(`replays navCase %s`, (_name, navCase) => {
    const permissions = permissionsFor(navCase.role as `owner` | `member`)
    const context: SettingsNavContext = { isCloud: navCase.cloud }
    const visible = Object.fromEntries(
      SETTINGS_NAV.map((group) => [
        group.id,
        group.entries.flatMap((entry) =>
          entry.kind === `list`
            ? settingsNavListVisible(entry.list, permissions)
              ? [entry.list]
              : []
            : entry.item.visible(permissions, context)
              ? [entry.item.id]
              : []
        ),
      ] as const).filter(([, ids]) => ids.length > 0)
    )
    expect(visible).toEqual(navCase.expected)
  })

  it(`keeps the owner-only pages and New entries from members`, () => {
    const team: SettingsNavContext = { isCloud: true }
    for (const id of [`general`, `widget`, `storage`, `import`, `archived-boards`, `billing`]) {
      const item = items.find((entry) => entry.id === id)!
      expect(item.visible(permissionsFor(`owner`), team), id).toBe(true)
      expect(item.visible(permissionsFor(`member`), team), id).toBe(false)
    }
    for (const list of [`@new-board`, `@new-agent`, `@new-app`]) {
      expect(settingsNavListVisible(list, permissionsFor(`member`)), list).toBe(false)
      expect(settingsNavListVisible(list, permissionsFor(`owner`)), list).toBe(true)
    }
    expect(settingsNavListVisible(`@assistant`, permissionsFor(`member`))).toBe(true)
  })

  it(`has no Voice page and titles the Assistant page`, () => {
    expect(items.some((item) => item.label === `Voice`)).toBe(false)
    expect(settingsSectionLabel(`assistant`)).toBe(ASSISTANT_LABEL)
    expect(ASSISTANT_LABEL).toBe(`Assistant`)
    expect(NEW_AGENT_LABEL).toBe(`New agent`)
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

  it(`every row's page is in the web nav (or an injected list's group)`, () => {
    for (const row of SETTINGS_NAV_ROWS) {
      if (row.id === `devices` || row.id === `boards` || row.id === `agents`) continue
      expect(items.some((item) => item.id === row.id), row.id).toBe(true)
    }
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
