import { useMemo, type ReactNode } from "react"
import type { TeamAction } from "@/components/action-editor-dialog"
import {
  ActionPicker as SharedActionPicker,
  PickerList,
  actionPickerItems,
  type ActionPickerAction,
} from "@exp/ui"

// EXP-825: the composer's action picker — the launch dialog's Actions tab
// (EXP-257/EXP-768) as a popover off the card's ▶ tool. Single-select: a row
// becomes the subject (and the popover closes). Both listed builtins ride
// the list now — "Fix merge conflicts" pinned first, then "Create action"
// (its dedicated dialog is gone: the request is the composer text) — and the
// hidden Chat builtin never does (no subject IS the chat).
//
// EXP-1030: the shared typed picker (`@exp/ui` `ActionPicker` / `PickerList`,
// EXP-1021). The row is the primitive's now — the action's curated icon
// (`getActionIcon` inside `actionPickerItems`), its name, and its description
// as the muted second line — so the composer draws the same action row as the
// automation editor beside it.

/** The team's actions as the shared picker's rows; null = the shape is still
 *  loading, which the surface says instead of drawing an empty list. */
function useActionRows(actions: TeamAction[] | null): ActionPickerAction[] {
  return useMemo(
    () =>
      (actions ?? []).map((action) => ({
        id: action.id,
        name: action.name,
        icon: action.icon,
        description: action.description,
      })),
    [actions]
  )
}

export function ActionPickerList({
  actions,
  selectedActionId,
  onSelect,
}: {
  /** Builtin-first sorted list; null while the shape is loading. */
  actions: TeamAction[] | null
  selectedActionId: string | null
  onSelect: (actionId: string) => void
}) {
  const rows = useActionRows(actions)
  return (
    // EXP-946: shrinks with its host, like the issue picker beside it.
    <PickerList
      mode="single"
      items={actionPickerItems(rows)}
      value={selectedActionId}
      onChange={onSelect}
      search
      loading={actions === null}
      searchPlaceholder="Search actions"
      emptyText="No actions match."
    />
  )
}

/** The ▶ tool's popover (a bottom sheet on phones); closes on a pick. */
export function ActionPicker({
  actions,
  selectedActionId,
  onSelect,
  disabled,
  children,
}: {
  actions: TeamAction[] | null
  selectedActionId: string | null
  onSelect: (actionId: string) => void
  disabled?: boolean
  /** The trigger (a `ComposerTool`). */
  children: ReactNode
}) {
  const rows = useActionRows(actions)
  return (
    // EXP-946: capped to the space its side has, so it never runs off the top
    // of the window — the primitive caps every popover the same way.
    <SharedActionPicker
      actions={rows}
      value={selectedActionId}
      onChange={onSelect}
      disabled={disabled}
      width="xl"
      mobileTitle="Actions"
      searchPlaceholder="Search actions"
      // The typed picker forwards no `loading` (only the primitive under it
      // takes one), and a list that is merely UNSYNCED must not read as a
      // team with no actions — so the empty line says which it is.
      emptyText={actions === null ? `Loading…` : `No actions match.`}
      data-testid="agent-composer-actions-picker"
      trigger={children}
    />
  )
}
