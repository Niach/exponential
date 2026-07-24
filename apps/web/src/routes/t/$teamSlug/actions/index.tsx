import { useCallback, useEffect, useMemo, useState } from "react"
import { createFileRoute, redirect } from "@tanstack/react-router"
import {
  Github,
  Loader2,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Trash2,
  Zap,
} from "lucide-react"
import { trpc } from "@/lib/trpc-client"
import { EmptyState } from "@/components/empty-state"
import { SectionLabel, SessionRow } from "@/components/agent-session-row"
import { useSteerConfig } from "@/components/agent-session"
import { useAgentDock } from "@/components/agent-dock/agent-dock-provider"
import {
  ActionEditorDialog,
  type ActionRepoOption,
  type TeamAction,
} from "@/components/action-editor-dialog"
import { RunActionDialog } from "@/components/run-action-dialog"
import { useAgentsData } from "@/hooks/use-agents-data"
import { useRunAction } from "@/hooks/use-run-action"
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

// Team Actions view (EXP-253): reusable agent prompts (tRPC-only, not an
// Electric shape) any member can run on one of their actions-capable
// desktops, plus the LIVE action runs in the team (the actionName-labeled
// slice of the synced coding_sessions rows). Writes are owner-only; the
// interactive parts additionally gate on membership + a configured relay —
// the server enforces all of it regardless.
export const Route = createFileRoute(`/t/$teamSlug/actions/`)({
  beforeLoad: async ({ context }) => {
    if (!context.session) {
      throw redirect({
        to: `/auth/login`,
        search: { redirect: undefined },
      })
    }
  },
  component: ActionsPage,
})

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
  return (
    <div className="flex items-center gap-2 border-b border-border/30 px-3 py-2">
      <Zap className="size-4 shrink-0 text-muted-foreground" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{action.name}</span>
          {repoName && (
            <Badge
              variant="outline"
              className="shrink-0 gap-1 font-mono text-[0.625rem]"
            >
              <Github className="h-3 w-3" />
              {repoName}
            </Badge>
          )}
        </div>
        {action.description && (
          <div className="truncate text-xs text-muted-foreground">
            {action.description}
          </div>
        )}
      </div>
      {canRun && (
        <Button
          variant="outline"
          size="sm"
          disabled={runBusy}
          onClick={onRun}
        >
          <Play />
          Run
        </Button>
      )}
      {isOwner && (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-7 w-7 text-muted-foreground"
              aria-label={`Action menu for ${action.name}`}
            >
              <MoreHorizontal className="h-4 w-4" />
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
      )}
    </div>
  )
}

