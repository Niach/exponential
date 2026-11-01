// EXP-1604 (React's natives/motion.tsx): Motion, a thin wrapper over the
// shared web painter (`MotionView`, @exponential_app/eui/web: source, slots,
// clock, Canvas2D). The box: the node's style, else the composition's px
// size shrinking to the width (`motionBoxStyle`); the canvas fills it.

import { createEffect, onCleanup, onMount, type Component, type JSX } from "solid-js"
import { MotionView, motionA11y, motionBoxStyle, motionDeclaredSize } from "@exponential_app/eui/web"
import { useSurfaceContext } from "../context"
import { mergeStyle, type NativeProps } from "../node-view"

const CANVAS_STYLE = { display: `block`, width: `100%`, height: `100%` } as const

export function MotionNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  let root!: HTMLDivElement
  let canvas!: HTMLCanvasElement
  let view: MotionView | null = null
  onMount(() => {
    view = new MotionView(root, canvas)
    createEffect(() => {
      const host = ctx.host
      const surfaceId = ctx.surfaceId
      const id = props.node.id
      view?.update(props.props, { theme: ctx.theme, mode: ctx.mode, reducedMotion: ctx.reducedMotion, host, onError: (message) => host.onPaintError?.({ surfaceId, componentId: id, message }) }, props.node.style)
    })
  })
  onCleanup(() => {
    view?.destroy()
    view = null
  })
  return (
    <div ref={root} {...props.rootProps} {...motionA11y(props.props.label)} style={mergeStyle(props.rootProps, motionBoxStyle(motionDeclaredSize(props.props), props.node.style))}>
      <canvas ref={canvas} aria-hidden="true" style={CANVAS_STYLE} />
    </div>
  )
}

export const NATIVES_MOTION: Record<string, Component<NativeProps>> = {
  Motion: MotionNative,
}
