// Shared model for the team-settings pages (EXP-146): the grouped nav
// definition consumed by the layout route (sidebar) and the index route
// (redirect to the first visible section), plus the per-section access guard
// and the data hook every section page needs.
import { useEffect, useState } from "react"
import {
  CreditCard,
  FolderKanban,
  Github,
  MessageSquarePlus,
  Settings2,
  Tags,
  Users,
  type LucideIcon,
} from "lucide-react"
import { useSession } from "@/hooks/use-session"
import {
  useShowWorkspaceChrome,
  useWorkspaceBySlug,
  useWorkspaceUsers,
} from "@/hooks/use-workspace-data"
import {
  useWorkspacePermissions,
  type WorkspacePermissions,
} from "@/hooks/use-workspace-permissions"
import { getRuntimeConfig, type RuntimeConfig } from "@/lib/runtime-config"

export interface SettingsNavContext {
  isCloud: boolean
  solo: boolean
}

// Kept to the settings sub-route literals (not the all-routes union) so
// `<Link to={item.to} params={{ workspaceSlug }}>` type-checks.
export type SettingsSectionPath =
  | `/t/$workspaceSlug/settings/general`
  | `/t/$workspaceSlug/settings/members`
  | `/t/$workspaceSlug/settings/labels`
  | `/t/$workspaceSlug/settings/billing`
  | `/t/$workspaceSlug/settings/projects`
  | `/t/$workspaceSlug/settings/repositories`
  | `/t/$workspaceSlug/settings/widget`

export interface SettingsNavItem {
  label: string
  to: SettingsSectionPath
  icon: LucideIcon
  visible: (
    permissions: WorkspacePermissions,
    context: SettingsNavContext
  ) => boolean
}

// Grouped Linear-style — General first (team name on top). Gating mirrors the
// pre-split page exactly; General additionally hides in solo mode because the
// section renders nothing there (general-section.tsx).
export const SETTINGS_NAV: { group: string; items: SettingsNavItem[] }[] = [
  {
    group: `Team`,
    items: [
      {
        label: `General`,
        to: `/t/$workspaceSlug/settings/general`,
        icon: Settings2,
        visible: (permissions, context) =>
          permissions.canManageWorkspace && !context.solo,
      },
      {
        label: `Members`,
        to: `/t/$workspaceSlug/settings/members`,
        icon: Users,
        visible: () => true,
      },
      {
        label: `Labels`,
        to: `/t/$workspaceSlug/settings/labels`,
        icon: Tags,
        visible: () => true,
      },
      {
        label: `Plan & Billing`,
        to: `/t/$workspaceSlug/settings/billing`,
        icon: CreditCard,
        visible: (permissions, context) =>
          permissions.canManageWorkspace && context.isCloud,
      },
    ],
  },
  {
    group: `Projects`,
    items: [
      {
        label: `Projects`,
        to: `/t/$workspaceSlug/settings/projects`,
        icon: FolderKanban,
        visible: (permissions) => permissions.isOwner,
      },
      {
        label: `Repositories`,
        to: `/t/$workspaceSlug/settings/repositories`,
        icon: Github,
        visible: (permissions) => permissions.canManageRepos,
      },
    ],
  },
  {
    group: `Features`,
    items: [
      {
        label: `Feedback widget`,
        to: `/t/$workspaceSlug/settings/widget`,
        icon: MessageSquarePlus,
        visible: (permissions) => permissions.canManageWidgets,
      },
    ],
  },
]

// Everything a settings section page needs. `resolved` flips true only once
// the current user's own member row has synced — permissions are transiently
// all-false while shapes load, so guards must treat !resolved as loading,
// never as denied.
export function useSettingsPage(workspaceSlug: string) {
  const { data: session } = useSession()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { members, userMap } = useWorkspaceUsers(workspace?.id)
  const permissions = useWorkspacePermissions(workspace)
  const showChrome = useShowWorkspaceChrome(workspace?.id, session?.user?.id)
  const solo = !showChrome
  const [config, setConfig] = useState<RuntimeConfig | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then(setConfig)
  }, [])

  const currentUserId = session?.user?.id
  const resolved = Boolean(
    workspace &&
      currentUserId &&
      members.some((member) => member.userId === currentUserId)
  )

  return {
    session,
    workspace,
    members,
    userMap,
    permissions,
    solo,
    config,
    resolved,
  }
}

// Access guard for owner-gated section pages reached by direct URL. A notice
// beats a redirect: no bounce loops, and transient loading renders nothing.
export function SettingsSectionGuard({
  resolved,
  allowed,
  children,
}: {
  resolved: boolean
  allowed: boolean
  children: React.ReactNode
}) {
  if (!resolved) return null
  if (!allowed) {
    return (
      <p className="text-sm text-muted-foreground">
        Only the team owner can manage this section.
      </p>
    )
  }
  return <>{children}</>
}
