import { useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { actionCollection } from "@/lib/collections"
import {
  builtinCreateAction,
  builtinFixConflictsAction,
} from "@/lib/builtin-actions"
import {
  Bot,
  Github,
  LoaderCircle,
  Monitor,
  MonitorOff,
  MonitorUp,
  Ellipsis,
  Pencil,
  Play,
  Trash2,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { SectionLabel, SessionRow } from "@/components/agent-session-row"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import {
  ActionEditorDialog,
  type ActionRepoOption,
  type TeamAction,
} from "@/components/action-editor-dialog"
import {
  LaunchDialog,
  type LaunchTab,
} from "@/components/launch-dialog/launch-dialog"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useIsMobile } from "@/hooks/use-mobile"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamBySlug } from "@/hooks/use-team-data"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { TAB_BAR_CLEARANCE } from "@/components/team/mobile-tab-bar"
import { getActionIcon } from "@/lib/board-icons"

// Team Agents view (EXP-257 — absorbed the old Actions route): the caller's
// online desktops (remote-start entry point) plus the team's actions. On
// desktop viewports (md+) the actions render as a command-center card grid
// and there is NO Live section — the AgentDock bottom strip already shows
// every live session. On mobile (<md) the page mirrors the native apps: My
// desktops → Running → Actions rows. One page-level unified LaunchDialog
// serves both entry points (device "Start coding" → Issues tab, action "Run"
// → Actions tab). Action writes are owner-only; the interactive parts gate on
// membership + a configured relay — the server enforces all of it regardless.
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

