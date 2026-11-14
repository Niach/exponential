import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { SidebarProvider, TooltipProvider } from "@exp/ui"
import type { AppListEntry } from "@/hooks/use-apps"

// #FEED-203 (fixtures/apps.json DESKTOP NAV): every app row carries the hover
// gear, for every member, to THE EDITOR's Settings tab (read-only below the
// creator or an owner). It used to show only to those two, so everyone else
// had no way in from the sidebar.

vi.mock(`@tanstack/react-router`, () => ({
  Link: ({
    to,
    params,
    search,
    children,
    ...rest
  }: {
    to: string
    params: { teamSlug: string; appSlug: string }
    search?: { tab?: string }
    children: React.ReactNode
  }) => (
    <a
      href={`${to.replace(`$teamSlug`, params.teamSlug).replace(`$appSlug`, params.appSlug)}${search?.tab ? `?tab=${search.tab}` : ``}`}
      {...rest}
    >
      {children}
    </a>
  ),
}))
vi.mock(`@/components/apps/apps-new-menu`, () => ({ AppsNewMenu: () => null }))
// #EXP-1832: no synced layout = the default order, every row at the top.
vi.mock(`@/hooks/use-sidebar-folders`, () => ({
  useSidebarFolders: (_teamId: string, _section: string, ids: readonly string[]) => ({
    rows: ids.map((id) => ({ kind: `item`, id, depth: 0, group: false, name: null, open: false, count: 0 })),
    editingId: null,
    setEditingId: () => {},
    move: () => {},
    toggle: () => {},
    rename: () => {},
    ungroup: () => {},
  }),
}))

import { SidebarAppsSection } from "@/components/team/sidebar-apps"

const app = (slug: string, name: string, createdBy: string | null) =>
  ({ id: slug, slug, name, createdBy, icon: null, color: null }) as unknown as AppListEntry

describe(`#FEED-203: the sidebar's app settings gear`, () => {
  it(`shows on every row, whoever made the app, and opens its Settings tab`, () => {
    render(
      <TooltipProvider>
        <SidebarProvider>
          <SidebarAppsSection
            teamId="t1"
            teamSlug="prep"
            apps={[app(`autotax`, `Autotax`, `someone-else`), app(`imported`, `Imported`, null)]}
            activeAppSlug={null}
          />
        </SidebarProvider>
      </TooltipProvider>
    )
    for (const [name, slug] of [
      [`Autotax`, `autotax`],
      [`Imported`, `imported`],
    ]) {
      const gear = screen.getByRole(`link`, { name: `Settings for ${name}` })
      expect(gear.getAttribute(`href`)).toBe(`/t/prep/app-editor/${slug}?tab=settings`)
      // A real link: Tab reaches it (the row reveals it on focus-visible).
      expect(gear.tabIndex).toBe(0)
    }
  })
})
