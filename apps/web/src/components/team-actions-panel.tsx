import { useMemo, useState } from "react"
import { deleteActionPrompt, promptActions } from "@/lib/prompts"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import type { SyncedAction, Team } from "@/db/schema"
import { actionCollection } from "@/lib/collections"
import {
  conceptIcon,
  Button,
  Pill,
  EmptyCta,
  GlassSectionHeader,
  ListRow,
  Prompt,
  Menu,
  getActionIcon,
  SkeletonRows,
  type MenuEntry,
} from "@exp/ui"
import { trpc } from "@/lib/trpc-client"
import { CreateActionDialog } from "@/components/action-prompt-form"
import { Empty } from "@/components/empty"
import { useOpenComposerInTeam } from "@/hooks/use-open-composer"
import { TeamBandHeader } from "@/components/team/team-band"
import { useTeamPermissions } from "@/hooks/use-team-permissions"
import {
  pinLabel,
  usePinToggleVisible,
} from "@/components/pin-toggle-button"
import { usePinToggle } from "@/hooks/use-pins"

// The team Actions surface (EXP-257/EXP-530), extracted from the Agents route
// in EXP-574: ONE list of actions, the same on every viewport (EXP-1270: the
// Suggestions tab and the lightbulb are gone ×4). EXP-1293: an action is a
// SAVED PROMPT — no triggers (they live on team agents), no builtins. A row
// opens the action's page (`actions/$actionId`); ▶ seeds the home composer.

// EXP-431: the create entry points share the cross-client `action-create`
// concept (desktop's `registry::ACTION_CREATE`), never a raw glyph.
const ActionCreateIcon = conceptIcon(`action-create`)
// EXP-615: running is a play icon button on every client — no text label.
const ActionRunIcon = conceptIcon(`action-run`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiPinIcon = conceptIcon(`ui-pin`)
const UiUnpinIcon = conceptIcon(`ui-unpin`)
const UiEditIcon = conceptIcon(`ui-edit`)
const UiDeleteIcon = conceptIcon(`ui-delete`)

// The row's ⋯ menu. EXP-778: every member gets Pin/Unpin (a
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
  const pin = usePinToggle(action.teamId, `action`, action.id)
  if (!pinVisible && !isOwner) return null
  const entries: MenuEntry[] = [
    ...(pinVisible
      ? [
          {
            kind: `item` as const,
            id: `pin`,
            label: pinLabel(pin.pinned),
            icon: pin.pinned ? UiUnpinIcon : UiPinIcon,
            onSelect: pin.toggle,
          },
        ]
      : []),
    ...(isOwner
      ? [
          { kind: `item` as const, id: `edit`, label: `Edit`, icon: UiEditIcon, onSelect: onEdit },
          {
            kind: `item` as const,
            id: `delete`,
            label: `Delete`,
            icon: UiDeleteIcon,
            destructive: true,
            onSelect: onDelete,
          },
        ]
      : []),
  ]
  return (
    <Menu
      align="end"
      aria-label={`${action.name} actions`}
      title={action.name}
      entries={entries}
      trigger={
        <Button
          variant="ghost"
          size="icon-sm"
          aria-label={`Action menu for ${action.name}`}
        >
          <UiMoreIcon />
        </Button>
      }
    />
  )
}

