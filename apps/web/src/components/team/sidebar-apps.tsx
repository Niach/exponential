import { useMemo } from "react"
import { Link } from "@tanstack/react-router"
import { APPS_COPY, APPS_TEST_IDS } from "@exp/domain-contract/apps"
import {
  conceptIcon,
  getAppIcon,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@exp/ui"
import { AppsNewMenu } from "@/components/apps/apps-new-menu"
import { SidebarFolderList } from "@/components/team/sidebar-folders"
import type { AppListEntry } from "@/hooks/use-apps"
import { cn } from "@/lib/utils"

// VAPP-110 (fixtures/apps.json DESKTOP NAV): the rail's APPS section, drawn exactly like
// Agents and Boards — the label (VAPP-242: not clickable, there is no Apps page), a "+" =
// THE NEW MENU (New app / Import…), one row per app (its glyph in its colour + its name)
// RUNNING it (THE RUN VIEW), and the hover gear (#FEED-203: every member) to THE EDITOR's
// Settings tab, read-only below the creator or an owner.
// Desktop widths only: phones keep the Apps list tab.

const UiAddIcon = conceptIcon(`ui-add`)
const NavSettingsIcon = conceptIcon(`nav-settings`)

/** An app's glyph: the curated set boards and agents share. */
export function AppIcon({ app, className }: { app: Pick<AppListEntry, `icon` | `color`>; className?: string }) {
  const Icon = getAppIcon({ icon: app.icon })
  return <Icon className={cn(`size-4 shrink-0`, className)} style={{ color: app.color ?? undefined }} />
}

export function SidebarAppsSection({
  teamId,
  teamSlug,
  apps,
  activeAppSlug,
}: {
  teamId: string
  teamSlug: string
  apps: readonly AppListEntry[]
  activeAppSlug: string | null
}) {
  // #EXP-1832: the default order is the list's (by name); the member's
  // folders and drag order sit on top (sidebar-folders.json).
  const ids = useMemo(() => apps.map((app) => app.id), [apps])
  const byId = useMemo(() => new Map(apps.map((app) => [app.id, app])), [apps])
  return (
    <SidebarGroup data-testid="sidebar-apps">
      <SidebarGroupLabel>{APPS_COPY.title}</SidebarGroupLabel>
      <AppsNewMenu
        teamId={teamId}
        teamSlug={teamSlug}
        align="start"
        trigger={
          <SidebarGroupAction title={APPS_COPY.newApp} aria-label={APPS_COPY.newApp} data-testid={APPS_TEST_IDS.newApp}>
            <UiAddIcon className="h-4 w-4" />
          </SidebarGroupAction>
        }
      />
      <SidebarGroupContent>
        <SidebarFolderList
          teamId={teamId}
          section="app"
          ids={ids}
          renderItem={(id, itemProps, guides) => {
            const app = byId.get(id)
            if (!app) return null
            return (
              <SidebarMenuItem key={app.id} {...itemProps}>
                {guides}
                <SidebarMenuButton asChild density="compact" isActive={activeAppSlug === app.slug}>
                  <Link
                    to="/t/$teamSlug/apps/$appSlug"
                    params={{ teamSlug, appSlug: app.slug }}
                    data-testid="apps-row"
                  >
                    <AppIcon app={app} />
                    <span className="min-w-0 flex-1 truncate">{app.name}</span>
                  </Link>
                </SidebarMenuButton>
                <SidebarMenuAction asChild showOnHover title="App settings">
                  <Link
                    to="/t/$teamSlug/app-editor/$appSlug"
                    params={{ teamSlug, appSlug: app.slug }}
                    search={{ tab: `settings` }}
                    aria-label={`Settings for ${app.name}`}
                  >
                    <NavSettingsIcon />
                  </Link>
                </SidebarMenuAction>
              </SidebarMenuItem>
            )
          }}
        />
      </SidebarGroupContent>
    </SidebarGroup>
  )
}

