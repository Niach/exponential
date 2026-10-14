import { useMemo } from "react"
import type { ComposerAction } from "@/hooks/use-launch-composer"
import { SearchList, actionPickerItems } from "@exp/ui"
import { COMPOSER_MENU_TEST_IDS } from "@/components/launch-dialog/composer-menu"

// EXP-825: the composer's action picker — the launch dialog's Actions tab
// (EXP-257/EXP-768). Single-select: a row becomes the subject. EXP-1249: it
// is the "+" menu's Run action submenu body now (the ▶ tool is gone), and the
// menu closes on the pick. EXP-1293: the team's own actions only (the
// builtins are gone).
//
// SLOP-28 (composer-menu.json): THE search list, the same component and row
// height as the Implement issue submenu beside it. An action row is its
// curated icon (`getActionIcon` inside `actionPickerItems`) and its NAME;
// the description is dropped.

export function ActionPickerList({
  actions,
  selectedActionId,
  onSelect,
}: {
  /** The team's actions in server order; null while the shape is loading. */
  actions: ComposerAction[] | null
  selectedActionId: string | null
  onSelect: (actionId: string) => void
}) {
  const items = useMemo(
    () =>
      actionPickerItems(
        (actions ?? []).map((action) => ({
          id: action.id,
          name: action.name,
          icon: action.icon,
        }))
      ),
    [actions]
  )
  return (
    <SearchList
      mode="single"
      items={items}
      value={selectedActionId}
      onChange={onSelect}
      loading={actions === null}
      placeholder="Search actions"
      emptyText="No actions match."
      inputTestId={COMPOSER_MENU_TEST_IDS.submenuSearch}
    />
  )
}
