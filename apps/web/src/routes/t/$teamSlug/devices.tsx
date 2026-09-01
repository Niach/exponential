import { useEffect, useState } from "react"
import {
  createFileRoute,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { MyMachines } from "@/components/my-machines"
import { SessionRow } from "@/components/agent-session-row"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// Team Devices view (EXP-686 — the old Agents route, minus the actions
// surface: Actions and Automations are their own routes now): the caller's
// online desktops and servers, the remote-start entry point, and the native
// apps' Running section below them on every viewport (EXP-697). The
// LaunchDialog here serves the device rows' "Start coding".
//
// EXP-631: `?chat=1` is the mobile FAB's one-shot open — the tab bar owns the
// button, this route owns the launcher, so the request rides the URL (the
// board route's `?new=1` compose pattern).
type DevicesSearch = { chat?: 1 }

export const Route = createFileRoute(`/t/$teamSlug/devices`)({
  validateSearch: (search: Record<string, unknown>): DevicesSearch => ({
    chat: search.chat === 1 || search.chat === `1` ? 1 : undefined,
  }),
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
  const search = Route.useSearch()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const dock = useAgentDock()

  const currentUserId = session?.user?.id
  const teamId = team?.id
  // Own sessions only (EXP-312 follow-up): a teammate's live session can
  // never be watched from here, so listing it only read as "not online".
  const { running, isLoading } = useAgentsData(teamId, currentUserId)
  // Steer tickets require team membership and a configured relay; the
  // server enforces both at mint time, this only decides whether the
  // interactive affordances render.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  const remote = useRemoteStart({
    enabled: steerEnabled,
    currentUserId,
    teamId,
  })
  const runBusy = remote.starting || remote.sentTo !== null

  // The device rows' "Start coding" dialog, opened on the Issues tab
  // pre-targeted at the picked machine.
  const [launchDeviceId, setLaunchDeviceId] = useState<string | null>(null)
  // The mobile FAB's Chat launcher — the SAME dialog, opened on its Chat tab
  // with no device preference.
  const [chatOpen, setChatOpen] = useState(false)

  // Consume `?chat=1` once, then drop the key so a back/refresh doesn't
  // re-open the dialog.
  useEffect(() => {
    if (search.chat !== 1) return
    setChatOpen(true)
    void navigate({
      to: `/t/$teamSlug/devices`,
      params: { teamSlug },
      search: {},
      replace: true,
    })
  }, [search.chat, navigate, teamSlug])

  const closeLaunch = () => {
    setLaunchDeviceId(null)
    setChatOpen(false)
  }

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4 md:max-w-5xl">
      <div className={`flex-1 overflow-y-auto ${TAB_BAR_CLEARANCE}`}>
        {isMember && steerConfig?.enabled && (
          <MyMachines
            devices={remote.devices}
            runBusy={runBusy}
            sentTo={remote.sentTo}
            onStartCoding={(deviceId) => setLaunchDeviceId(deviceId)}
            onChanged={remote.refresh}
            latestVersions={remote.latestVersions}
            teamId={teamId}
          />
        )}

        {/* The native apps' Running section, on every viewport (EXP-697 —
            it used to be mobile-only because the AgentDock strip covers
            desktop, but the machines page lists sessions everywhere now). */}
        {isLoading ? (
          <div className="text-muted-foreground p-6 text-sm">Loading…</div>
        ) : (
          <div className="mb-6">
            <GlassSectionHeader label="Running" />
            {running.length > 0 ? (
              <div className="flex flex-col gap-2">
                {running.map((row) => (
                  <SessionRow
                    key={row.session.id}
                    row={row}
                    teamSlug={teamSlug}
                    isOwner={isOwner}
                    currentUserId={currentUserId}
                    steerEnabled={steerEnabled}
                    onOpen={() => dock?.openDock(row.session.id)}
                  />
                ))}
              </div>
            ) : (
              <GlassRow className="text-sm text-muted-foreground">
                No agents running right now.
              </GlassRow>
            )}
          </div>
        )}
      </div>

      <LaunchDialog
        open={launchDeviceId !== null || chatOpen}
        onOpenChange={(next) => {
          if (!next) {
            setLaunchDeviceId(null)
            setChatOpen(false)
          }
        }}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={team.id}
        initialTab={chatOpen ? `chat` : `issues`}
        initialDeviceId={launchDeviceId ?? undefined}
        onStartIssues={(device, options, issueIds) => {
          remote
            .startIssues(device, options, issueIds)
            .then(() => closeLaunch())
            .catch(() => {})
        }}
        onRunAction={(device, action, options, inputs) => {
          remote
            .runAction(device, action, options, inputs)
            .then(() => closeLaunch())
            .catch(() => {})
        }}
      />
    </div>
  )
}
