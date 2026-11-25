import type { ReactNode } from "react"
import { Link } from "@tanstack/react-router"
import { conceptIcon, getAppIcon, SidebarMenuAction, SidebarMenuButton, SidebarMenuItem, type MenuTargetProps } from "@exp/ui"
import { useSidebarTreeDensity, type SidebarRowItemProps } from "@/components/team/sidebar-folders"
import type { AppListEntry } from "@/hooks/use-apps"
import { cn } from "@/lib/utils"

// VAPP-110 (fixtures/apps.json DESKTOP NAV) / #EXP-2027: an APP's row, drawn like the
// agents and boards of its team's list — its glyph in its colour + its name RUNNING it
// (THE RUN VIEW), and the hover gear (#FEED-203: every member) to THE EDITOR's Settings
// tab, read-only below the creator or an owner. Desktop widths only: phones keep the Apps
// list tab.

const NavSettingsIcon = conceptIcon(`nav-settings`)

/** An app's glyph: the curated set boards and agents share. */
export function AppIcon({ app, className }: { app: Pick<AppListEntry, `icon` | `color`>; className?: string }) {
  const Icon = getAppIcon({ icon: app.icon })
  return <Icon className={cn(`size-4 shrink-0`, className)} style={{ color: app.color ?? undefined }} />
}

/** #EXP-2027: an app's row in its team's list (sidebar-tree.json
 *  `teamItems`): its glyph + name RUNNING it, the hover gear (#FEED-203:
 *  every member) to THE EDITOR's Settings tab. */
export function SidebarAppRow({
  app,
  teamSlug,
  active,
  itemProps,
  guides,
  menu,
}: {
  /** #EXP-2124: the row's context-menu target (`menuProps`). */
  menu?: MenuTargetProps
  app: AppListEntry
  teamSlug: string
  active: boolean
  itemProps: SidebarRowItemProps
  guides: ReactNode
}) {
  const density = useSidebarTreeDensity()
  return (
    <SidebarMenuItem key={app.id} {...itemProps}>
      {guides}
      <SidebarMenuButton asChild density={density} isActive={active}>
        <Link to="/t/$teamSlug/apps/$appSlug" params={{ teamSlug, appSlug: app.slug }} data-testid="apps-row" {...menu}>
          <AppIcon app={app} />
          <span className="min-w-0 flex-1 truncate">{app.name}</span>
        </Link>
      </SidebarMenuButton>
      {density === `compact` && <SidebarMenuAction asChild showOnHover title="App settings">
        <Link
          to="/t/$teamSlug/app-editor/$appSlug"
          params={{ teamSlug, appSlug: app.slug }}
          search={{ tab: `settings` }}
          aria-label={`Settings for ${app.name}`}
        >
          <NavSettingsIcon />
        </Link>
      </SidebarMenuAction>}
    </SidebarMenuItem>
  )
}
