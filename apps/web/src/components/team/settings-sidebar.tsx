// EXP-456: the settings navigation as a sidebar panel. It REPLACES the main menu
// in the sidebar column while any settings route is active (desktop parity:
// the IDE's `LeftOccupant::Settings`), so the settings pages carry no nav
// column of their own. #EXP-2027 (settings-nav.json `desktopNavs`): md+ has
// no current team, so the panel is ONE of two navs, read off the URL:
// `/settings/*` = Personal (back + "Settings"), `/t/<slug>/settings/*` = that
// team's pages (back + the team's mark and name). Phones never draw it.
import { useEffect, useRef, useState } from "react"
import { Link, useRouterState } from "@tanstack/react-router"
import { useTeamPermissions, type TeamPermissions } from "@/hooks/use-team-permissions"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { getRuntimeConfigCached, type RuntimeConfig } from "@/lib/runtime-config"
import { isPersonalSettingsPath, teamSettingsSlug } from "@/lib/app-paths"
import {
  conceptIcon,
  Separator,
  SidebarContent,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarMenuSub,
  SidebarMenuSubButton,
  SidebarMenuSubItem,
  TeamAvatar,
} from "@exp/ui"
import {
  desktopSettingsNav,
  type SettingsNavContext,
  type SettingsNavItem,
} from "@/routes/t/$teamSlug/settings/-shared"
import { SidebarBackRow } from "@/components/team/sidebar-back-row"
import { AssistantMark } from "@/components/team/assistant-mark"
import { useEnsureAssistant } from "@/hooks/use-ensure-assistant"

const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)
const UiBackIcon = conceptIcon(`ui-back`)

/** Which nav the panel draws: personal, or one team's. */
export type SettingsSidebarScope = { kind: `personal` } | { kind: `team`; teamSlug: string }

/** The scope a pathname opens; null = not a settings page. */
export function settingsSidebarScope(pathname: string): SettingsSidebarScope | null {
  if (isPersonalSettingsPath(pathname)) return { kind: `personal` }
  const teamSlug = teamSettingsSlug(pathname)
  return teamSlug ? { kind: `team`, teamSlug } : null
}

// The trailing path of a nav entry (`/settings/labels`) — what the current
// location ends with while that page is open.
function sectionPath(item: SettingsNavItem): string {
  return item.to.replace(`/t/$teamSlug`, ``)
}

// EXP-630: an entry with sub-pages (Issues → Labels, Statuses). The row
// itself navigates to the parent page; the chevron folds the sub-pages,
// which start unfolded while the parent or one of them is open.
function NavItemWithChildren({
  item,
  teamSlug,
  permissions,
  navContext,
}: {
  item: SettingsNavItem
  teamSlug: string
  permissions: TeamPermissions
  navContext: SettingsNavContext
}) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  const children = (item.children ?? []).filter((child) =>
    child.visible(permissions, navContext)
  )
  const within = [item, ...children].some((entry) =>
    pathname.endsWith(sectionPath(entry))
  )
  const [toggled, setToggled] = useState<boolean | null>(null)
  const open = toggled ?? within
  const Chevron = open ? ChevronDownIcon : ChevronRightIcon
  return (
    <SidebarMenuItem>
      <SidebarMenuButton asChild>
        <Link to={item.to} params={{ teamSlug }}>
          <item.icon className="h-4 w-4" />
          <span>{item.label}</span>
        </Link>
      </SidebarMenuButton>
      {children.length > 0 && (
        <SidebarMenuAction
          aria-label={open ? `Collapse ${item.label}` : `Expand ${item.label}`}
          aria-expanded={open}
          onClick={() => setToggled(!open)}
        >
          <Chevron />
        </SidebarMenuAction>
      )}
      {open && children.length > 0 && (
        <SidebarMenuSub>
          {children.map((child) => (
            <SidebarMenuSubItem key={child.id}>
              <SidebarMenuSubButton asChild>
                <Link to={child.to} params={{ teamSlug }}>
                  <child.icon className="h-4 w-4" />
                  <span>{child.label}</span>
                </Link>
              </SidebarMenuSubButton>
            </SidebarMenuSubItem>
          ))}
        </SidebarMenuSub>
      )}
    </SidebarMenuItem>
  )
}

