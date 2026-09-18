import { useMemo, type ReactNode } from "react"
import type { TeamAction } from "@/components/action-editor-dialog"
import {
  Combobox,
  ComboboxList,
  getActionIcon,
  type PickerOption,
} from "@exp/ui"

// EXP-825: the composer's action picker — the launch dialog's Actions tab
// (EXP-257/EXP-768) as a popover off the card's ▶ tool. Single-select: a row
// becomes the subject (and the popover closes). Both listed builtins ride
// the list now — "Fix merge conflicts" pinned first, then "Create action"
// (its dedicated dialog is gone: the request is the composer text) — and the
// hidden Chat builtin never does (no subject IS the chat).
//
// EXP-941: the shared `Combobox`. The row keeps its two-liner body (icon,
// name over an optional one-line description) through `renderOption`; the
// picked row wears the primitive's single-select trailing check.

function useActionRows(actions: TeamAction[] | null) {
  const options = useMemo<PickerOption[]>(
    () =>
      (actions ?? []).map((action) => ({
        value: action.id,
        label: action.name,
        keywords: [action.name, action.description ?? ``],
      })),
    [actions]
  )
  const byId = useMemo(
    () => new Map((actions ?? []).map((action) => [action.id, action])),
    [actions]
  )
  const renderOption = (option: PickerOption) => {
    const action = byId.get(option.value)
    const RowIcon = action ? getActionIcon(action) : null
    return (
      <>
        {RowIcon && (
          <RowIcon className="size-4 shrink-0 text-muted-foreground" />
        )}
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate text-sm">{option.label}</span>
          {action?.description && (
            <span className="truncate text-xs text-muted-foreground">
              {action.description}
            </span>
          )}
        </span>
      </>
    )
  }
  return { options, renderOption }
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
  const { options, renderOption } = useActionRows(actions)
  return (
    // EXP-946: shrinks with its host, like the issue picker beside it.
    <ComboboxList
      options={options}
      value={selectedActionId}
      onChange={(actionId) => {
        if (actionId) onSelect(actionId)
      }}
      loading={actions === null}
      placeholder="Search actions"
      emptyText="No actions match."
      renderOption={renderOption}
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
  const { options, renderOption } = useActionRows(actions)
  return (
    // EXP-946: capped to the space its side has, so it never runs off the top
    // of the window — the primitive caps every popover the same way.
    <Combobox
      options={options}
      value={selectedActionId}
      onChange={(actionId) => {
        if (actionId) onSelect(actionId)
      }}
      loading={actions === null}
      disabled={disabled}
      width="xl"
      mobileTitle="Actions"
      placeholder="Search actions"
      emptyText="No actions match."
      data-testid="agent-composer-actions-picker"
      renderOption={renderOption}
      renderTrigger={() => children}
    />
  )
}
