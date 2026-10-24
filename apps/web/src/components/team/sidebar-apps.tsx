import type * as React from "react"
import { useState } from "react"
import { Link } from "@tanstack/react-router"
import { APPS_COPY } from "@exp/domain-contract/apps"
import {
  conceptIcon,
  getActionIcon,
  SidebarGroup,
  SidebarGroupAction,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@exp/ui"
import { AppFormDialog } from "@/components/apps/app-form-dialog"
import type { AppListEntry } from "@/hooks/use-apps"
import { cn } from "@/lib/utils"

// VAPP-110 (fixtures/apps.json): the rail's APPS section, drawn exactly like
// Agents and Boards — the label, a "+" that opens the New app form, one row
// per app (its glyph in its colour + its name) opening the editor, and the hover gear (the creator or an owner)
// to the app's settings page. Desktop widths only: phones keep the Apps
// list route.

const UiAddIcon = conceptIcon(`ui-add`)
const NavSettingsIcon = conceptIcon(`nav-settings`)

/** An app's glyph: the curated set boards and agents share. */
export function AppIcon({ app, className }: { app: Pick<AppListEntry, `icon` | `color`>; className?: string }) {
  const Icon = getActionIcon({ icon: app.icon })
  return <Icon className={cn(`size-4 shrink-0`, className)} style={{ color: app.color ?? undefined }} />
}

export function SidebarAppsSection({
  teamId,
  teamSlug,
  apps,
  me,
  isOwner,
  activeAppSlug,
}: {
  teamId: string
  teamSlug: string
  apps: readonly AppListEntry[]
  me: string | null
  isOwner: boolean
  activeAppSlug: string | null
}) {
  const [creating, setCreating] = useState(false)
  const canManage = (app: AppListEntry) => isOwner || app.createdBy === me
  return (
    <SidebarGroup data-testid="sidebar-apps">
      <SidebarGroupLabel>{APPS_COPY.title}</SidebarGroupLabel>
      <SidebarGroupAction title={APPS_COPY.newApp} aria-label={APPS_COPY.newApp} onClick={() => setCreating(true)}>
        <UiAddIcon className="h-4 w-4" />
      </SidebarGroupAction>
      <SidebarGroupContent>
        <SidebarMenu>
          {apps.map((app) => (
            <SidebarMenuItem key={app.id}>
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
              {canManage(app) && (
                <SidebarMenuAction asChild showOnHover title="App settings">
                  <Link
                    to="/t/$teamSlug/settings/apps/$appId"
                    params={{ teamSlug, appId: app.id }}
                    aria-label={`Settings for ${app.name}`}
                  >
                    <NavSettingsIcon />
                  </Link>
                </SidebarMenuAction>
              )}
            </SidebarMenuItem>
          ))}
        </SidebarMenu>
      </SidebarGroupContent>
      <AppFormDialog open={creating} onOpenChange={setCreating} teamId={teamId} teamSlug={teamSlug} />
    </SidebarGroup>
  )
}

/** The compact rail's app icons, after the agent icons. */
export function SidebarAppsIcons({
  teamSlug,
  apps,
  renderItem,
  separator,
}: {
  teamSlug: string
  apps: readonly AppListEntry[]
  renderItem: (item: {
    key: string
    label: string
    link: { to: string; params: Record<string, string> }
    icon: React.ReactNode
  }) => React.ReactNode
  separator?: React.ReactNode
}) {
  if (apps.length === 0) return null
  return (
    <>
      {separator}
      {apps.map((app) =>
        renderItem({
          key: app.id,
          label: app.name,
          link: { to: `/t/$teamSlug/apps/$appSlug`, params: { teamSlug, appSlug: app.slug } },
          icon: <AppIcon app={app} />,
        })
      )}
    </>
  )
}