function ActionsPage() {
  const { teamSlug } = Route.useParams()
  const { data: session } = useSession()
  const team = useTeamBySlug(teamSlug)
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const dock = useAgentDock()
  const { running } = useAgentsData(team?.id)

  const currentUserId = session?.user?.id
  const teamId = team?.id

  // tRPC-fetched state (no react-query in this codebase — the team-settings
  // sections' useState/useEffect + refetch-callback convention).
  const [actions, setActions] = useState<TeamAction[] | null>(null)
  const [listError, setListError] = useState<string | null>(null)
  const refetch = useCallback(async () => {
    if (!teamId) return
    try {
      const res = await trpc.actions.list.query({ teamId })
      setActions(res.actions)
      setListError(null)
    } catch (err) {
      setListError(err instanceof Error ? err.message : String(err))
    }
  }, [teamId])

  useEffect(() => {
    setActions(null)
    void refetch()
  }, [refetch])

  // Repo names for the row badges + the editor's repository select.
  const [repos, setRepos] = useState<ActionRepoOption[]>([])
  useEffect(() => {
    if (!teamId) return
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
  }, [teamId])
  const repoNameById = useMemo(
    () => new Map(repos.map((repo) => [repo.id, repo.fullName])),
    [repos]
  )

  // Runs need membership + a configured relay (also gates the presence
  // fetch inside the hook); watching live rows needs the same.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  const runAction = useRunAction({ enabled: steerEnabled, currentUserId })
  const canWatch = Boolean(currentUserId && steerEnabled)
  const runBusy = runAction.starting || runAction.sentTo !== null

  // The actionName-labeled slice of the team's live sessions — a deleted
  // action's runs keep their name snapshot, so they stay visible here.
  const actionRuns = useMemo(
    () => running.filter((row) => row.session.actionName != null),
    [running]
  )

  const [editorOpen, setEditorOpen] = useState(false)
  const [editing, setEditing] = useState<TeamAction | null>(null)
  const [runTarget, setRunTarget] = useState<TeamAction | null>(null)
  const [deleteTarget, setDeleteTarget] = useState<TeamAction | null>(null)
  const [deleting, setDeleting] = useState(false)

  const openCreate = () => {
    setEditing(null)
    setEditorOpen(true)
  }

  const confirmDelete = async () => {
    if (!deleteTarget) return
    setDeleting(true)
    try {
      // Failures surface via the global mutation-error toast.
      await trpc.actions.delete.mutate({ id: deleteTarget.id })
      setDeleteTarget(null)
      await refetch()
    } catch {
      // Toast already shown; keep the confirm open for a retry.
    } finally {
      setDeleting(false)
    }
  }

  if (!team) {
    return <div className="text-muted-foreground text-sm p-6">Loading…</div>
  }

  return (
    <div className="mx-auto flex h-full w-full max-w-3xl flex-col px-4 py-4">
      <div className="mb-3 flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-lg font-semibold">
          <Zap className="h-4 w-4" />
          Actions
          {actionRuns.length > 0 && (
            <span className="text-sm font-normal text-muted-foreground">
              · {actionRuns.length} live
            </span>
          )}
        </h1>
        {isOwner && (
          <Button size="sm" onClick={openCreate}>
            <Plus />
            New action
          </Button>
        )}
      </div>
      <p className="mb-3 text-sm text-muted-foreground">
        Reusable agent prompts anyone in the team can run on their desktop —
        review sweeps, triage passes, changelog drafts.
      </p>

      <div className={`flex-1 overflow-y-auto ${TAB_BAR_CLEARANCE}`}>
        {actionRuns.length > 0 && (
          <div className="mb-4">
            <SectionLabel label="Live runs" count={actionRuns.length} />
            {actionRuns.map((row) => (
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

        {listError && (
          <div className="mb-3 rounded-md border border-destructive/50 bg-destructive/10 px-3 py-2 text-sm text-destructive">
            {listError}
          </div>
        )}

        {actions === null ? (
          !listError && (
            <div className="text-muted-foreground p-6 text-sm">Loading…</div>
          )
        ) : actions.length === 0 ? (
          <EmptyState
            icon={Zap}
            title="No actions yet"
            description={
              isOwner
                ? `Create a reusable prompt your team can run on any desktop — start from a template.`
                : `No actions exist in this team yet — a team owner can create one.`
            }
          >
            {isOwner && (
              <Button size="sm" onClick={openCreate}>
                <Plus />
                New action
              </Button>
            )}
          </EmptyState>
        ) : (
          <div className="mb-4">
            <SectionLabel label="Actions" count={actions.length} />
            {actions.map((action) => (
              <ActionRow
                key={action.id}
                action={action}
                repoName={
                  action.repositoryId
                    ? repoNameById.get(action.repositoryId)
                    : undefined
                }
                isOwner={isOwner}
                canRun={steerEnabled}
                runBusy={runBusy}
                onRun={() => setRunTarget(action)}
                onEdit={() => {
                  setEditing(action)
                  setEditorOpen(true)
                }}
                onDelete={() => setDeleteTarget(action)}
              />
            ))}
          </div>
        )}

        {runAction.sentTo && (
          <div className="flex items-center gap-1.5 px-3 py-2 text-xs text-muted-foreground">
            <Loader2 className="size-3 animate-spin" />
            Start sent to {runAction.sentTo} — waiting for the desktop…
          </div>
        )}
      </div>

      <ActionEditorDialog
        open={editorOpen}
        onOpenChange={setEditorOpen}
        teamId={team.id}
        repos={repos}
        action={editing}
        onSaved={() => void refetch()}
      />

      <RunActionDialog
        open={runTarget !== null}
        onOpenChange={(next) => {
          if (!next) setRunTarget(null)
        }}
        actionName={runTarget?.name ?? ``}
        devices={runAction.devices}
        starting={runAction.starting}
        onStart={(device, options) => {
          if (!runTarget) return
          runAction
            .start(device, runTarget.id, options)
            .then(() => setRunTarget(null))
            .catch(() => {})
        }}
      />

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
              {deleting ? <Loader2 className="animate-spin" /> : <Trash2 />}
              Delete
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  )
}
