// EXP-456: the settings navigation as a sidebar panel. It occupies the same
// 16rem slot as the main team nav — TeamSidebar slides it in over the main
// panel while any /settings route is active — so the settings pages no longer
// carry their own desktop nav column (the in-page nav remains mobile-only).
import { useEffect, useState } from "react"
import { Link } from "@tanstack/react-router"
import { conceptIcon } from "@/lib/icons.generated"
import type { TeamPermissions } from "@/hooks/use-team-permissions"
import { getRuntimeConfigCached, type RuntimeConfig } from "@/lib/runtime-config"
import { SETTINGS_NAV } from "@/routes/t/$teamSlug/settings/-shared"
import { Separator } from "@/components/ui/separator"
import {
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar"

const UiBackIcon = conceptIcon(`ui-back`)

interface SettingsSidebarProps {
  teamSlug: string
  // Computed once by TeamSidebar — this panel is mounted on every team route
  // (off-screen until a settings route activates), so running its own
  // useTeamPermissions here would duplicate the billing fetch (EXP-457).
  permissions: TeamPermissions
  onBack: () => void
}

export function SettingsSidebar({
  teamSlug,
  permissions,
  onBack,
}: SettingsSidebarProps) {
  const [config, setConfig] = useState<RuntimeConfig | null>(null)
  useEffect(() => {
    void getRuntimeConfigCached().then(setConfig)
  }, [])
  const navContext = { isCloud: Boolean(config?.isCloud) }

  return (
    <>
      <SidebarHeader className="p-2">
        <SidebarMenu>
          <SidebarMenuItem>
            {/* h-10 matches the team-switcher row so the top edge doesn't
                jump mid-slide. The whole row is the back affordance. */}
            <SidebarMenuButton onClick={onBack} aria-label="Back" className="h-10">
              <UiBackIcon className="h-4 w-4" />
              <span className="text-sm font-semibold">Settings</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>

      <Separator />

      <SidebarContent>
        {SETTINGS_NAV.map((group) => {
          const items = group.items.filter((item) =>
            item.visible(permissions, navContext)
          )
          if (items.length === 0) return null
          return (
            <SidebarGroup key={group.group}>
              <SidebarGroupLabel>{group.group}</SidebarGroupLabel>
              <SidebarGroupContent>
                <SidebarMenu>
                  {items.map((item) => (
                    <SidebarMenuItem key={item.label}>
                      <SidebarMenuButton asChild>
                        <Link to={item.to} params={{ teamSlug }}>
                          <item.icon className="h-4 w-4" />
                          <span>{item.label}</span>
                        </Link>
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
          )
        })}
      </SidebarContent>
    </>
  )
}
