import { useCallback, useEffect, useMemo, useRef, useState, type MouseEvent, type RefObject } from "react"
import { useIssueMenuSelection } from "@/components/issue-context-menu/selection"
import {
  chatSelectionClick,
  chatSelectionEnter,
  chatSelectionPrune,
  type ChatSelection,
} from "@/lib/chat-selection"

const OPEN_OVERLAY = `[data-state="open"][role="menu"], [data-state="open"][role="listbox"], [data-state="open"][role="dialog"], [data-state="open"][role="alertdialog"]`

// #EXP-1388 (chats.json SELECTION): the Chats list's selection, shared by the
// team's Chats page and (#EXP-2027) the team-less one: a touch long-press or
// the pointer menu's Select enters it (through the layout's menu host, the
// list registered like the issue list), cmd/ctrl-click toggles, shift-click
// ranges, the wide checkbox column toggles and Cmd/Ctrl+A selects every row.
// It follows the DISPLAYED order and keeps only rows still listed; a view
// switch clears it.

export function useChatListSelection(
  order: readonly string[],
  view: string,
  open: (sessionId: string) => void
): {
  selection: ChatSelection | null
  selectedIds: ReadonlySet<string> | null
  clear: () => void
  listRef: RefObject<HTMLDivElement | null>
  onRowClick: (sessionId: string, event: MouseEvent<HTMLButtonElement>) => void
  onSelectToggle: (sessionId: string, event: MouseEvent<HTMLElement>) => void
} {
  const [selection, setSelection] = useState<ChatSelection | null>(null)
  useEffect(() => {
    setSelection((current) => chatSelectionPrune(current, [...order]))
  }, [order])
  useEffect(() => {
    setSelection(null)
  }, [view])
  const clear = useCallback(() => setSelection(null), [])
  const listRef = useRef<HTMLDivElement>(null)
  const selectionRef = useRef(selection)
  selectionRef.current = selection
  useIssueMenuSelection(
    useMemo(
      () => ({
        root: listRef,
        isSelected: (id: string) => selectionRef.current?.ids.includes(id) ?? false,
        toggle: (id: string) => setSelection((current) => chatSelectionEnter(current, id)),
      }),
      []
    )
  )
  const selectedIds = useMemo(() => (selection ? new Set(selection.ids) : null), [selection])

  const onRowClick = (sessionId: string, event: MouseEvent<HTMLButtonElement>) => {
    const next = chatSelectionClick(selectionRef.current, {
      id: sessionId,
      order: [...order],
      toggle: event.metaKey || event.ctrlKey,
      range: event.shiftKey,
    })
    if (next.open) open(sessionId)
    else setSelection(next.selection)
  }

  // The wide checkbox column: a click toggles (shift = the range), the
  // issue row's checkbox cell.
  const onSelectToggle = (sessionId: string, event: MouseEvent<HTMLElement>) => {
    const next = chatSelectionClick(selectionRef.current, {
      id: sessionId,
      order: [...order],
      toggle: true,
      range: event.shiftKey,
    })
    setSelection(next.selection)
  }

  // Cmd/Ctrl+A selects every row, the issue list's rule: never while an
  // overlay is up or a field (or anything outside the list) has focus.
  const orderRef = useRef(order)
  orderRef.current = order
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.defaultPrevented || !(event.metaKey || event.ctrlKey) || event.key.toLowerCase() !== `a`) return
      if (document.querySelector(OPEN_OVERLAY) !== null) return
      const active = document.activeElement
      if (active !== null && active !== document.body && !listRef.current?.contains(active)) return
      if (orderRef.current.length === 0) return
      event.preventDefault()
      const ids = [...orderRef.current]
      setSelection({ ids, anchor: ids[ids.length - 1] ?? null })
    }
    window.addEventListener(`keydown`, onKey)
    return () => window.removeEventListener(`keydown`, onKey)
  }, [])

  return { selection, selectedIds, clear, listRef, onRowClick, onSelectToggle }
}
