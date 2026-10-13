import { useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import { useNavigate } from "@tanstack/react-router"
import {
  conceptIcon,
  getActionIcon,
  Button,
  Menu,
  SkeletonRows,
} from "@exp/ui"
import type { SyncedAction, Team } from "@/db/schema"
import { actionCollection } from "@/lib/collections"
import { ActionPromptForm } from "@/components/action-prompt-form"
import { DeleteActionDialog } from "@/components/team-actions-panel"
import { PinToggleButton } from "@/components/pin-toggle-button"
import { useOpenComposer } from "@/hooks/use-open-composer"
import { useTeamPermissions } from "@/hooks/use-team-permissions"

// ONE action as a page. EXP-1293: an action is a SAVED PROMPT, so the page
// is its header (back, icon, name, pin, ⋯ Delete, Run) over the ONE form —
// no tabs, no triggers (they live on team agents now), no runs list. Run
// seeds the home composer with the action as its subject; the session it
// starts is a thread like any other. The row is read from the synced shape,
// so the page settles on its own as Electric catches up.

const UiBackIcon = conceptIcon(`ui-back`)
const UiMoreIcon = conceptIcon(`ui-more`)
const UiDeleteIcon = conceptIcon(`ui-delete`)
const ActionRunIcon = conceptIcon(`action-run`)

export function ActionPage({
  team,
  teamSlug,
  actionId,
}: {
  team: Team
  teamSlug: string
  actionId: string
}) {
  const navigate = useNavigate()
  const { isMember, isOwner } = useTeamPermissions(team)
  const openComposer = useOpenComposer()

  const { data: actionRows } = useLiveQuery(
    (query) =>
      query.from({ a: actionCollection }).where(({ a }) => eq(a.id, actionId)),
    [actionId]
  )
  const action = (actionRows?.[0] ?? null) as SyncedAction | null

  const [deleteOpen, setDeleteOpen] = useState(false)

  const backToList = () =>
    void navigate({ to: `/t/$teamSlug/actions`, params: { teamSlug } })

  if (!isMember) return null
  if (actionRows === undefined) {
    return (
      <div className="p-6">
        <SkeletonRows />
      </div>
    )
  }
  if (!action) {
    return (
      <div className="flex flex-col items-start gap-3 p-6 text-sm text-muted-foreground">
        This action no longer exists.
        <Button variant="outline" size="sm" onClick={backToList}>
          Back to actions
        </Button>
      </div>
    )
  }

  const ActionIcon = getActionIcon(action)

  return (
    <>
      <header className="flex items-center gap-2 pb-4" data-testid="action-page-header">
        <Button
          variant="ghost"
          size="icon-sm"
          onClick={backToList}
          aria-label="Back to actions"
        >
          <UiBackIcon />
        </Button>
        <ActionIcon className="size-4 shrink-0 text-foreground/70" />
        <h1 className="min-w-0 flex-1 truncate text-base font-medium">
          {action.name}
        </h1>
        <PinToggleButton teamId={action.teamId} kind="action" targetId={action.id} />
        {isOwner && (
          <Menu
            align="end"
            aria-label={`${action.name} actions`}
            title={action.name}
            entries={[
              {
                kind: `item`,
                id: `delete`,
                label: `Delete`,
                icon: UiDeleteIcon,
                destructive: true,
                onSelect: () => setDeleteOpen(true),
              },
            ]}
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
        )}
        <Button
          variant="glass"
          size="icon"
          // EXP-825: the home composer with this action as the subject chip.
          onClick={() => openComposer({ actionId: action.id })}
          aria-label="Run"
          title="Run"
          data-testid="action-run"
        >
          <ActionRunIcon />
        </Button>
      </header>

      <ActionPromptForm action={action} teamId={team.id} readOnly={!isOwner} />

      <DeleteActionDialog
        action={deleteOpen ? action : null}
        onClose={() => setDeleteOpen(false)}
        onDeleted={backToList}
      />
    </>
  )
}
