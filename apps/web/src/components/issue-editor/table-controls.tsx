import { useCallback, useEffect, useRef, useState } from "react"
import type { Editor } from "@tiptap/react"
import { TextSelection } from "@tiptap/pm/state"
import {
  cellAround,
  moveTableColumn,
  moveTableRow,
  TableMap,
} from "@tiptap/pm/tables"
import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { conceptIcon } from "@/lib/icons.generated"

// EXP-726 — the desktop table chrome: a hover-only overlay drawn INSIDE the
// `.tiptap-wrapper` (already `position: relative`) so it scrolls with the
// document instead of floating over the viewport. Read-only editors and
// phones never mount it — mobile edits cell content, and its one table
// action (Delete table) sits on the keyboard bar (EXP-727, formatting-rail).
//
// Everything is derived from the cell under the pointer: `+` on the table's
// right and bottom edges append a column/row, a grip above the hovered column
// and one left of the hovered row open the insert/move/delete menus.

const AddIcon = conceptIcon(`ui-add`)
const DeleteIcon = conceptIcon(`ui-delete`)
const MoreIcon = conceptIcon(`ui-more`)
const ChevronUpIcon = conceptIcon(`ui-chevron-up`)
const ChevronDownIcon = conceptIcon(`ui-chevron-down`)
const ChevronLeftIcon = conceptIcon(`ui-chevron-left`)
const ChevronRightIcon = conceptIcon(`ui-chevron-right`)

/** Milliseconds the overlay survives the pointer leaving the table. */
const hoverGraceMs = 150

export interface TableCellCoords {
  row: number
  col: number
  width: number
  height: number
}

export interface TableMenuModel {
  isHeaderRow: boolean
  column: {
    canInsertLeft: boolean
    canInsertRight: boolean
    canMoveLeft: boolean
    canMoveRight: boolean
    canDelete: boolean
  }
  row: {
    canInsertAbove: boolean
    canInsertBelow: boolean
    canMoveUp: boolean
    canMoveDown: boolean
    canDelete: boolean
  }
}

/**
 * Which menu entries a cell offers. GFM pins row 0 as the header row: it can
 * neither move, nor be deleted, nor gain a row above it, and row 1 cannot
 * move up into its slot. Pure so the rules are unit-testable without a DOM.
 */
export function tableMenuModel({
  row,
  col,
  width,
  height,
}: TableCellCoords): TableMenuModel {
  const isHeaderRow = row === 0
  return {
    isHeaderRow,
    column: {
      canInsertLeft: true,
      canInsertRight: true,
      canMoveLeft: col > 0,
      canMoveRight: col < width - 1,
      canDelete: width > 1,
    },
    row: {
      canInsertAbove: !isHeaderRow,
      canInsertBelow: true,
      canMoveUp: row > 1,
      canMoveDown: !isHeaderRow && row < height - 1,
      canDelete: !isHeaderRow,
    },
  }
}

interface OverlayRect {
  left: number
  top: number
  width: number
  height: number
}

interface HoverState extends TableCellCoords {
  /** Position of the table node itself (one before its content start). */
  tablePos: number
  table: OverlayRect
  cell: OverlayRect
  rowRect: OverlayRect
}

function relativeRect(rect: DOMRect, origin: DOMRect): OverlayRect {
  return {
    left: rect.left - origin.left,
    top: rect.top - origin.top,
    width: rect.width,
    height: rect.height,
  }
}

/** Resolve the pointer target to a table cell plus the rects to draw against. */
function resolveHover(
  editor: Editor,
  wrapper: HTMLElement | null,
  target: EventTarget | null
): HoverState | null {
  if (!wrapper || !(target instanceof Element)) return null
  if (editor.isDestroyed || !editor.isEditable) return null
  const cellElement = target.closest(`td, th`)
  const tableElement = cellElement?.closest(`table`)
  const rowElement = cellElement?.closest(`tr`)
  if (!cellElement || !tableElement || !rowElement) return null
  if (!editor.view.dom.contains(cellElement)) return null

  try {
    const pos = editor.view.posAtDOM(cellElement, 0)
    const $cell = cellAround(editor.state.doc.resolve(pos))
    if (!$cell) return null
    const table = $cell.node(-1)
    const tableStart = $cell.start(-1)
    const map = TableMap.get(table)
    const rect = map.findCell($cell.pos - tableStart)
    const origin = wrapper.getBoundingClientRect()
    return {
      tablePos: tableStart - 1,
      row: rect.top,
      col: rect.left,
      width: map.width,
      height: map.height,
      table: relativeRect(tableElement.getBoundingClientRect(), origin),
      cell: relativeRect(cellElement.getBoundingClientRect(), origin),
      rowRect: relativeRect(rowElement.getBoundingClientRect(), origin),
    }
  } catch {
    // posAtDOM throws while the view is mid-update — the next mousemove or
    // the `update` listener recomputes.
    return null
  }
}

