// Shared model for the team-settings pages (EXP-146): the grouped nav
// definition consumed by the layout route (sidebar) and the index route
// (redirect to the first visible section), plus the per-section access guard
// and the data hook every section page needs.
import { useEffect, useState } from "react"
import { type LucideIcon } from "lucide-react"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug, useTeamUsers } from "@/hooks/use-team-data"
import {
  useTeamPermissions,
  type TeamPermissions,
} from "@/hooks/use-team-permissions"
import { getRuntimeConfig, type RuntimeConfig } from "@/lib/runtime-config"
import { conceptIcon } from "@/lib/icons.generated"

export interface SettingsNavContext {
  isCloud: boolean
}

// Kept to the settings sub-route literals (not the all-routes union) so
// `<Link to={item.to} params={{ teamSlug }}>` type-checks.
export type SettingsSectionPath =
  | `/t/$teamSlug/settings/general`
  | `/t/$teamSlug/settings/members`
  | `/t/$teamSlug/settings/labels`
  | `/t/$teamSlug/settings/statuses`
  | `/t/$teamSlug/settings/billing`
  | `/t/$teamSlug/settings/storage`
  | `/t/$teamSlug/settings/boards`
  | `/t/$teamSlug/settings/repositories`
  | `/t/$teamSlug/settings/widget`
  | `/t/$teamSlug/settings/account`
  | `/t/$teamSlug/settings/notifications`
  | `/t/$teamSlug/settings/api-keys`

export interface SettingsNavItem {
  label: string
  to: SettingsSectionPath
  icon: LucideIcon
  visible: (
    permissions: TeamPermissions,
    context: SettingsNavContext
  ) => boolean
}

// Grouped Linear-style — General first (team name on top). Gating mirrors the
// pre-split page exactly. General's Danger Zone is the sole UI path to
// deleting a team, which owners may do even for their last one (EXP-188).
//
// EXP-317: every icon comes from the shared registry (`settings-*` concepts),
// so this nav and the desktop IDE's (`settings/mod.rs::section_icon`) draw the
// same glyph per section — locked by lib/icons.test.ts.
export const SETTINGS_NAV: { group: string; items: SettingsNavItem[] }[] = [
  {
    group: `Team`,
    items: [
      {
        label: `General`,
        to: `/t/$teamSlug/settings/general`,
        icon: conceptIcon(`settings-general`),
        visible: (permissions) => permissions.canManageTeam,
      },
      {
        label: `Members`,
        to: `/t/$teamSlug/settings/members`,
        icon: conceptIcon(`settings-members`),
        visible: () => true,
      },
      {
        label: `Labels`,
        to: `/t/$teamSlug/settings/labels`,
        icon: conceptIcon(`settings-labels`),
        visible: () => true,
      },
      // EXP-314 custom issue statuses. Member-editable like Labels (the
      // router gates writes at `mutate_resources`), so visible to everyone.
      {
        label: `Statuses`,
        to: `/t/$teamSlug/settings/statuses`,
        icon: conceptIcon(`settings-statuses`),
        visible: () => true,
      },
      {
        label: `Plan & Billing`,
        to: `/t/$teamSlug/settings/billing`,
        icon: conceptIcon(`settings-billing`),
        visible: (permissions, context) =>
          permissions.canManageTeam && context.isCloud,
      },
      // EXP-297 attachment manager: usage meter + per-file delete + the
      // unreferenced-image sweep. Owner-only, like the router behind it.
      {
        label: `Storage`,
        to: `/t/$teamSlug/settings/storage`,
        icon: conceptIcon(`settings-storage`),
        visible: (permissions) => permissions.isOwner,
      },
    ],
  },
  {
    group: `Boards`,
    items: [
      {
        label: `Boards`,
        to: `/t/$teamSlug/settings/boards`,
        icon: conceptIcon(`settings-boards`),
        visible: (permissions) => permissions.isOwner,
      },
      // EXP-557 per-user sharing: every member manages their own GitHub
      // connection and shares repos here, so the section is member-visible.
      {
        label: `Repositories`,
        to: `/t/$teamSlug/settings/repositories`,
        icon: conceptIcon(`settings-repositories`),
        visible: () => true,
      },
    ],
  },
  {
    group: `Features`,
    items: [
      {
        label: `Feedback widget`,
        to: `/t/$teamSlug/settings/widget`,
        icon: conceptIcon(`settings-widget`),
        visible: (permissions) => permissions.canManageWidgets,
      },
    ],
  },
  // EXP-238: account settings merged into the one settings surface (the
  // desktop IDE's Personal group). Last on purpose — the index redirect
  // lands on the first visible item, which must stay a team section.
  {
    group: `Personal`,
    items: [
      {
        label: `Account`,
        to: `/t/$teamSlug/settings/account`,
        icon: conceptIcon(`settings-account`),
        visible: () => true,
      },
      {
        label: `Notifications`,
        to: `/t/$teamSlug/settings/notifications`,
        icon: conceptIcon(`settings-notifications`),
        visible: () => true,
      },
      {
        label: `API keys`,
        to: `/t/$teamSlug/settings/api-keys`,
        icon: conceptIcon(`settings-api`),
        visible: () => true,
      },
    ],
  },
]

// Everything a settings section page needs. `resolved` flips true only once
// the current user's own member row has synced — permissions are transiently
// all-false while shapes load, so guards must treat !resolved as loading,
// never as denied.
export function useSettingsPage(teamSlug: string) {
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { members, userMap } = useTeamUsers(team?.id)
  const permissions = useTeamPermissions(team)
  const [config, setConfig] = useState<RuntimeConfig | null>(null)

  useEffect(() => {
    void getRuntimeConfig().then(setConfig)
  }, [])

  const currentUserId = session?.user?.id
  const resolved = Boolean(
    team &&
    currentUserId &&
    members.some((member) => member.userId === currentUserId)
  )

  return {
    session,
    team,
    members,
    userMap,
    permissions,
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
