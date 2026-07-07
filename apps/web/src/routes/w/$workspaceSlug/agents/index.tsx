import { Fragment, useState } from "react"
import { createFileRoute, redirect, useNavigate } from "@tanstack/react-router"
import { Bot, MonitorPlay, X } from "lucide-react"
import type { AgentSessionRow } from "@/hooks/use-agents-data"
import { EmptyState } from "@/components/empty-state"
import { relativeTime } from "@/components/comment-rows/format"
import { SteerViewer, useSteerConfig } from "@/components/steer-terminal"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useSession } from "@/hooks/use-session"
import { useWorkspaceBySlug } from "@/hooks/use-workspace-data"
import { useWorkspacePermissions } from "@/hooks/use-workspace-permissions"
import { displayUserName } from "@/lib/user-display"
import { Button } from "@/components/ui/button"

// Workspace Agents view: every desktop coding session in the workspace,
// running first (live indicator + inline Watch/Steer via the steer relay),
// then the recently ended ones. The list is pure client work over the synced
// coding_sessions shape; watching reuses the SteerViewer transport from
// steer-terminal.tsx (ticket minted by trpc.steer.mintTicket — membership and
// perm are enforced server-side at mint time, the UI only mirrors them).
export const Route = createFileRoute(`/w/$workspaceSlug/agents/`)({
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

function SectionLabel({ label, count }: { label: string; count: number }) {
  return (
    <div
      className="flex items-center gap-1.5 rounded-t-md border-b border-border/50 px-3 py-1.5"
      style={{ backgroundColor: `rgba(113, 113, 122, 0.08)` }}
    >
      <span className="text-sm font-medium">{label}</span>
      <span className="text-xs text-muted-foreground">{count}</span>
    </div>
  )
}

function RunningIndicator() {
  return (
    <span className="relative flex size-2">
      <span className="absolute inline-flex h-full w-full animate-ping rounded-full bg-emerald-400 opacity-60" />
      <span className="relative inline-flex size-2 rounded-full bg-emerald-500" />
    </span>
  )
}

function SessionRow({
  row,
  watching,
  canWatch,
  onToggleWatch,
  onOpenIssue,
}: {
  row: AgentSessionRow
  watching: boolean
  /** Whether the Watch button shows at all (running + member + relay on). */
  canWatch: boolean
  onToggleWatch: () => void
  onOpenIssue: () => void
}) {
  const { session, issue, project, user } = row
  const isRunning = session.status === `running`

  return (
    <div
      className="group/row grid cursor-pointer grid-cols-[1.5rem_4.5rem_1fr_auto] items-center border-b border-border/30 px-3 py-2 hover:bg-muted/50"
      onClick={onOpenIssue}
      data-testid={`agent-session-${issue?.identifier ?? session.id}`}
    >
      <span className="flex items-center">
        {isRunning ? (
          <RunningIndicator />
        ) : (
          <span className="inline-flex size-2 rounded-full bg-muted-foreground/40" />
        )}
      </span>
      <span className="truncate font-mono text-xs text-muted-foreground">
        {issue?.identifier ?? `—`}
      </span>
      <div className="min-w-0 pr-2">
        <div className="truncate text-sm">{issue?.title ?? `Issue syncing…`}</div>
        <div className="flex min-w-0 items-center gap-1.5 text-xs text-muted-foreground">
          {project && (
            <span className="inline-flex min-w-0 items-center gap-1">
              <span
                className="h-2 w-2 shrink-0 rounded-full"
                style={{ backgroundColor: project.color }}
              />
              <span className="truncate">{project.name}</span>
              <span aria-hidden>·</span>
            </span>
          )}
          <span className="truncate">
            {displayUserName(user, session.userId)}
            {session.deviceLabel ? ` · ${session.deviceLabel}` : ``}
          </span>
          <span className="shrink-0 whitespace-nowrap">
            {isRunning
              ? `· started ${relativeTime(session.startedAt)}`
              : `· ended ${relativeTime(session.endedAt ?? session.startedAt)}`}
          </span>
        </div>
      </div>
      <div className="flex items-center gap-2">
        {isRunning && canWatch && (
          <Button
            variant="outline"
            size="sm"
            onClick={(e) => {
              e.stopPropagation()
              onToggleWatch()
            }}
          >
            {watching ? <X /> : <MonitorPlay />}
            {watching ? `Close` : `Watch`}
          </Button>
        )}
      </div>
    </div>
  )
}

function AgentsPage() {
  const { workspaceSlug } = Route.useParams()
  const navigate = useNavigate()
  const { data: session } = useSession()
  const workspace = useWorkspaceBySlug(workspaceSlug)
  const { running, ended, isLoading } = useAgentsData(workspace?.id)
  const { isMember } = useWorkspacePermissions(workspace)
  const steerConfig = useSteerConfig()

  // The running session whose steer viewer is expanded inline (one at a time —
  // each viewer holds a live relay socket).
  const [watchSessionId, setWatchSessionId] = useState<string | null>(null)

  const currentUserId = session?.user?.id
  // Steer tickets require workspace membership and a configured relay; the
  // server enforces both at mint time, this only decides whether the Watch
  // button renders (mirrors the SteerTerminal wrapper's gating).
  const canWatch = Boolean(
    currentUserId && isMember && steerConfig?.enabled
  )

  const openIssue = (row: AgentSessionRow) => {
    if (!row.issue || !row.project) return
    void navigate({
      to: `/w/$workspaceSlug/projects/$projectSlug/issues/$issueIdentifier`,
      params: {
        workspaceSlug,
        projectSlug: row.project.slug,
        issueIdentifier: row.issue.identifier,
      },
    })
  }

  if (!workspace) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  const isEmpty = running.length === 0 && ended.length === 0

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Bot className="h-4 w-4" />
          Agents
          {running.length > 0 && (
            <span className="text-sm font-normal text-muted-foreground">
              · {running.length} running
            </span>
          )}
        </h1>
      </div>

      <div className="flex-1 overflow-y-auto">
        {isLoading ? (
          <div className="text-muted-foreground p-6 text-sm">Loading…</div>
        ) : isEmpty ? (
          <EmptyState
            icon={Bot}
            title="No coding sessions yet"
            description="When someone starts coding an issue in the desktop app, the live session appears here to watch and steer."
          />
        ) : (
          <>
            {running.length > 0 && (
              <div className="mb-4">
                <SectionLabel label="Running" count={running.length} />
                {running.map((row) => (
                  <Fragment key={row.session.id}>
                    <SessionRow
                      row={row}
                      watching={watchSessionId === row.session.id}
                      canWatch={canWatch}
                      onToggleWatch={() =>
                        setWatchSessionId((prev) =>
                          prev === row.session.id ? null : row.session.id
                        )
                      }
                      onOpenIssue={() => openIssue(row)}
                    />
                    {watchSessionId === row.session.id &&
                      canWatch &&
                      currentUserId && (
                        <div className="border-b border-border/30 px-3 pb-3">
                          <SteerViewer
                            key={row.session.id}
                            session={row.session}
                            currentUserId={currentUserId}
                            autoConnect
                          />
                        </div>
                      )}
                  </Fragment>
                ))}
              </div>
            )}

            {ended.length > 0 && (
              <div className="mb-4">
                <SectionLabel label="Recently ended" count={ended.length} />
                {ended.map((row) => (
                  <SessionRow
                    key={row.session.id}
                    row={row}
                    watching={false}
                    canWatch={false}
                    onToggleWatch={() => {}}
                    onOpenIssue={() => openIssue(row)}
                  />
                ))}
              </div>
            )}
          </>
        )}
      </div>
    </div>
  )
}
