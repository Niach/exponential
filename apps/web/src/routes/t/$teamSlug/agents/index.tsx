import { useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { Bot, Loader2, Monitor, MonitorOff, MonitorUp } from "lucide-react"
import { EmptyState } from "@/components/empty-state"
import { SectionLabel, SessionRow } from "@/components/agent-session-row"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import { StartCodingDialog } from "@/components/start-coding-dialog"
import { useRemoteCodingStart } from "@/hooks/use-remote-coding-start"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { Button } from "@/components/ui/button"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"

// Team Agents view: the caller's online desktops (remote-start entry
// point) plus every RUNNING coding session in the team. Rows focus the
// global dock (components/agent-dock) — the live viewer lives there alone, one
// at a time — instead of expanding inline. Membership + a configured relay gate
// the interactive parts; the server enforces both regardless.
export const Route = createFileRoute(`/t/$teamSlug/agents/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: AgentsPage,
})

// The caller's online desktops + a page-level Start-coding dialog (the multi-
// issue picker). Rendered only for members on a relay-enabled instance.
function MyDesktops({ teamId }: { teamId: string }) {
  const remote = useRemoteCodingStart()
  const [dialogOpen, setDialogOpen] = useState(false)
  const [seedDeviceId, setSeedDeviceId] = useState<string | undefined>(undefined)

  const openFor = (deviceId: string) => {
    setSeedDeviceId(deviceId)
    setDialogOpen(true)
  }

  const busy = remote.starting || remote.sentTo !== null

  return (
    <div className="mb-4">
      <SectionLabel label="My desktops" count={remote.devices?.length ?? 0} />
      {remote.devices === null ? (
        <div className="px-3 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : remote.devices.length === 0 ? (
        <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
          <MonitorOff className="size-3.5 shrink-0" />
          No desktop online — open the Exponential desktop app to start coding.
        </div>
      ) : (
        remote.devices.map((device) => (
          <div
            key={device.deviceId}
            className="flex items-center gap-2 border-b border-border/30 px-3 py-2"
          >
            <Monitor className="size-4 shrink-0 text-muted-foreground" />
            <span className="flex-1 truncate text-sm">
              {device.deviceLabel || device.deviceId}
            </span>
            <Button
              variant="outline"
              size="sm"
              disabled={busy}
              onClick={() => openFor(device.deviceId)}
            >
              <MonitorUp />
              Start coding
            </Button>
          </div>
        ))
      )}
      {remote.sentTo && (
        <div className="flex items-center gap-1.5 px-3 py-2 text-xs text-muted-foreground">
          <Loader2 className="size-3 animate-spin" />
          Start sent to {remote.sentTo} — waiting for the desktop…
        </div>
      )}
      <StartCodingDialog
        open={dialogOpen}
        onOpenChange={setDialogOpen}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={teamId}
        initialDeviceId={seedDeviceId}
        onStart={(device, options, issueIds) => {
          remote
            .start(device, options, issueIds)
            .then(() => setDialogOpen(false))
            .catch(() => {})
        }}
      />
    </div>
  )
}

function AgentsPage() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { running, isLoading } = useAgentsData(team?.id)
  const { isMember } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const dock = useAgentDock()

  const currentUserId = session?.user?.id
  // Steer tickets require team membership and a configured relay; the
  // server enforces both at mint time, this only decides whether the Watch
  // affordance renders.
  const canWatch = Boolean(currentUserId && isMember && steerConfig?.enabled)

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Bot className="h-4 w-4" />
          Agents
          {running.length > 0 && (
            <span className="text-sm font-normal text-muted-foreground">
              · {running.length} live
            </span>
          )}
        </h1>
      </div>

      <div className={`flex-1 overflow-y-auto ${TAB_BAR_CLEARANCE}`}>
        {isMember && steerConfig?.enabled && (
          <MyDesktops teamId={team.id} />
        )}

        {isLoading ? (
          <div className="text-muted-foreground p-6 text-sm">Loading…</div>
        ) : running.length === 0 ? (
          <EmptyState
            icon={Bot}
            title="No coding sessions yet"
            description="When you or a teammate starts coding, live sessions appear here."
          />
        ) : (
          <div className="mb-4">
            <SectionLabel label="Live" count={running.length} />
            {running.map((row) => (
              <SessionRow
                key={row.session.id}
                row={row}
                canWatch={canWatch}
                teamSlug={teamSlug}
                onOpen={() => dock?.openDock(row.session.id)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  )
}
