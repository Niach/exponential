// Round 2 (docs/round-2-contract.md §1): Resizable — panels (its children)
// split by handles. The arithmetic is the core's (`normalizeSizes`,
// `resizePanels` from the sizes at the drag START, `keyboardResize`,
// `dragDelta`): sizes are percentages of the main axis minus the handles,
// laid out as flex-grow factors over a 0 basis (= `panelExtents`). Each
// handle is `$control.hairline` thick in layout with a `resizeHandleHit` (8
// px) hit area centred on it, a focusable `separator` (orientation of the
// line, value = the panel before it, its limits, `controls` = that panel,
// named `$string.resize`). A bound `sizes` is written; `change {sizes}`
// fires after every drag end or key.

import { useMemo, useRef, useState, type KeyboardEvent, type PointerEvent as ReactPointerEvent } from "react"
import { dragDelta, keyboardResize, normalizeSizes, resizePanels, RESIZE_HANDLE_HIT, type PanelLimits } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { useBoundState } from "./bound"
import { arr, bool, num, useParts } from "./shared"

const RESIZE_KEYS = new Set([`ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`, `Home`, `End`, `Enter`])

export function ResizableNative({ node, props, rootProps, emit, scope, domId }: NativeProps) {
  const ctx = useSurfaceContext()
  const part = useParts(node, props)
  const orientation: `horizontal` | `vertical` = props.direction === `vertical` ? `vertical` : `horizontal`
  const count = node.children.length
  const limits = useMemo(() => arr<PanelLimits>(props.panels), [props.panels])
  const external = useMemo(() => normalizeSizes(arr<unknown>(props.sizes), count, limits), [props.sizes, count, limits])
  const [sizes, setBound] = useBoundState<number[]>(node, scope, `sizes`, external)
  // Live sizes while a drag runs (written + `change` at its end).
  const [live, setLive] = useState<number[] | null>(null)
  // The latest drag sizes, for the commit: a pointerup can land before the
  // last move's render, so the render-time `live` may be one step behind.
  const liveRef = useRef<number[] | null>(null)
  const shown = live ?? (sizes.length === count ? sizes : external)
  const hairline = num((ctx.theme.tokens as unknown as { control?: Record<string, number> }).control?.hairline, 1)
  const rootRef = useRef<HTMLDivElement | null>(null)
  const drag = useRef<{ id: number; handle: number; start: number[]; at: number; container: number } | null>(null)
  const commit = (next: number[]) => {
    setBound(next)
    void emit(`change`, { sizes: next })
  }
  const mainOf = (e: { clientX: number; clientY: number }) => (orientation === `horizontal` ? e.clientX : e.clientY)
  const onPointerDown = (i: number) => (e: ReactPointerEvent<HTMLDivElement>) => {
    if (e.button !== 0) return
    const box = rootRef.current?.getBoundingClientRect()
    if (!box) return
    e.preventDefault()
    try {
      ;(e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId)
    } catch {
      // A synthetic or already-released pointer: the drag still follows moves on the handle.
    }
    ;(e.currentTarget as HTMLElement).focus()
    drag.current = { id: e.pointerId, handle: i, start: shown, at: mainOf(e), container: orientation === `horizontal` ? box.width : box.height }
    liveRef.current = null
  }
  const onPointerMove = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d || d.id !== e.pointerId) return
    const delta = dragDelta(mainOf(e) - d.at, d.container, count, orientation, ctx.direction, hairline)
    const next = resizePanels(d.start, d.handle, delta, limits)
    liveRef.current = next
    setLive(next)
  }
  const onPointerEnd = (e: ReactPointerEvent<HTMLDivElement>) => {
    const d = drag.current
    if (!d || d.id !== e.pointerId) return
    drag.current = null
    const next = liveRef.current
    liveRef.current = null
    setLive(null)
    if (next && next.some((v, k) => v !== d.start[k])) commit(next)
  }
  const onKey = (i: number) => (e: KeyboardEvent<HTMLDivElement>) => {
    if (!RESIZE_KEYS.has(e.key)) return
    e.preventDefault()
    const next = keyboardResize(shown, i, e.key, orientation, ctx.direction, limits)
    if (next.some((v, k) => v !== shown[k])) commit(next)
  }
  const parts = []
  for (let i = 0; i < count; i++) {
    const size = shown[i] ?? 0
    parts.push(
      <div key={node.children[i].id} {...(part.at(`panel`, i) as Record<string, string>)} id={`${domId}.panel.${i}`} data-size={size} style={{ flex: `${size} 1 0px` }}>
        <NodeView node={node.children[i]} />
      </div>
    )
    if (i < count - 1) {
      const lim = limits[i] ?? {}
      const active = live !== null && drag.current?.handle === i
      parts.push(
        <div
          key={`h${i}`}
          {...(part.at(`handle`, i, active && `pressed`) as Record<string, string>)}
          role="separator"
          tabIndex={0}
          aria-orientation={orientation === `horizontal` ? `vertical` : `horizontal`}
          aria-valuenow={Math.round(size)}
          aria-valuemin={typeof lim.min === `number` ? lim.min : undefined}
          aria-valuemax={typeof lim.max === `number` ? lim.max : undefined}
          aria-controls={`${domId}.panel.${i}`}
          aria-label={ctx.t(`resize`)}
          onPointerDown={onPointerDown(i)}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerEnd}
          onPointerCancel={onPointerEnd}
          onKeyDown={onKey(i)}
        >
          <span className="xui-resize-hit" aria-hidden="true" style={{ [orientation === `horizontal` ? `width` : `height`]: RESIZE_HANDLE_HIT }} />
          {bool(props.handle) ? <span {...(part.at(`grip`, i) as Record<string, string>)} aria-hidden="true" /> : null}
        </div>
      )
    }
  }
  return (
    <div ref={rootRef} {...(rootProps as Record<string, unknown>)} role="group" data-orientation={orientation} data-dragging={live ? `` : undefined}>
      {parts}
    </div>
  )
}
