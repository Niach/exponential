import { createFileRoute, Navigate, useNavigate } from "@tanstack/react-router"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { MobileDetailHeader, useIsMobile } from "@exp/ui"
import { MyMachines } from "@/components/my-machines"
import { useMemberTeams } from "@/components/home/use-member-teams"
import { PushedTitle } from "@/components/team/mobile-page-header"
import { PAGE_END_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { useSteerConfig } from "@/hooks/use-steer-config"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { pageTitle } from "@/lib/page-title"

// #EXP-2109 (settings-nav.json `nav.phone`): the phones' Devices list, pushed
// from Settings: the caller's machines and every teammate's server shared into
// a member team. md+ has a sidebar row per device (#EXP-2027).
// #EXP-1534 (inbox-route.json): `?device=<devices row id>` opens that device's
// settings (an `agent_signed_out` inbox row or push).
export const Route = createFileRoute(`/_app/devices/`)({
  head: () => ({ meta: [{ title: pageTitle(`Devices`) }] }),
  validateSearch: (search: Record<string, unknown>): { device?: string } =>
    typeof search.device === `string` ? { device: search.device } : {},
  component: DevicesRoute,
})

function DevicesRoute() {
  const { device } = Route.useSearch()
  if (useIsMobile()) return <PhoneDevices device={device} />
  return device ? (
    <Navigate to="/devices/$deviceId/settings" params={{ deviceId: device }} replace />
  ) : (
    <Navigate to="/home" replace />
  )
}

function PhoneDevices({ device }: { device: string | undefined }) {
  const { user, defaultTeamSlug } = Route.useRouteContext()
  const navigate = useNavigate()
  const team = useTeamBySlug(defaultTeamSlug)
  const { isOwner } = useTeamPermissions(team)
  const { teamIds } = useMemberTeams(user.id)
  const steerConfig = useSteerConfig()
  const enabled = Boolean(steerConfig?.enabled)
  const remote = useSteerDevices({ enabled, currentUserId: user.id, teamId: team?.id, listTeamIds: teamIds })
  return (
    <div className="flex h-full min-h-0 flex-col pt-[env(safe-area-inset-top)]">
      <MobileDetailHeader
        title={<PushedTitle>{CHATS_COPY.headers.devices}</PushedTitle>}
        onBack={() => void navigate({ to: `/settings` })}
        backLabel={CHATS_COPY.headers.settings}
      />
      <div className={`min-h-0 flex-1 overflow-y-auto px-4 pt-6 ${PAGE_END_CLEARANCE}`}>
        {enabled && (
          <MyMachines
            devices={remote.devices}
            onChanged={remote.refresh}
            latestVersions={remote.latestVersions}
            teamId={team?.id}
            isTeamOwner={isOwner}
            openSettingsRowId={device}
            onSettingsClosed={() => void navigate({ to: `/devices`, search: {}, replace: true })}
          />
        )}
      </div>
    </div>
  )
}
