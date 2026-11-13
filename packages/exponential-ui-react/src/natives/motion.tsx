// EXP-1604: Motion, a thin wrapper over the shared web painter
// (`MotionView`, @exponential_app/eui/web/motion: source, slots, clock, Canvas2D).
// The box: the node's style, else the composition's px size shrinking to
// the width (`motionBoxStyle`); the canvas fills it. M4 (EXP-1605): the
// view's events (press, hover, visible, complete, state) reach the node's
// `on` through `emit`; with `on.press` it is a button.

import { useEffect, useLayoutEffect, useRef } from "react"
import { MotionView, motionA11y, motionBoxStyle, motionDeclaredSize } from "@exponential_app/eui/web/motion"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { mergeStyle } from "../node-view"

const CANVAS_STYLE = { display: `block`, width: `100%`, height: `100%` } as const

export function MotionNative({ node, props, rootProps, emit }: NativeProps) {
  const ctx = useSurfaceContext()
  const root = useRef<HTMLDivElement | null>(null)
  const canvas = useRef<HTMLCanvasElement | null>(null)
  const view = useRef<MotionView | null>(null)
  useLayoutEffect(() => {
    view.current = new MotionView(root.current!, canvas.current!)
    return () => {
      view.current?.destroy()
      view.current = null
    }
  }, [])
  const host = ctx.host
  const surfaceId = ctx.surfaceId
  const pressable = Boolean(node.on?.press)
  useEffect(() => {
    view.current?.update(props, { theme: ctx.theme, mode: ctx.mode, reducedMotion: ctx.reducedMotion, host, onError: (message) => host.onPaintError?.({ surfaceId, componentId: node.id, message }), emit: (event, payload) => void emit(event, payload), pressable }, node.style)
  })
  return (
    <div ref={root} {...(rootProps as Record<string, unknown>)} {...motionA11y(props.label, pressable)} style={mergeStyle(rootProps, motionBoxStyle(motionDeclaredSize(props), node.style))}>
      <canvas ref={canvas} aria-hidden="true" style={CANVAS_STYLE} />
    </div>
  )
}
