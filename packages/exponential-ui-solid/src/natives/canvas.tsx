// Round 8 (React's natives/canvas.tsx): Canvas, a thin wrapper over the
// shared web painter (`CanvasView`, @exponential_app/eui/web/motion: compile over
// the WASM CanvasPlayer, theme slots, Canvas2D + native text, pointer). The
// box: `canvasBoxStyle` (Motion's rule); role img named by `label`
// (`canvasA11y`); `pointer: true` fires press/move/leave with `{x, y}` in
// drawing units through the node's `on`.

import { createEffect, onCleanup, onMount, type Component, type JSX } from "solid-js"
import { canvasA11y, canvasBoxStyle, canvasDeclaredSize } from "@exponential_app/eui"
import { CanvasView } from "@exponential_app/eui/web/motion"
import { useSurfaceContext } from "../context"
import { mergeStyle, type NativeProps } from "../node-view"

const CANVAS_STYLE = { display: `block`, width: `100%`, height: `100%` } as const

export function CanvasNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  let root!: HTMLDivElement
  let canvas!: HTMLCanvasElement
  let view: CanvasView | null = null
  onMount(() => {
    view = new CanvasView(root, canvas)
    createEffect(() => {
      const host = ctx.host
      const surfaceId = ctx.surfaceId
      const id = props.node.id
      const emit = props.emit
      view?.update(props.props, { theme: ctx.theme, mode: ctx.mode, onError: (message) => host.onPaintError?.({ surfaceId, componentId: id, message }), emit: (event, payload) => void emit(event, payload) })
    })
  })
  onCleanup(() => {
    view?.destroy()
    view = null
  })
  return (
    <div ref={root} {...props.rootProps} {...canvasA11y(props.props.label)} style={mergeStyle(props.rootProps, canvasBoxStyle(canvasDeclaredSize(props.props), props.node.style))}>
      <canvas ref={canvas} aria-hidden="true" style={CANVAS_STYLE} />
    </div>
  )
}

export const NATIVES_CANVAS: Record<string, Component<NativeProps>> = {
  Canvas: CanvasNative,
}
