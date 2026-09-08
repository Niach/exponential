import { useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { MyMachines } from "@/components/my-machines"
import {
  EndedSessionRow,
  pastRunRowByline,
  SessionRow,
} from "@/components/agent-session-row"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { useSteerConfig } from "@/components/agent-session"
import { useOpenSession } from "@/hooks/use-open-session"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { useAgentsData, usePastRuns } from "@/hooks/use-agents-data"
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
// EXP-739: the mobile chat FAB is a LINK to `/t/$teamSlug/chat` now — the
// launcher's Chat tab keeps working from the dialog, but this route no longer
// carries a `?chat=1` one-shot.

export const Route = createFileRoute(`/t/$teamSlug/devices`)({
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
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const openSession = useOpenSession()

  const currentUserId = session?.user?.id
  const teamId = team?.id
  // Own sessions only (EXP-312 follow-up): a teammate's live session can
  // never be watched from here, so listing it only read as "not online".
  const { running, isLoading } = useAgentsData(teamId, currentUserId)
  // EXP-746: the caller's own FINISHED, person-started runs — the native
  // apps' Past section, on every viewport like Running above it.
  const { past } = usePastRuns(teamId, currentUserId)
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

  const closeLaunch = () => setLaunchDeviceId(null)

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
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
              it used to be mobile-only because the dock strip covers desktop,
              but the machines page lists sessions everywhere now). A row opens
              the run's own session page (EXP-740). */}
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
                      onOpen={() => openSession(row.session)}
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

          {/* EXP-746: Past — the caller's own finished runs, newest first,
              capped at PAST_RUN_CAP. Hidden entirely when empty: a header over
              nothing is noise, and the natives do the same. */}
          {past.length > 0 && (
            <div className="mb-6">
              <GlassSectionHeader label="Past" />
              <div className="flex flex-col gap-2">
                {past.map((row) => (
                  <EndedSessionRow
                    key={row.session.id}
                    row={{ session: row.session }}
                    title={row.title}
                    identifier={row.identifier ?? undefined}
                    byline={pastRunRowByline(row)}
                  />
                ))}
              </div>
            </div>
          )}
        </div>
      </div>

      <LaunchDialog
        open={launchDeviceId !== null}
        onOpenChange={(next) => {
          if (!next) setLaunchDeviceId(null)
        }}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={team.id}
        initialTab="issues"
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
    </>
  )
}