export function SettingsSidebar({ onBack }: { onBack: () => void }) {
  const pathname = useRouterState({ select: (state) => state.location.pathname })
  // The panel stays mounted (off-screen) once settings close: it keeps the
  // last nav it drew so it slides out unchanged.
  const current = settingsSidebarScope(pathname)
  const last = useRef<SettingsSidebarScope>(current ?? { kind: `personal` })
  if (current) last.current = current
  const scope = current ?? last.current

  const [config, setConfig] = useState<RuntimeConfig | null>(null)
  useEffect(() => {
    void getRuntimeConfigCached().then(setConfig)
  }, [])
  const navContext: SettingsNavContext = { isCloud: Boolean(config?.isCloud) }

  return scope.kind === `team` ? (
    <TeamSettingsNav teamSlug={scope.teamSlug} navContext={navContext} onBack={onBack} />
  ) : (
    <PersonalSettingsNav open={current?.kind === `personal`} navContext={navContext} onBack={onBack} />
  )
}

function PersonalSettingsNav({
  open,
  navContext,
  onBack,
}: {
  open: boolean
  navContext: SettingsNavContext
  onBack: () => void
}) {
  // #EXP-1987: the Assistant is created on first use while Settings is open.
  useEnsureAssistant(open ? null : undefined)
  // Personal pages are the same for every viewer (no owner-only page).
  const nav = desktopSettingsNav({ isOwner: false, isCloud: navContext.isCloud }).personal
  return (
    <>
      <SidebarBackRow label="Settings" onBack={onBack} />
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>{nav.label}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {nav.items.map((item) => (
                <SidebarMenuItem key={item.id}>
                  <SidebarMenuButton asChild>
                    <Link to={item.to} data-testid={item.id === `assistant` ? `settings-assistant` : undefined}>
                      {item.id === `assistant` ? <AssistantMark /> : <item.icon className="h-4 w-4" />}
                      <span>{item.label}</span>
                    </Link>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </>
  )
}

function TeamSettingsNav({
  teamSlug,
  navContext,
  onBack,
}: {
  teamSlug: string
  navContext: SettingsNavContext
  onBack: () => void
}) {
  const team = useTeamBySlug(teamSlug)
  const permissions = useTeamPermissions(team)
  const nav = desktopSettingsNav({ isOwner: permissions.isOwner, isCloud: navContext.isCloud }).team
  const name = team?.name ?? ``
  return (
    <>
      {/* The team's own header: back + its mark + its name (SidebarBackRow's
          shape, the team mark beside the label). */}
      <SidebarHeader className="p-2">
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton onClick={onBack} aria-label={`Back from ${name}`} className="h-10">
              <UiBackIcon className="h-4 w-4" />
              <TeamAvatar iconUrl={team?.iconUrl} size={16} />
              <span className="min-w-0 truncate text-sm font-semibold">{name}</span>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <Separator />

      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>{nav.label}</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {nav.items.map((item) =>
                item.children ? (
                  <NavItemWithChildren
                    key={item.id}
                    item={item}
                    teamSlug={teamSlug}
                    permissions={permissions}
                    navContext={navContext}
                  />
                ) : (
                  <SidebarMenuItem key={item.id}>
                    <SidebarMenuButton asChild>
                      <Link to={item.to} params={{ teamSlug }}>
                        <item.icon className="h-4 w-4" />
                        <span>{item.label}</span>
                      </Link>
                    </SidebarMenuButton>
                  </SidebarMenuItem>
                )
              )}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
    </>
  )
}
