import { useMemo, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import type { SyncedAction, Team } from "@/db/schema"
import { actionCollection } from "@/lib/collections"
import { BUILTIN_CREATE_ACTION_ID } from "@/lib/builtin-actions"
import { parseActionTriggers, triggerBadges } from "@/lib/action-triggers"
import { LoaderCircle, Ellipsis, Pencil, Trash2 } from "lucide-react"
import {
  conceptIcon,
  Button,
  Pill,
  Dialog,
  DialogCancel,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
  EmptyCta,
  GlassSectionHeader,
  ListRow,
  Tabs,
  TabsContent,
  TabsList,
  TabsTrigger,
  getActionIcon,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { useSteerConfig } from "@/components/agent-session"
import type { TeamAction } from "@/components/action-prompt-form"
import { TriggerGlyph } from "@/components/action-triggers-section"
import { useOpenComposerInTeam } from "@/hooks/use-open-composer"
import { TeamBandHeader } from "@/components/team/team-band"
import { SuggestionsButton } from "@/components/getting-started/getting-started-sheet"
import {
  ActionSuggestionsPanel,
} from "@/components/action-suggestions-list"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import {
  PinToggleMenuItem,
  usePinToggleVisible,
} from "@/components/pin-toggle-button"

// The team Actions surface (EXP-257/EXP-530), extracted from the Agents route
// in EXP-574: ONE list of actions (SLOP-2 — an action carries its triggers,
// so there is no separate automations list). On a desktop viewport `/actions`
// renders the list alone; on mobile it keeps the native-parity Actions ·
// Suggestions tabs, driven by `?tab=`. Suggestions live in Getting started on
// desktop — the lightbulb in the section header goes there. A row opens the
// action's page (`actions/$actionId`: prompt, triggers, runs).

// EXP-431: the create entry points share the cross-client `action-create`
// concept (desktop's `registry::ACTION_CREATE`), never a raw glyph.
const ActionCreateIcon = conceptIcon(`action-create`)
// EXP-615: running is a play icon button on every client — no text label.
const ActionRunIcon = conceptIcon(`action-run`)

/** Which surface this panel renders. `tabs` is the mobile Actions page (the
 * list and the suggestions behind a tab strip); `actions` the desktop one. */
export type ActionsPanelView = `tabs` | `actions`
/** The mobile tab strip's value — also the `?tab=` search param. */
export type ActionsPanelTab = `actions` | `suggestions`

// The row's ⋯ menu — hidden entirely on the builtin (server-shipped, not
// editable, deletable or pinnable). EXP-778: every member gets Pin/Unpin (a
// pin is personal), on a sidebar-width viewport only (EXP-858); Edit and
// Delete stay owner-only, like the IDE's row menu. Both rules are
// conditional, so the TRIGGER asks first whether anything would be in the
// menu: a non-owner member on a phone viewport gets no `⋯` at all rather than
// one that opens an empty popover.
export function ActionMenu({
  action,
  isOwner,
  onEdit,
  onDelete,
}: {
  action: SyncedAction
  isOwner: boolean
  onEdit: () => void
  onDelete: () => void
}) {
  const pinVisible = usePinToggleVisible()
  if (!pinVisible && !isOwner) return null
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Action menu for ${action.name}`}
        >
          <Ellipsis />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <PinToggleMenuItem
          teamId={action.teamId}
          kind="action"
          targetId={action.id}
        />
        {isOwner && (
          <>
            <DropdownMenuItem onClick={onEdit}>
              <Pencil className="h-4 w-4" />
              Edit
            </DropdownMenuItem>
            <DropdownMenuItem variant="destructive" onClick={onDelete}>
              <Trash2 className="h-4 w-4" />
              Delete
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  )
}

// One action as a row on every viewport (EXP-618 — native-app parity; the
// EXP-257 desktop card grid unified onto this shape).
function ActionRow({
  action,
  isOwner,
  canRun,
  onRun,
  onOpen,
  onDelete,
}: {
  action: TeamAction
  isOwner: boolean
  canRun: boolean
  onRun: () => void
  onOpen: () => void
  onDelete: () => void
}) {
  const RowIcon = getActionIcon(action)
  // SLOP-2: which trigger kinds the action carries — a glyph each beside the
  // name, muted while none of that kind is enabled. The triggers themselves
  // are on the action's page.
  const badges = triggerBadges(
    action.builtin ? [] : parseActionTriggers(action.triggers)
  )
  return (
    // EXP-862: every flat row takes the hover wash; a row opens the action's
    // page (the ▶ and the menu stop the click underneath them).
    <ListRow
      interactive
      onClick={action.builtin ? undefined : onOpen}
      // A clickable row is a button to assistive tech; without its own name
      // it would be called after everything inside it, including the "..."
      // menu's label, and mask that control.
      aria-label={action.builtin ? undefined : action.name}
    >
      <RowIcon className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{action.name}</span>
          {([`schedule`, `event`] as const).map((kind) => {
            const badge = badges[kind]
            if (!badge) return null
            const label =
              kind === `schedule` ? `Runs on a schedule` : `Runs on an event`
            return (
              <span
                key={kind}
                title={badge.active ? label : `${label} (paused)`}
                className="flex shrink-0 items-center"
              >
                <TriggerGlyph
                  kind={kind}
                  className={`size-3 ${badge.active ? `text-muted-foreground` : `text-muted-foreground/40`}`}
                />
              </span>
            )
          })}
        </div>
        {action.description && (
          <div className="line-clamp-2 text-xs text-muted-foreground">
            {action.description}
          </div>
        )}
      </div>
      {canRun && (
        <Button
          variant="glass"
          size="icon"
          onClick={(event) => {
            event.stopPropagation()
            onRun()
          }}
          aria-label="Run"
          title="Run"
        >
          <ActionRunIcon />
        </Button>
      )}
      {!action.builtin && (
        <span onClick={(event) => event.stopPropagation()}>
          <ActionMenu
            action={action}
            isOwner={isOwner}
            onEdit={onOpen}
            onDelete={onDelete}
          />
        </span>
      )}
    </ListRow>
  )
}

/** The delete confirm — the list's row menu and the action page's menu. */
export function DeleteActionDialog({
  action,
  onClose,
  onDeleted,
}: {
  /** The action to delete; null = closed. */
  action: TeamAction | null
  onClose: () => void
  onDeleted?: () => void
}) {
  const [deleting, setDeleting] = useState(false)
  const confirmDelete = async () => {
    if (!action) return
    setDeleting(true)
    try {
      // Failures surface via the global mutation-error toast; the synced
      // collection drops the row.
      await trpc.actions.delete.mutate({ id: action.id })
      onClose()
      onDeleted?.()
    } catch {
      // Toast already shown; keep the confirm open for a retry.
    } finally {
      setDeleting(false)
    }
  }
  return (
    <Dialog
      open={action !== null}
      onOpenChange={(next) => {
        if (!next && !deleting) onClose()
      }}
    >
      <DialogContent mobile="alert" className="sm:max-w-sm">
        <DialogHeader>
          <DialogTitle>Delete action</DialogTitle>
          <DialogDescription>
            {`Delete "${action?.name ?? ``}"? Its triggers go with it. Live runs keep going and keep their label; this cannot be undone.`}
          </DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <DialogCancel onClick={onClose} disabled={deleting} />
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
  )
}

// Rendered after the builtin row(s) while the team has no custom actions yet
// (EXP-431) — the create flow no longer poses as a list entry, so the empty-ish
// list nudges toward the "New action" button's dialog instead. EXP-962: that
// dashed nudge IS `EmptyCta`.
function NoCustomActionsNudge({ onClick }: { onClick: () => void }) {
  return (
    <EmptyCta
      icon={ActionCreateIcon}
      title="No custom actions yet"
      description="Describe one and your agent will build it."
      onClick={onClick}
    />
  )
}

export function TeamActionsPanel({
  team,
  teams,
  grouped = false,
  view,
  tab = `actions`,
  onTabChange,
}: {
  team: Team
  /** EXP-1186: the teams the list reads (the phone: every member team);
   *  absent = `team` alone. */
  teams?: readonly Team[]
  /** EXP-1186: one band per team (team mark + name) instead of the one
   *  "Actions" band. */
  grouped?: boolean
  view: ActionsPanelView
  /** `tabs` view only — the controlled tab, i.e. the route's `?tab=`. */
  tab?: ActionsPanelTab
  onTabChange?: (tab: ActionsPanelTab) => void
}) {
  const { isMember } = useTeamPermissions(team)
  const [deleteTarget, setDeleteTarget] = useState<TeamAction | null>(null)

  if (!isMember) return null

  // EXP-686: the seeds live in Getting started on a desktop viewport; the
  // mobile tabs keep their own Suggestions tab.
  const showSuggestions = view !== `tabs`
  const sectionTeams = grouped && teams && teams.length > 1 ? teams : [team]
  const actionsSection = (
    <>
      {sectionTeams.map((row) => (
        <TeamActionsSection
          key={row.id}
          team={row}
          grouped={sectionTeams.length > 1}
          showSuggestions={showSuggestions}
          onDelete={setDeleteTarget}
        />
      ))}
    </>
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
            <TabsTrigger value="suggestions" className="flex-1">
              Suggestions
            </TabsTrigger>
          </TabsList>

          <TabsContent value="actions">{actionsSection}</TabsContent>
          <TabsContent value="suggestions">
            <ActionSuggestionsPanel team={team} />
          </TabsContent>
        </Tabs>
      ) : (
        actionsSection
      )}

      <DeleteActionDialog
        action={deleteTarget}
        onClose={() => setDeleteTarget(null)}
      />
    </>
  )
}

/** One team's actions under its band — the whole list on md+ and for a
 *  single team; one of several (EXP-1186) on a phone in more than one team,
 *  where the band carries the team mark + name and every start, open and
 *  create goes to THAT team. */
function TeamActionsSection({
  team,
  grouped,
  showSuggestions,
  onDelete,
}: {
  team: Team
  grouped: boolean
  showSuggestions: boolean
  onDelete: (action: TeamAction) => void
}) {
  const { isMember, isOwner } = useTeamPermissions(team)
  const steerConfig = useSteerConfig()
  const navigate = useNavigate()

  const teamId = team.id
  // Steer tickets require team membership and a configured relay; the
  // server enforces both at mint time, this only decides whether the
  // interactive affordances render.
  const steerEnabled = Boolean(isMember && steerConfig?.enabled)
  // EXP-1186: starts go to the action's OWN team (the route's team keeps
  // the dialog).
  const openComposer = useOpenComposerInTeam()

  // Actions ride the Electric `actions` shape since EXP-268 (body excluded —
  // the action page fetches it via tRPC), so a builtin "Create action" run's
  // MCP-authored action just appears; no refetch machinery.
  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )

  // The synced rows re-apply the server's ordering (sortOrder asc, then name
  // — collections hydrate unordered). No builtin is LISTED: "Create action"
  // lives behind the section's own "New action" button (EXP-431), and
  // EXP-686 hid "Fix merge conflicts" too — it is launched from Reviews and
  // over MCP, never picked out of this list.
  const sortedActions = useMemo<TeamAction[] | null>(() => {
    if (!isMember || actionRows === undefined) return null
    return [...actionRows]
      .sort((a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name))
      .map((row) => ({ ...row, builtin: false as const }))
  }, [isMember, actionRows])

  if (!isMember) return null

  const actionItemProps = (action: TeamAction) => ({
    action,
    isOwner,
    canRun: steerEnabled,
    // EXP-825: the composer with this action as the subject chip.
    onRun: () => openComposer(team.slug, { actionId: action.id }),
    onOpen: () => {
      void navigate({
        to: `/t/$teamSlug/actions/$actionId`,
        params: { teamSlug: team.slug, actionId: action.id },
      })
    },
    onDelete: () => onDelete(action),
  })

  const canCreateAction = steerEnabled && isOwner
  // EXP-825: "New action" is the composer with the Create action builtin
  // picked — the request is typed there (its own dialog is gone).
  const openCreateAction = () =>
    openComposer(team.slug, { actionId: BUILTIN_CREATE_ACTION_ID })
  // A grouped team with nothing to list and nothing to create is no band.
  if (grouped && sortedActions?.length === 0 && !canCreateAction) return null
  const trailing =
    !showSuggestions && !canCreateAction ? undefined : (
      <>
        {showSuggestions && <SuggestionsButton />}
        {canCreateAction && (
          <Pill mode="action" onClick={openCreateAction}>
            <ActionCreateIcon className="size-3" />
            New action
          </Pill>
        )}
      </>
    )
  return (
    <div className={grouped ? `mb-4 last:mb-0` : undefined}>
      {grouped ? (
        <TeamBandHeader team={team} trailing={trailing} />
      ) : (
        <GlassSectionHeader label="Actions" trailing={trailing} />
      )}
      {sortedActions === null ? (
        <div className="px-1 py-3 text-sm text-muted-foreground">Loading…</div>
      ) : (
        <div className="flex flex-col gap-0">
          {sortedActions.map((action) => (
            <ActionRow key={action.id} {...actionItemProps(action)} />
          ))}
          {canCreateAction && !grouped && sortedActions.length === 0 && (
            <NoCustomActionsNudge onClick={openCreateAction} />
          )}
        </div>
      )}
    </div>
  )
}
