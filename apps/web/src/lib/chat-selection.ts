import { chatSelectionRange } from "@exp/domain-contract/chats"

// #EXP-1388 (chats.json SELECTION + ARCHIVE): the Chats list's selection as
// pure state transitions. `ids` keeps SELECTION order (the bar's actions act
// in it); `anchor` = the last clicked row (a shift-click's range start).
// `null` = not selecting.

export interface ChatSelection {
  ids: readonly string[]
  anchor: string | null
}

const settle = (ids: readonly string[], anchor: string | null): ChatSelection | null =>
  ids.length === 0 ? null : { ids, anchor }

/** A long-press (touch) or the menu's Select: enter with the row, or add it. */
export function chatSelectionEnter(selection: ChatSelection | null, id: string): ChatSelection {
  const ids = selection?.ids ?? []
  return { ids: ids.includes(id) ? ids : [...ids, id], anchor: id }
}

/** A click on a row. Not selecting: cmd/ctrl or shift starts the selection
 *  with the row, a plain click OPENS it. Selecting: shift adds the range
 *  from the anchor (`chatSelectionRange`), any other click toggles; the
 *  last row out leaves the selection. */
export function chatSelectionClick(
  selection: ChatSelection | null,
  input: { id: string; order: readonly string[]; toggle: boolean; range: boolean }
): { selection: ChatSelection | null; open: boolean } {
  const { id } = input
  if (!selection) {
    if (input.toggle || input.range) return { selection: { ids: [id], anchor: id }, open: false }
    return { selection: null, open: true }
  }
  if (input.range) {
    const add = chatSelectionRange(input.order, selection.anchor, id).filter((rowId) => !selection.ids.includes(rowId))
    return { selection: { ids: [...selection.ids, ...add], anchor: id }, open: false }
  }
  const ids = selection.ids.includes(id) ? selection.ids.filter((rowId) => rowId !== id) : [...selection.ids, id]
  return { selection: settle(ids, id), open: false }
}

/** The selection keeps only rows still in the list (a filter, a delete, a
 *  chat that moved to the other view); none left = not selecting. */
export function chatSelectionPrune(selection: ChatSelection | null, order: readonly string[]): ChatSelection | null {
  if (!selection) return null
  const present = new Set(order)
  const ids = selection.ids.filter((id) => present.has(id))
  if (ids.length === selection.ids.length) return selection
  return settle(ids, selection.anchor && present.has(selection.anchor) ? selection.anchor : null)
}