export function EditorTableControls({
  editor,
  wrapperRef,
}: {
  editor: Editor
  wrapperRef: React.RefObject<HTMLDivElement | null>
}) {
  const [hover, setHover] = useState<HoverState | null>(null)
  // The dropdowns portal OUTSIDE the overlay, so without this latch the
  // pointer leaving the table would tear the trigger down under the open menu.
  const menuOpenRef = useRef(false)
  const clearTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  // The element the pointer last resolved against, so an editor `update`
  // (a row/column just changed) can recompute the rects without a mouse move.
  const lastTargetRef = useRef<Element | null>(null)

  const cancelClear = useCallback(() => {
    if (clearTimerRef.current !== null) {
      clearTimeout(clearTimerRef.current)
      clearTimerRef.current = null
    }
  }, [])

  const scheduleClear = useCallback(() => {
    if (clearTimerRef.current !== null) return
    clearTimerRef.current = setTimeout(() => {
      clearTimerRef.current = null
      if (menuOpenRef.current) return
      lastTargetRef.current = null
      setHover(null)
    }, hoverGraceMs)
  }, [])

  useEffect(() => {
    if (typeof editor?.on !== `function` || editor.isDestroyed) return
    const dom = editor.view.dom
    let frame = 0
    let pending: Element | null = null

    const apply = (target: Element | null) => {
      const next = resolveHover(editor, wrapperRef.current, target)
      if (next) {
        cancelClear()
        lastTargetRef.current = target
        setHover(next)
      } else {
        scheduleClear()
      }
    }

    // rAF-throttled: every pointer pixel would otherwise run posAtDOM plus
    // three getBoundingClientRect calls.
    const flush = () => {
      frame = 0
      const target = pending
      pending = null
      apply(target)
    }
    const onMove = (event: MouseEvent) => {
      pending = event.target instanceof Element ? event.target : null
      if (frame === 0) frame = requestAnimationFrame(flush)
    }
    const onLeave = () => scheduleClear()
    // A structural edit moves every rect below it.
    const onUpdate = () => {
      if (lastTargetRef.current) apply(lastTargetRef.current)
    }

    dom.addEventListener(`mousemove`, onMove)
    dom.addEventListener(`mouseleave`, onLeave)
    editor.on(`update`, onUpdate)
    return () => {
      if (frame !== 0) cancelAnimationFrame(frame)
      dom.removeEventListener(`mousemove`, onMove)
      dom.removeEventListener(`mouseleave`, onLeave)
      editor.off(`update`, onUpdate)
      cancelClear()
    }
  }, [cancelClear, editor, scheduleClear, wrapperRef])

  useEffect(() => {
    if (editor.isEditable) return
    setHover(null)
  }, [editor.isEditable])

  /**
   * Put the caret in the given cell. Every prosemirror-tables command reads
   * the SELECTION to find its table (`moveTableRow` included), so this has to
   * land before the command runs.
   */
  const selectCell = useCallback(
    (row: number, col: number) => {
      if (!hover || editor.isDestroyed) return false
      const { state } = editor
      const table = state.doc.nodeAt(hover.tablePos)
      if (!table || table.type.name !== `table`) return false
      const map = TableMap.get(table)
      if (row >= map.height || col >= map.width) return false
      const cellPos = hover.tablePos + 1 + map.map[row * map.width + col]
      editor.view.dispatch(
        state.tr.setSelection(
          TextSelection.near(state.doc.resolve(cellPos + 1))
        )
      )
      return true
    },
    [editor, hover]
  )

  const runAt = useCallback(
    (row: number, col: number, run: () => void) => {
      if (!selectCell(row, col)) return
      run()
      editor.view.focus()
    },
    [editor, selectCell]
  )

  const moveColumn = useCallback(
    (from: number, to: number) => {
      runAt(hover?.row ?? 0, from, () => {
        moveTableColumn({
          from,
          to,
          select: false,
          pos: editor.state.selection.from,
        })(editor.state, editor.view.dispatch)
      })
    },
    [editor, hover, runAt]
  )

  const moveRow = useCallback(
    (from: number, to: number) => {
      runAt(from, hover?.col ?? 0, () => {
        moveTableRow({
          from,
          to,
          select: false,
          pos: editor.state.selection.from,
        })(editor.state, editor.view.dispatch)
      })
    },
    [editor, hover, runAt]
  )

  if (!hover) return null

  const model = tableMenuModel(hover)
  const { row, col } = hover
  const onMenuOpenChange = (open: boolean) => {
    menuOpenRef.current = open
    if (open) cancelClear()
    else scheduleClear()
  }

  const deleteTable = () =>
    runAt(row, col, () => {
      editor.chain().deleteTable().run()
    })

  return (
    <div
      // Whitelisted by the dialog hosts' interact-outside guards, like the
      // formatting rail.
      data-editor-rail=""
      className="pointer-events-none absolute inset-0 z-10"
      onMouseEnter={cancelClear}
      onMouseLeave={scheduleClear}
    >
      {/* Append a column on the table's right edge. */}
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        tabIndex={-1}
        aria-label="Add column"
        title="Add column"
        className="editor-table-add"
        style={{
          left: hover.table.left + hover.table.width + 10,
          top: hover.table.top + hover.table.height / 2,
        }}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() =>
          runAt(row, hover.width - 1, () => {
            editor.chain().addColumnAfter().run()
          })
        }
      >
        <AddIcon />
      </Button>

      {/* Append a row on the table's bottom edge. */}
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        tabIndex={-1}
        aria-label="Add row"
        title="Add row"
        className="editor-table-add"
        style={{
          left: hover.table.left + hover.table.width / 2,
          top: hover.table.top + hover.table.height + 10,
        }}
        onMouseDown={(event) => event.preventDefault()}
        onClick={() =>
          runAt(hover.height - 1, col, () => {
            editor.chain().addRowAfter().run()
          })
        }
      >
        <AddIcon />
      </Button>

      {/* Column grip, centred above the hovered column. */}
      <DropdownMenu onOpenChange={onMenuOpenChange}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            tabIndex={-1}
            aria-label={`Column ${col + 1} options`}
            title={`Column ${col + 1} options`}
            className="editor-table-grip is-column"
            style={{
              left: hover.cell.left + hover.cell.width / 2,
              top: hover.table.top - 10,
            }}
            onMouseDown={(event) => event.preventDefault()}
          >
            <MoreIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="center" className="w-52">
          <DropdownMenuItem
            onSelect={() =>
              runAt(row, col, () => {
                editor.chain().addColumnBefore().run()
              })
            }
          >
            <ChevronLeftIcon />
            Insert column left
          </DropdownMenuItem>
          <DropdownMenuItem
            onSelect={() =>
              runAt(row, col, () => {
                editor.chain().addColumnAfter().run()
              })
            }
          >
            <ChevronRightIcon />
            Insert column right
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!model.column.canMoveLeft}
            onSelect={() => moveColumn(col, col - 1)}
          >
            <ChevronLeftIcon />
            Move column left
          </DropdownMenuItem>
          <DropdownMenuItem
            disabled={!model.column.canMoveRight}
            onSelect={() => moveColumn(col, col + 1)}
          >
            <ChevronRightIcon />
            Move column right
          </DropdownMenuItem>
          {/* No separator above a destructive item (EXP-687). */}
          <DropdownMenuItem
            variant="destructive"
            disabled={!model.column.canDelete}
            onSelect={() =>
              runAt(row, col, () => {
                editor.chain().deleteColumn().run()
              })
            }
          >
            <DeleteIcon />
            Delete column
          </DropdownMenuItem>
          <DropdownMenuItem variant="destructive" onSelect={deleteTable}>
            <DeleteIcon />
            Delete table
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

      {/* Row grip, centred left of the hovered row. The header row (row 0)
          must stay the header in GFM, so it offers neither a row above it,
          nor a move, nor its own deletion. */}
      <DropdownMenu onOpenChange={onMenuOpenChange}>
        <DropdownMenuTrigger asChild>
          <Button
            type="button"
            variant="ghost"
            size="icon-xs"
            tabIndex={-1}
            aria-label={`Row ${row + 1} options`}
            title={`Row ${row + 1} options`}
            className="editor-table-grip is-row"
            style={{
              left: hover.table.left - 10,
              top: hover.rowRect.top + hover.rowRect.height / 2,
            }}
            onMouseDown={(event) => event.preventDefault()}
          >
            <MoreIcon />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="center" className="w-52">
          {model.row.canInsertAbove ? (
            <DropdownMenuItem
              onSelect={() =>
                runAt(row, col, () => {
                  editor.chain().addRowBefore().run()
                })
              }
            >
              <ChevronUpIcon />
              Insert row above
            </DropdownMenuItem>
          ) : null}
          <DropdownMenuItem
            onSelect={() =>
              runAt(row, col, () => {
                editor.chain().addRowAfter().run()
              })
            }
          >
            <ChevronDownIcon />
            Insert row below
          </DropdownMenuItem>
          {model.isHeaderRow ? null : (
            <>
              <DropdownMenuItem
                disabled={!model.row.canMoveUp}
                onSelect={() => moveRow(row, row - 1)}
              >
                <ChevronUpIcon />
                Move row up
              </DropdownMenuItem>
              <DropdownMenuItem
                disabled={!model.row.canMoveDown}
                onSelect={() => moveRow(row, row + 1)}
              >
                <ChevronDownIcon />
                Move row down
              </DropdownMenuItem>
              <DropdownMenuItem
                variant="destructive"
                onSelect={() =>
                  runAt(row, col, () => {
                    editor.chain().deleteRow().run()
                  })
                }
              >
                <DeleteIcon />
                Delete row
              </DropdownMenuItem>
            </>
          )}
          <DropdownMenuItem variant="destructive" onSelect={deleteTable}>
            <DeleteIcon />
            Delete table
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  )
}