// One action as a row on every viewport (EXP-618 — native-app parity; the
// EXP-257 desktop card grid unified onto this shape).
function ActionRow({
  action,
  isOwner,
  onRun,
  onOpen,
  onDelete,
}: {
  action: SyncedAction
  isOwner: boolean
  onRun: () => void
  onOpen: () => void
  onDelete: () => void
}) {
  const RowIcon = getActionIcon(action)
  return (
    // EXP-862: every flat row takes the hover wash; a row opens the action's
    // page (the ▶ and the menu stop the click underneath them).
    <ListRow
      interactive
      onClick={onOpen}
      // A clickable row is a button to assistive tech; without its own name
      // it would be called after everything inside it, including the "..."
      // menu's label, and mask that control.
      aria-label={action.name}
      data-testid="action-row"
    >
      <RowIcon className="size-4 shrink-0 text-foreground/70" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5 text-sm">
          <span className="truncate font-medium">{action.name}</span>
        </div>
      </div>
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
      <span onClick={(event) => event.stopPropagation()}>
        <ActionMenu
          action={action}
          isOwner={isOwner}
          onEdit={onOpen}
          onDelete={onDelete}
        />
      </span>
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
  action: SyncedAction | null
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
  const deleteCopy = deleteActionPrompt(action?.name ?? ``)

  return (
    <Prompt
      open={action !== null}
      onOpenChange={(next) => {
        if (!next) onClose()
      }}
      busy={deleting}
      title={deleteCopy.title}
      body={deleteCopy.body}
      actions={promptActions(deleteCopy, {
        delete: { busy: deleting, onSelect: confirmDelete },
      })}
    />
  )
}

// Rendered while the team has no actions yet (EXP-431): the create flow
// never poses as a list entry, so the empty list nudges toward "New action".
// EXP-962: that dashed nudge IS `EmptyCta`.
function NoActionsNudge({ onClick }: { onClick: () => void }) {
  return (
    <EmptyCta
      icon={ActionCreateIcon}
      title="No actions yet"
      description="Save a prompt your team runs again and again."
      onClick={onClick}
    />
  )
}

export function TeamActionsPanel({
  team,
  teams,
  grouped = false,
}: {
  team: Team
  /** EXP-1186: the teams the list reads (the phone: every member team);
   *  absent = `team` alone. */
  teams?: readonly Team[]
  /** EXP-1186: one band per team (team mark + name) instead of the one
   *  "Actions" band. */
  grouped?: boolean
}) {
  const { isMember } = useTeamPermissions(team)
  const [deleteTarget, setDeleteTarget] = useState<SyncedAction | null>(null)

  if (!isMember) return null

  const sectionTeams = grouped && teams && teams.length > 1 ? teams : [team]
  const actionsSection = (
    <>
      {sectionTeams.map((row) => (
        <TeamActionsSection
          key={row.id}
          team={row}
          grouped={sectionTeams.length > 1}
          onDelete={setDeleteTarget}
        />
      ))}
    </>
  )

  return (
    <>
      {actionsSection}

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
  onDelete,
}: {
  team: Team
  grouped: boolean
  onDelete: (action: SyncedAction) => void
}) {
  const { isMember, isOwner } = useTeamPermissions(team)
  const navigate = useNavigate()
  const [createOpen, setCreateOpen] = useState(false)

  const teamId = team.id
  // EXP-1186: starts go to the action's OWN team.
  const openComposer = useOpenComposerInTeam()

  // Actions ride the Electric `actions` shape since EXP-268 (body excluded —
  // the action page fetches it via tRPC), so an MCP-authored action just
  // appears; no refetch machinery.
  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.teamId, teamId)),
    [teamId]
  )

  // The synced rows re-apply the server's ordering (sortOrder asc, then name
  // — collections hydrate unordered).
  const sortedActions = useMemo<SyncedAction[] | null>(() => {
    if (!isMember || actionRows === undefined) return null
    return [...(actionRows as SyncedAction[])].sort(
      (a, b) => a.sortOrder - b.sortOrder || a.name.localeCompare(b.name)
    )
  }, [isMember, actionRows])

  if (!isMember) return null

  const openAction = (actionId: string) =>
    void navigate({
      to: `/t/$teamSlug/actions/$actionId`,
      params: { teamSlug: team.slug, actionId },
    })
  const actionItemProps = (action: SyncedAction) => ({
    action,
    isOwner,
    // EXP-825: the composer with this action as the subject chip.
    onRun: () => openComposer(team.slug, { actionId: action.id }),
    onOpen: () => openAction(action.id),
    onDelete: () => onDelete(action),
  })

  // EXP-1293: "New action" opens the action form in create mode (the
  // builtin "Create action" session is gone); owner-only like every write.
  const canCreateAction = isOwner
  const openCreateAction = () => setCreateOpen(true)
  // A grouped team with nothing to list and nothing to create is no band.
  if (grouped && sortedActions?.length === 0 && !canCreateAction) return null
  const trailing = canCreateAction ? (
    <Pill mode="action" onClick={openCreateAction}>
      <ActionCreateIcon className="size-3" />
      New action
    </Pill>
  ) : undefined
  return (
    <div className={grouped ? `mb-4 last:mb-0` : undefined}>
      {grouped ? (
        <TeamBandHeader team={team} trailing={trailing} />
      ) : (
        <GlassSectionHeader label="Actions" trailing={trailing} />
      )}
      {sortedActions === null ? (
        // D8 first load: the ActionRow (ListRow, 44px, leading icon) geometry.
        <SkeletonRows />
      ) : (
        <div className="flex flex-col gap-0">
          {sortedActions.map((action) => (
            <ActionRow key={action.id} {...actionItemProps(action)} />
          ))}
          {canCreateAction && !grouped && sortedActions.length === 0 && (
            <NoActionsNudge onClick={openCreateAction} />
          )}
          {!canCreateAction && !grouped && sortedActions.length === 0 && (
            <Empty id="actions" />
          )}
        </div>
      )}
      {canCreateAction && (
        <CreateActionDialog
          open={createOpen}
          onOpenChange={setCreateOpen}
          teamId={team.id}
          onCreated={openAction}
        />
      )}
    </div>
  )
}
