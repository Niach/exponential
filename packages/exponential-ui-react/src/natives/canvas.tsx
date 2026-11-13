// Round 8 (docs/round-8-contract.md §1): Canvas, a thin wrapper over the
// shared web painter (`CanvasView`, @exponential_app/eui/web/motion: compile over
// the WASM CanvasPlayer, theme slots, Canvas2D + native text, pointer). The
// box: `canvasBoxStyle` (Motion's rule); role img named by `label`
// (`canvasA11y`); `pointer: true` fires press/move/leave with `{x, y}` in
// drawing units through the node's `on`.

import { useEffect, useLayoutEffect, useRef } from "react"
import { canvasA11y, canvasBoxStyle, canvasDeclaredSize } from "@exponential_app/eui"
import { CanvasView } from "@exponential_app/eui/web/motion"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"

const CANVAS_STYLE = { display: `block`, width: `100%`, height: `100%` } as const

export function CanvasNative({ node, props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const root = useRef<HTMLDivElement | null>(null)
  const canvas = useRef<HTMLCanvasElement | null>(null)
  const view = useRef<CanvasView | null>(null)
  useLayoutEffect(() => {
    view.current = new CanvasView(root.current!, canvas.current!)
    return () => {
      view.current?.destroy()
      view.current = null
    }
  }, [])
  const host = ctx.host
  const surfaceId = ctx.surfaceId
  useEffect(() => {
    view.current?.update(props, { theme: ctx.theme, mode: ctx.mode, onError: (message) => host.onPaintError?.({ surfaceId, componentId: node.id, message }), emit: (event, payload) => void emit(event, payload) })
  })
  return (
    <div ref={root} {...(rootProps as Record<string, unknown>)} {...canvasA11y(props.label)} style={mergeStyle(rootProps, canvasBoxStyle(canvasDeclaredSize(props), node.style))}>
      <canvas ref={canvas} aria-hidden="true" style={CANVAS_STYLE} />
    </div>
  )
}