// Owner-only ⋯ menu — hidden entirely on the builtin (server-shipped, not
// editable or deletable).
function ActionMenu({
  action,
  onEdit,
  onDelete,
}: {
  action: TeamAction
  onEdit: () => void
  onDelete: () => void
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          className="h-7 w-7 text-muted-foreground"
          aria-label={`Action menu for ${action.name}`}
        >
          <Ellipsis className="h-4 w-4" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onClick={onEdit}>
          <Pencil className="h-4 w-4" />
          Edit
        </DropdownMenuItem>
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          <Trash2 className="h-4 w-4" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

function RepoBadge({ repoName }: { repoName: string }) {
  return (
    <Badge
      variant="outline"
      className="shrink-0 gap-1 font-mono text-[0.625rem]"
    >
      <Github className="h-3 w-3" />
      {repoName}
    </Badge>
  )
}

// One action as a desktop-viewport card (EXP-257 command-center grid).
function ActionCard({
  action,
  repoName,
  isOwner,
  canRun,
  runBusy,
  onRun,
  onEdit,
  onDelete,
}: {
  action: TeamAction
  repoName: string | undefined
  isOwner: boolean
  canRun: boolean
  runBusy: boolean
  onRun: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  const CardIcon = getActionIcon(action)
  return (
    <div className="flex flex-col gap-2 rounded-lg border border-border p-3">
      <div className="flex min-w-0 items-center gap-2">
        <CardIcon className="size-4 shrink-0 text-muted-foreground" />
        <span className="min-w-0 flex-1 truncate text-sm font-medium">
          {action.name}
        </span>
        {isOwner && !action.builtin && (
          <ActionMenu action={action} onEdit={onEdit} onDelete={onDelete} />
        )}
      </div>
      {repoName && (
        <div className="flex">
          <RepoBadge repoName={repoName} />
        </div>
      )}
      {action.description && (
        <p className="line-clamp-2 text-xs text-muted-foreground">
          {action.description}
        </p>
      )}
      {canRun && (
        <div className="mt-auto pt-1">
          <Button
            variant="outline"
            size="sm"
            disabled={runBusy}
            onClick={onRun}
          >
            <Play />
            Run
          </Button>
        </div>
      )}
    </div>
  )
}

// One action as a mobile-viewport row (native-app parity).
function ActionRow({
  action,
  repoName,
  isOwner,
  canRun,
  runBusy,
  onRun,
  onEdit,
  onDelete,
}: {
  action: TeamAction
  repoName: string | undefined
  isOwner: boolean
  canRun: boolean
  runBusy: boolean
  onRun: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  const RowIcon = getActionIcon(action)
  return (
    <div className="flex items-center gap-2 border-b border-border/30 px-3 py-2">
      <RowIcon className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{action.name}</span>
          {repoName && <RepoBadge repoName={repoName} />}
        </div>
        {action.description && (
          <div className="truncate text-xs text-muted-foreground">
            {action.description}
          </div>
        )}
      </div>
      {canRun && (
        <Button variant="outline" size="sm" disabled={runBusy} onClick={onRun}>
          <Play />
          Run
        </Button>
      )}
      {isOwner && !action.builtin && (
        <ActionMenu action={action} onEdit={onEdit} onDelete={onDelete} />
      )}
    </div>
  )
}

function AgentsPage() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { running, isLoading } = useAgentsData(team?.id)
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const dock = useAgentDock()
  const isMobile = useIsMobile()

  const currentUserId = session?.user?.id
  const teamId = team?.id
  // Steer tickets require team membership and a configured relay; the
  // server enforces both at mint time, this only decides whether the
  // interactive affordances render.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)

  const remote = useRemoteStart({ enabled: steerEnabled, currentUserId })
  const runBusy = remote.starting || remote.sentTo !== null

  // Actions ride the Electric `actions` shape since EXP-268 (body excluded —
  // editors fetch it via tRPC on open), so a builtin "Create action" run's
  // MCP-authored action just appears; no refetch machinery.
  const { data: actionRows } = useLiveQuery(
    (query) =>
      teamId
        ? query
            .from({ a: actionCollection })
            .where(({ a }) => eq(a.teamId, teamId))
        : undefined,
    [teamId]
  )

  // Builtins pinned FIRST by their flag (neither is a DB row, so the shape
  // can't carry them); the synced rows re-apply the server's ordering
  // (sortOrder asc, then name — collections hydrate unordered).
  const sortedActions = useMemo<TeamAction[] | null>(() => {
    if (!teamId || !isMember || actionRows === undefined) return null
    const rows = [...actionRows]
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
      .map((row) => ({ ...row, builtin: false as const }))
    return [
      builtinCreateAction(teamId),
      builtinFixConflictsAction(teamId),
      ...rows,
    ]
  }, [teamId, isMember, actionRows])

  // Repo names for the badges + the editor's repository select.
  const [repos, setRepos] = useState<ActionRepoOption[]>([])
  useEffect(() => {
    if (!teamId || !isMember) return
    let active = true
    trpc.repositories.list
      .query({ teamId })
      .then(
        (rows) =>
          active &&
          setRepos(rows.map((r) => ({ id: r.id, fullName: r.fullName })))
      )
      .catch(() => {})
    return () => {
      active = false
    }
  }, [teamId, isMember])
  const repoNameById = useMemo(
    () => new Map(repos.map((repo) => [repo.id, repo.fullName])),
    [repos]
  )

  // The one page-level unified launch dialog — device rows open the Issues
  // tab pre-targeted, action Run opens the Actions tab pre-selected.
  const [launch, setLaunch] = useState<{
    tab: LaunchTab
    deviceId?: string
    actionId?: string
  } | null>(null)

  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<TeamAction | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<TeamAction | null>(null)
  const [deleting, setDeleting] = useState(false)

  const confirmDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      // Failures surface via the global mutation-error toast; the synced
      // collection drops the row.
      await trpc.actions.delete.mutate({ id: deleteTarget.id })
      setDeleteTarget(null)
    } catch {
      // Toast already shown; keep the confirm open for a retry.
    } finally {
      setDeleting(false)
    }
  }

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  const actionItemProps = (action: TeamAction) => ({
    action,
    repoName: action.repositoryId
      ? repoNameById.get(action.repositoryId)
      : undefined,
    isOwner,
    canRun: steerEnabled,
    runBusy,
    onRun: () => setLaunch({ tab: `actions`, actionId: action.id }),
    onEdit: () => {
      setEditing(action)
      setEditorOpen(true)
    },
    onDelete: () => setDeleteTarget(action),
  })

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4 md:max-w-5xl">
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
          <div className="mb-4">
            <SectionLabel
              label="My desktops"
              count={remote.devices?.length ?? 0}
            />
            {remote.devices === null ? (
              <div className="px-3 py-3 text-sm text-muted-foreground">
                Loading…
              </div>
            ) : remote.devices.length === 0 ? (
              <div className="flex items-center gap-2 px-3 py-3 text-xs text-muted-foreground">
                <MonitorOff className="size-3.5 shrink-0" />
                No desktop online. Open the Exponential desktop app to start
                coding.
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
                    disabled={runBusy}
                    onClick={() =>
                      setLaunch({ tab: `issues`, deviceId: device.deviceId })
                    }
                  >
                    <MonitorUp />
                    Start coding
                  </Button>
                </div>
              ))
            )}
            {remote.sentTo && (
              <div className="flex items-center gap-1.5 px-3 py-2 text-xs text-muted-foreground">
                <LoaderCircle className="size-3 animate-spin" />
                Start sent to {remote.sentTo}. Waiting for the desktop…
              </div>
            )}
          </div>
        )}

        {/* Mobile mirrors the native apps' Running section; on desktop the
            AgentDock bottom strip already surfaces every live session. */}
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
                  // EXP-312: live sessions are owner-only — Watch renders
                  // only on the caller's own rows.
                  canWatch={steerEnabled && row.session.userId === currentUserId}
                  teamSlug={teamSlug}
                  onOpen={() => dock?.openDock(row.session.id)}
                />
              ))}
            </div>
          ) : null)}

        {isMember && (
          <div className="mb-4">
            <SectionLabel
              label="Actions"
              count={sortedActions?.length ?? 0}
            />
            {sortedActions === null ? (
              <div className="px-3 py-3 text-sm text-muted-foreground">
                Loading…
              </div>
            ) : isMobile ? (
              sortedActions.map((action) => (
                <ActionRow key={action.id} {...actionItemProps(action)} />
              ))
            ) : (
              <div className="grid gap-3 pt-1 sm:grid-cols-2 xl:grid-cols-3">
                {sortedActions.map((action) => (
                  <ActionCard key={action.id} {...actionItemProps(action)} />
                ))}
              </div>
            )}
          </div>
        )}
      </div>

      <LaunchDialog
        open={launch !== null}
        onOpenChange={(next) => {
          if (!next) setLaunch(null)
        }}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={team.id}
        initialTab={launch?.tab}
        initialDeviceId={launch?.deviceId}
        initialActionId={launch?.actionId}
        onStartIssues={(device, options, issueIds) => {
          remote
            .startIssues(device, options, issueIds)
            .then(() => setLaunch(null))
            .catch(() => {})
        }}
        onRunAction={(device, action, options, inputs) => {
          remote
            .runAction(device, action, options, inputs)
            .then(() => setLaunch(null))
            .catch(() => {})
        }}
      />

      {editing && (
        <ActionEditorDialog
          open={editorOpen}
          onOpenChange={setEditorOpen}
          repos={repos}
          action={editing}
        />
      )}

      <Dialog
        open={deleteTarget !== null}
        onOpenChange={(next) => {
          if (!next && !deleting) setDeleteTarget(null)
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>Delete action</DialogTitle>
            <DialogDescription>
              {`Delete "${deleteTarget?.name ?? ``}"? Live runs keep going and keep their label; this cannot be undone.`}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => setDeleteTarget(null)}
              disabled={deleting}
            >
              Cancel
            </Button>
            <Button
              variant="destructive"
              onClick={() => void confirmDelete()}
              disabled={deleting}
            >
              {deleting ? <LoaderCircle className="animate-spin" /> : <Trash2 />}
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
