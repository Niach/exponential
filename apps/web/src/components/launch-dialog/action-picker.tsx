import { useMemo } from "react"
import type { TeamAction } from "@/components/action-prompt-form"
import {
  PickerList,
  actionPickerItems,
  type ActionPickerAction,
} from "@exp/ui"

// EXP-825: the composer's action picker — the launch dialog's Actions tab
// (EXP-257/EXP-768). Single-select: a row becomes the subject. EXP-1249: it
// is the "+" menu's Run action submenu body now (the ▶ tool is gone), and the
// menu closes on the pick. Both listed builtins ride
// the list now — "Fix merge conflicts" pinned first, then "Create action"
// (its dedicated dialog is gone: the request is the composer text) — and the
// hidden Chat builtin never does (no subject IS the chat).
//
// EXP-1030: the shared typed picker (`@exp/ui` `ActionPicker` / `PickerList`,
// EXP-1021). The row is the primitive's now — the action's curated icon
// (`getActionIcon` inside `actionPickerItems`), its name, and its description
// as the muted second line — so the composer draws the same action row as the
// trigger editor beside it.

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
