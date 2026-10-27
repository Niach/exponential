import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { CHATS_COPY } from "@exp/domain-contract/chats"
import { Button, SkeletonRows, conceptIcon } from "@exp/ui"
import { MyMachines } from "@/components/my-machines"
import { useSteerConfig } from "@/hooks/use-steer-config"
import { useSteerDevices } from "@/hooks/use-steer-devices"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useCrossTeamScope } from "@/hooks/use-cross-team-scope"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { MobilePageHeader } from "@/components/team/mobile-page-header"
import { pageTitle } from "@/lib/page-title"

// Team Devices view (EXP-686 — the old Agents route, minus the actions
// surface: Actions is its own route now): the caller's
// online desktops and servers. EXP-909 follow-up: a device row starts
// nothing — its one control is the settings gear, and the Agent page
// composer's device picker is the way to aim a run at a machine.
//
// EXP-909: ONE surface, not two. The cross-device "Accounts" section is gone
// and every machine lists the logins it holds under its own row — a credential
// lives on a machine and is repaired there, so the device row is the only
// place that can act on it.

// #EXP-1384 (chats.json `shell.header`, settings-nav.json): phones PUSH
// Devices from the Team page (its last settings row), so it wears THE page
// header with a back control to the Team page before the title; the tab bar
// stays, the Team tab lit.

const UiBackIcon = conceptIcon(`ui-back`)

export const Route = createFileRoute(`/t/$teamSlug/devices`)({
  head: () => ({ meta: [{ title: pageTitle(`Devices`) }] }),
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: DevicesPage,
})

function DevicesPage() {
  const { teamSlug } = Route.useParams()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()

  const currentUserId = session?.user?.id
  const teamId = team?.id
  // Steer tickets require team membership and a configured relay; the
  // server enforces both at mint time, this only decides whether the
  // interactive affordances render.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  // EXP-1186: the phone's Devices is not team-scoped — every synced
  // teammate server shared into ANY member team, listed once; md+ keeps
  // the active team's.
  const scope = useCrossTeamScope(team)
  const remote = useSteerDevices({
    enabled: steerEnabled,
    currentUserId,
    teamId,
    listTeamIds: scope.teamIds,
  })

  const header = (
    <MobilePageHeader
      teamSlug={teamSlug}
      title={CHATS_COPY.headers.devices}
      leading={
        <Button
          variant="ghost"
          size="icon"
          aria-label={CHATS_COPY.tabs.team}
          className="-ml-2 size-9 shrink-0 rounded-full text-muted-foreground hover:text-foreground"
          onClick={() => void navigate({ to: `/t/$teamSlug/team`, params: { teamSlug } })}
        >
          <UiBackIcon className="size-5" />
        </Button>
      }
      className="pb-3 pt-[max(0rem,calc(env(safe-area-inset-top)-1rem))] md:hidden"
    />
  )

  if (!team) {
    // D8 first load: the device row geometry (MyMachines' two-line ListRow,
    // 12px padding around a 20px name + 16px state line = 60px).
    return (
      <div className="mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl">
        {header}
        <SkeletonRows rowHeight={60} />
      </div>
    )
  }

  return (
    <>
      {/* EXP-771: the SCROLLER is full width, the reading column lives inside
          it — so the scrollbar rides the panel's right edge instead of
          appearing mid-page beside a centred list (the actions.tsx pattern). */}
      <div className="h-full overflow-y-auto">
        <div
          className={`mx-auto w-full max-w-3xl px-4 py-4 md:max-w-5xl ${TAB_BAR_CLEARANCE}`}
        >
          {header}
          {isMember && steerConfig?.enabled && (
            <MyMachines
              devices={remote.devices}
              onChanged={remote.refresh}
              latestVersions={remote.latestVersions}
              teamId={teamId}
              isTeamOwner={isOwner}
            />
          )}
        </div>
      </div>
    </>
  )
}
