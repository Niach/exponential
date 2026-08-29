import { useEffect, useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import type { Automation, Team } from "@/db/schema"
import { actionCollection, automationCollection } from "@/lib/collections"
import {
  BUILTIN_CREATE_ACTION_ID,
  BUILTIN_CREATE_ACTION_NAME,
  builtinFixConflictsAction,
} from "@/lib/builtin-actions"
import { LoaderCircle, Ellipsis, Pencil, Trash2 } from "lucide-react"
import { conceptIcon } from "@/lib/icons.generated"
import { trpc } from "@/lib/trpc-client"
import { useSteerConfig } from "@/components/agent-session"
import {
  ActionEditorDialog,
  type ActionRepoOption,
  type TeamAction,
} from "@/components/action-editor-dialog"
import { LaunchDialog } from "@/components/launch-dialog/launch-dialog"
import { SuggestionsButton } from "@/components/getting-started/getting-started-sheet"
import { CreateActionDialog } from "@/components/launch-dialog/create-action-dialog"
import { AutomationsTab } from "@/components/automations-tab"
import {
  ActionSuggestionsPanel,
} from "@/components/action-suggestions-list"
import { useRemoteStart } from "@/hooks/use-remote-start"
import { useSession } from "@/hooks/use-session"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
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
import { GlassRow, GlassSectionHeader } from "@/components/ui/glass-rows"
import {
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
} from "@/components/ui/tabs"
import { getActionIcon } from "@/lib/board-icons"

// The team Actions surface (EXP-257/EXP-530), extracted from the Agents route
// in EXP-574. EXP-686 split it across three routes: on a desktop viewport
// `/actions` renders the actions LIST and `/automations` the automations one,
// each a single view with no tab strip; on mobile `/actions` keeps the
// native-parity Actions · Automations · Suggestions tabs, driven by `?tab=`.
// Suggestions moved to Getting started on desktop — the lightbulb in the
// section header goes there.

// EXP-431: the create entry points share the cross-client `action-create`
// concept (desktop's `registry::ACTION_CREATE`), never a raw glyph.
const ActionCreateIcon = conceptIcon(`action-create`)
// EXP-530: the automation glyph is a cross-client concept too.
const ActionAutomationIcon = conceptIcon(`action-automation`)
const ActionRepositoryIcon = conceptIcon(`action-repository`)
// EXP-615: running is a play icon button on every client — no text label.
const ActionRunIcon = conceptIcon(`action-run`)

/** Which of the three surfaces this panel renders. `tabs` is the mobile
 * Actions page (all three behind a tab strip); the other two are the desktop
 * routes, each a single view. */
export type ActionsPanelView = `tabs` | `actions` | `automations`
/** The mobile tab strip's value — also the `?tab=` search param. */
export type ActionsPanelTab = `actions` | `automations` | `suggestions`

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

// One action as a row on every viewport (EXP-618 — native-app parity; the
// EXP-257 desktop card grid unified onto this shape).
function ActionRow({
  action,
  repoName,
  automationCount,
  isOwner,
  canRun,
  runBusy,
  onRun,
  onEdit,
  onDelete,
}: {
  action: TeamAction
  repoName: string | undefined
  /** How many automations target this action (EXP-583) — the schedules and
   * event watchers themselves live on the Automations tab. */
  automationCount: number
  isOwner: boolean
  canRun: boolean
  runBusy: boolean
  onRun: () => void
  onEdit: () => void
  onDelete: () => void
}) {
  const RowIcon = getActionIcon(action)
  return (
    <GlassRow>
      <RowIcon className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{action.name}</span>
          {/* FEED-15: native parity — a small "runs in a repository" glyph
              (the repo name in its tooltip) instead of the full-name badge
              that ate the name on phones. */}
          {repoName && (
            <span
              className="inline-flex shrink-0"
              title={`Runs in ${repoName}`}
              aria-label={`Runs in ${repoName}`}
            >
              <ActionRepositoryIcon className="size-3 text-muted-foreground/70" />
            </span>
          )}
        </div>
        {action.description && (
          <div className="line-clamp-2 text-xs text-muted-foreground">
            {action.description}
          </div>
        )}
        {automationCount > 0 && (
          <div className="flex items-center gap-1 text-xs text-muted-foreground">
            <ActionAutomationIcon className="size-3 shrink-0" />
            <span className="truncate">
              {`${automationCount} ${automationCount === 1 ? `automation` : `automations`}`}
            </span>
          </div>
        )}
      </div>
      {canRun && (
        <Button
          variant="glass"
          size="icon"
          disabled={runBusy}
          onClick={onRun}
          aria-label="Run"
          title="Run"
        >
          <ActionRunIcon />
        </Button>
      )}
      {isOwner && !action.builtin && (
        <ActionMenu action={action} onEdit={onEdit} onDelete={onDelete} />
      )}
    </GlassRow>
  )
}

// Rendered after the builtin row(s) while the team has no custom actions yet
// (EXP-431) — the create flow no longer poses as a list entry, so the empty-ish
// list nudges toward the "New action" button's dialog instead.
function NoCustomActionsNudge({ onClick }: { onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="flex w-full flex-col items-start gap-1 rounded-md border border-dashed border-glass-stroke-strong p-3 text-left text-sm text-muted-foreground hover:bg-muted/50"
    >
      <span className="flex items-center gap-2">
        <ActionCreateIcon className="size-4 shrink-0" />
        No custom actions yet
      </span>
      <span className="text-xs">
        Describe one and your agent will build it.
      </span>
    </button>
  )
}

export function TeamActionsPanel({
  team,
  view,
  tab = `actions`,
  onTabChange,
}: {
  team: Team
  view: ActionsPanelView
  /** `tabs` view only — the controlled tab, i.e. the route's `?tab=`. */
  tab?: ActionsPanelTab
  onTabChange?: (tab: ActionsPanelTab) => void
}) {
  const { data: session } = useSession()
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()

  const currentUserId = session?.user?.id
  const teamId = team.id
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

  // Actions ride the Electric `actions` shape since EXP-268 (body excluded —
  // editors fetch it via tRPC on open), so a builtin "Create action" run's
  // MCP-authored action just appears; no refetch machinery.
  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )

  // EXP-583: automations are their own synced rows — the Actions tab only
  // shows how many target each action; the rows themselves live one tab over.
  const { data: automationRows } = useLiveQuery(
    (query) =>
      query
        .from({ au: automationCollection })
        .where(({ au }) => eq(au.teamId, teamId)),
    [teamId]
  )
  const automationCountByAction = useMemo(() => {
    const counts = new Map<string, number>()
    for (const automation of (automationRows ?? []) as Automation[]) {
      counts.set(
        automation.actionId,
        (counts.get(automation.actionId) ?? 0) + 1
      )
    }
    return counts
  }, [automationRows])

  // The synced rows re-apply the server's ordering (sortOrder asc, then name
  // — collections hydrate unordered). Neither builtin is LISTED: "Create
  // action" lives behind the section's own "New action" button (EXP-431), and
  // EXP-686 hid "Fix merge conflicts" too — it is launched from Reviews and
  // over MCP, never picked out of this list.
  const sortedActions = useMemo<TeamAction[] | null>(() => {
    if (!isMember || actionRows === undefined) return null
    return [...actionRows]
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
      .map((row) => ({ ...row, builtin: false as const }))
  }, [isMember, actionRows])

  // …but the Automations tab still has to NAME a fix-conflicts run, so its
  // lookup pool keeps the builtin.
  const automationActions = useMemo<TeamAction[] | null>(
    () =>
      sortedActions === null
        ? null
        : [builtinFixConflictsAction(teamId), ...sortedActions],
    [teamId, sortedActions]
  )

  // Repo names for the badges + the editor's repository select.
  const [repos, setRepos] = useState<ActionRepoOption[]>([])
  useEffect(() => {
    if (!isMember) return
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

  // The unified launch dialog, opened here via an action's Run (Actions tab
  // pre-selected — its Issues tab keeps working for a device picked inside).
  const [launchActionId, setLaunchActionId] = useState<string | null>(null)

  // The dedicated "New action" creation dialog (EXP-431).
  const [createActionOpen, setCreateActionOpen] = useState(false)

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

  if (!isMember) return null

  const actionItemProps = (action: TeamAction) => ({
    action,
    repoName: action.repositoryId
      ? repoNameById.get(action.repositoryId)
      : undefined,
    automationCount: automationCountByAction.get(action.id) ?? 0,
    isOwner,
    canRun: steerEnabled,
    runBusy,
    onRun: () => setLaunchActionId(action.id),
    onEdit: () => {
      setEditing(action)
      setEditorOpen(true)
    },
    onDelete: () => setDeleteTarget(action),
  })

  const canCreateAction = steerEnabled && isOwner
  // EXP-686: the seeds live in Getting started on a desktop viewport; the
  // mobile tabs keep their own Suggestions tab.
  const showSuggestions = view !== `tabs`
  const actionsSection = (
    <>
      <GlassSectionHeader
        label="Actions"
        count={sortedActions?.length ?? 0}
        trailing={
          !showSuggestions && !canCreateAction ? undefined : (
            <>
              {showSuggestions && <SuggestionsButton />}
              {canCreateAction && (
                <Button
                  variant="outline"
                  size="sm"
                  className="h-6 gap-1 px-2 text-xs"
                  disabled={runBusy}
                  onClick={() => setCreateActionOpen(true)}
                >
                  <ActionCreateIcon className="size-3.5" />
                  New action
                </Button>
              )}
            </>
          )
        }
      />
      {sortedActions === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : (
        <div className="flex flex-col gap-2">
          {sortedActions.map((action) => (
            <ActionRow key={action.id} {...actionItemProps(action)} />
          ))}
          {canCreateAction && sortedActions.length === 0 && (
            <NoCustomActionsNudge onClick={() => setCreateActionOpen(true)} />
          )}
        </div>
      )}
    </>
  )

  const automationsSection = (
    <AutomationsTab
      actions={automationActions}
      devices={remote.devices ?? []}
      isOwner={isOwner}
      steerEnabled={steerEnabled}
      teamId={teamId}
      showSuggestions={showSuggestions}
    />
  )

  return (
    <>
      {view === `tabs` ? (
        <Tabs
          value={tab}
          onValueChange={(value) => onTabChange?.(value as ActionsPanelTab)}
          className="mb-4"
        >
          <TabsList className="w-full">
            <TabsTrigger value="actions" className="flex-1">
              Actions
            </TabsTrigger>
            <TabsTrigger value="automations" className="flex-1">
              Automations
            </TabsTrigger>
            <TabsTrigger value="suggestions" className="flex-1">
              Suggestions
            </TabsTrigger>
          </TabsList>

          <TabsContent value="actions">{actionsSection}</TabsContent>
          <TabsContent value="automations">{automationsSection}</TabsContent>
          <TabsContent value="suggestions">
            <ActionSuggestionsPanel team={team} />
          </TabsContent>
        </Tabs>
      ) : view === `actions` ? (
        actionsSection
      ) : (
        automationsSection
      )}

      <LaunchDialog
        open={launchActionId !== null}
        onOpenChange={(next) => {
          if (!next) setLaunchActionId(null)
        }}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={teamId}
        initialTab="actions"
        initialActionId={launchActionId ?? undefined}
        onStartIssues={(device, options, issueIds) => {
          remote
            .startIssues(device, options, issueIds)
            .then(() => setLaunchActionId(null))
            .catch(() => {})
        }}
        onRunAction={(device, action, options, inputs) => {
          remote
            .runAction(device, action, options, inputs)
            .then(() => setLaunchActionId(null))
            .catch(() => {})
        }}
      />

      <CreateActionDialog
        open={createActionOpen}
        onOpenChange={(next) => {
          if (!next) setCreateActionOpen(false)
        }}
        devices={remote.devices ?? []}
        starting={remote.starting}
        teamId={teamId}
        repos={repos}
        onCreate={(device, options, inputs) => {
          remote
            .runAction(
              device,
              {
                id: BUILTIN_CREATE_ACTION_ID,
                name: BUILTIN_CREATE_ACTION_NAME,
                teamId,
              },
              options,
              inputs
            )
            .then(() => setCreateActionOpen(false))
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
    </>
  )
}
