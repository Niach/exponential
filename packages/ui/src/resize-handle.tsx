import * as React from "react"

import { cn } from "./cn"

// EXP-1156: the grab strip on a resizable column's RIGHT edge — the web half
// of the desktop's `resize_edge.rs`. It owns the gesture and nothing else:
// the caller places it (`absolute inset-y-0` is all it sets), sizes it and
// keeps the width, so the same strip serves the team sidebar and any future
// column. A pointer drag follows `startWidth + (clientX - startX)`, ←/→ step
// it, Home/End jump to the bounds, a double-click resets it.

export interface ResizeHandleProps
  extends Omit<
    React.ComponentProps<`div`>,
    `onChange` | `aria-valuenow` | `aria-valuemin` | `aria-valuemax`
  > {
  /** The column's current width, in the caller's units. */
  value: number
  min: number
  max: number
  /** What one arrow key moves the width; Shift moves it four times as far. */
  step?: number
  /**
   * CSS pixels per unit of `value`. 1 for a px column; the web app's columns
   * are authored in 16px design units rendered as rem, so it passes the root
   * font's px over 16 and a drag still tracks the pointer exactly.
   */
  scale?: number
  /** Every intermediate width — a drag calls it on each pointer move. */
  onChange: (width: number) => void
  /** The settled width: the end of a drag, or each keyboard step. */
  onCommit?: (width: number) => void
  /** Double-click: back to the column's default. */
  onReset?: () => void
  onDraggingChange?: (dragging: boolean) => void
  /** Forces the hairline on (a resting specimen of the dragged look). */
  active?: boolean
  "aria-label": string
}

interface DragState {
  pointerId: number
  startX: number
  startWidth: number
  latest: number
  cursor: string
  userSelect: string
}

function clamp(width: number, min: number, max: number): number {
  return Math.round(Math.min(Math.max(width, min), Math.max(min, max)))
}

function ResizeHandle({
  value,
  min,
  max,
  step = 16,
  scale = 1,
  onChange,
  onCommit,
  onReset,
  onDraggingChange,
  active = false,
  className,
  ...props
}: ResizeHandleProps) {
  const dragRef = React.useRef<DragState | null>(null)
  const [dragging, setDragging] = React.useState(false)
  // Latest callback through a ref: an inline caller arrow must not re-run the
  // unmount cleanup below (which would cut every drag short on re-render).
  const draggingChangeRef = React.useRef(onDraggingChange)
  draggingChangeRef.current = onDraggingChange

  // The body carries the cursor and the selection lock for the drag's
  // duration: the pointer leaves the 8px strip at once, and without these
  // the cursor flickers back to an arrow and the drag selects page text.
  const release = React.useCallback(() => {
    const drag = dragRef.current
    if (!drag) return null
    dragRef.current = null
    document.body.style.cursor = drag.cursor
    document.body.style.userSelect = drag.userSelect
    setDragging(false)
    draggingChangeRef.current?.(false)
    return drag
  }, [])

  // A drag cut short by an unmount must not leave the body locked.
  React.useEffect(() => () => void release(), [release])

  const begin = (event: React.PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return
    event.preventDefault()
    event.currentTarget.setPointerCapture(event.pointerId)
    dragRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startWidth: value,
      latest: value,
      cursor: document.body.style.cursor,
      userSelect: document.body.style.userSelect,
    }
    document.body.style.cursor = `col-resize`
    document.body.style.userSelect = `none`
    setDragging(true)
    draggingChangeRef.current?.(true)
  }

  const move = (event: React.PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current
    if (!drag || drag.pointerId !== event.pointerId) return
    event.preventDefault()
    const next = clamp(
      drag.startWidth + (event.clientX - drag.startX) / scale,
      min,
      max
    )
    if (next === drag.latest) return
    drag.latest = next
    onChange(next)
  }

  const end = (event: React.PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return
    const drag = release()
    if (drag && drag.latest !== drag.startWidth) onCommit?.(drag.latest)
  }

  const keyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const by = event.shiftKey ? step * 4 : step
    let next: number | null = null
    if (event.key === `ArrowLeft`) next = value - by
    else if (event.key === `ArrowRight`) next = value + by
    else if (event.key === `Home`) next = min
    else if (event.key === `End`) next = max
    if (next === null) return
    event.preventDefault()
    const width = clamp(next, min, max)
    if (width === value) return
    onChange(width)
    onCommit?.(width)
  }

  const shown = dragging || active

  return (
    <div
      role="separator"
      aria-orientation="vertical"
      aria-valuenow={Math.round(value)}
      aria-valuemin={Math.round(min)}
      aria-valuemax={Math.round(Math.max(min, max))}
      tabIndex={0}
      data-slot="resize-handle"
      data-dragging={shown ? `true` : undefined}
      onPointerDown={begin}
      onPointerMove={move}
      onPointerUp={end}
      onPointerCancel={end}
      onLostPointerCapture={end}
      onKeyDown={keyDown}
      onDoubleClick={onReset}
      className={cn(
        `group/resize-handle absolute inset-y-0 cursor-col-resize touch-none outline-none`,
        className
      )}
      {...props}
    >
      {/* The 1px hairline, centred in the strip: hidden at rest, the glass
          stroke on hover, the focus ring's colour from the keyboard or
          while dragging. */}
      <span
        aria-hidden
        className={cn(
          `pointer-events-none absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-transparent transition-colors duration-fast ease-standard motion-reduce:transition-none`,
          `group-hover/resize-handle:bg-glass-stroke-active group-focus-visible/resize-handle:bg-ring`,
          shown && `bg-ring group-hover/resize-handle:bg-ring`
        )}
      />
    </div>
  )
}

export { ResizeHandle }
