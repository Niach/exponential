import { useEffect, useState } from "react"
import {
  createFileRoute,
  redirect,
  useNavigate,
} from "@tanstack/react-router"
import { MyMachines } from "@/components/my-machines"
import {
  EndedSessionRow,
  SessionRow,
} from "@/components/agent-session-row"
import { agentLabel } from "@/components/agent-usage-bar"
import { relativeTime } from "@/components/comment-rows/format"
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { pastRunByline, pastRunEndedAt } from "@/lib/past-runs"
import {
  useAgentsData,
  usePastRuns,
  type PastRunRow,
} from "@/hooks/use-agents-data"
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

/** EXP-746: the Past row's caption. The ORDER and the "ended by" wording are
 * the ×4 rule (lib/past-runs.ts); the agent label and the relative time are
 * this client's own vocabulary and formatter. */
function rowByline(row: PastRunRow): string {
  // A row that stamped neither end nor heartbeat has no honest time to show
  // (0 would render as 1970), so that segment simply drops.
  const endedAt = pastRunEndedAt(row.session)
  return pastRunByline(row.session, {
    deviceLabel: row.device.label ?? row.session.deviceLabel,
    agentLabel: row.session.agent ? agentLabel(row.session.agent) : null,
    relativeTime: endedAt > 0 ? relativeTime(new Date(endedAt)) : ``,
  })
}

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
                  row={{ session: row.session, canResume: row.canResume }}
                  title={row.title}
                  identifier={row.identifier ?? undefined}
                  byline={rowByline(row)}
                />
              ))}
            </div>
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
