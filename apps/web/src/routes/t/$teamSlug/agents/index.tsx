import { useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { MyMachines } from "@/components/my-machines"
import { SectionLabel, SessionRow } from "@/components/agent-session-row"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { TeamActionsPanel } from "@/components/team-actions-panel"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useIsMobile } from "@/hooks/use-mobile"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// Team Agents view (EXP-257 — absorbed the old Actions route): the caller's
// online desktops (remote-start entry point) plus the team's actions. On
// desktop viewports (md+) the actions render inline as the tabbed command
// center (`TeamActionsPanel`) and there is NO Live section — the AgentDock
// bottom strip already shows every live session. On mobile (<md) the page
// mirrors the native apps (EXP-574): My desktops → Running only, with the
// Actions surface behind the topbar's own "Actions" entry as a separate page
// (`/t/$teamSlug/agents/actions`). The LaunchDialog here serves the device
// rows' "Start coding"; the panel owns its own for action runs.
export const Route = createFileRoute(`/t/$teamSlug/agents/`)({
  beforeLoad: async ({ context, location }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: location.href },
      })
    }
  },
  component: AgentsPage,
})

function AgentsPage() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const dock = useAgentDock()
  const isMobile = useIsMobile()

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

        {/* Mobile mirrors the native apps' Running section; on desktop the
            AgentDock bottom strip already surfaces the caller's live
            sessions. */}
        {isMobile &&
          (isLoading ? (
            <div className="text-muted-foreground p-6 text-sm">Loading…</div>
          ) : running.length > 0 ? (
            <div className="mb-4">
              <SectionLabel label="Running" count={running.length} />
              {running.map((row) => (
                <SessionRow
                  key={row.session.id}
                  row={row}
                  // EXP-312: live sessions are owner-only, and every row in
                  // this list is already the caller's own.
                  canWatch={steerEnabled}
                  teamSlug={teamSlug}
                  onOpen={() => dock?.openDock(row.session.id)}
                />
              ))}
            </div>
          ) : null)}

        {/* EXP-574: on mobile the actions surface is its own page behind the
            topbar's "Actions" entry — native-app parity. */}
        {!isMobile && <TeamActionsPanel team={team} />}
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
            .then(() => setLaunchDeviceId(null))
            .catch(() => {})
        }}
        onRunAction={(device, action, options, inputs) => {
          remote
            .runAction(device, action, options, inputs)
            .then(() => setLaunchDeviceId(null))
            .catch(() => {})
        }}
      />
    </div>
  )
}
