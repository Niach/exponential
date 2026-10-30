// FEED-128 (React's natives/resizable.tsx; round 2, docs/round-2-contract.md
// §1): Resizable — panels (its children) split by handles. The arithmetic
// is the core's (`normalizeSizes`, `resizePanels` from the sizes at the drag
// START, `keyboardResize`, `dragDelta`): sizes are percentages of the main
// axis minus the handles, laid out as flex-grow factors over a 0 basis (=
// `panelExtents`). Each handle is `$control.hairline` thick in layout with a
// `resizeHandleHit` (8 px) hit area centred on it, a focusable `separator`
// (orientation of the line, value = the panel before it, its limits,
// `controls` = that panel, named `$string.resize`). A bound `sizes` is
// written; `change {sizes}` fires after every drag end or key.

import { For, Show, createMemo, createSignal, untrack, type Component, type JSX } from "solid-js"
import { dragDelta, keyboardResize, normalizeSizes, resizePanels, RESIZE_HANDLE_HIT, type PanelLimits, type UiNode } from "@exponential_app/eui"
import { useSurfaceContext } from "../context"
import type { NativeProps } from "../node-view"
import { NodeView } from "../node-view"
import { createBoundState } from "./bound"
import { arr, bool, css, num, parts } from "./shared"

const RESIZE_KEYS = new Set([`ArrowLeft`, `ArrowRight`, `ArrowUp`, `ArrowDown`, `Home`, `End`, `Enter`])

const sameKeys = (a: readonly string[], b: readonly string[]): boolean => a.length === b.length && a.every((x, i) => x === b[i])

export function ResizableNative(props: NativeProps): JSX.Element {
  const ctx = useSurfaceContext()
  const part = parts(props)
  const orientation = (): `horizontal` | `vertical` => (props.props.direction === `vertical` ? `vertical` : `horizontal`)
  const count = () => props.node.children.length
  const limits = createMemo(() => arr<PanelLimits>(props.props.panels))
  const external = createMemo(() => normalizeSizes(arr<unknown>(props.props.sizes), count(), limits()))
  const [sizes, setBound] = createBoundState<number[]>(() => props.node, () => props.scope, `sizes`, external)
  // Live sizes while a drag runs (written + `change` at its end).
  const [live, setLive] = createSignal<number[] | null>(null)
  // The latest drag sizes, for the commit.
  let liveLatest: number[] | null = null
  const shown = () => live() ?? (sizes().length === count() ? sizes() : external())
  const hairline = () => num((ctx.theme.tokens as unknown as { control?: Record<string, number> }).control?.hairline, 1)
  let rootEl: HTMLDivElement | undefined
  let drag: { id: number; handle: number; start: number[]; at: number; container: number } | null = null
  const commit = (next: number[]) => {
    setBound(next)
    void props.emit(`change`, { sizes: next }, { sizes: next })
  }
  const mainOf = (e: { clientX: number; clientY: number }) => (untrack(orientation) === `horizontal` ? e.clientX : e.clientY)
  const onPointerDown = (i: number) => (e: PointerEvent) => {
    if (e.button !== 0) return
    const box = rootEl?.getBoundingClientRect()
    if (!box) return
    e.preventDefault()
    const target = e.currentTarget as HTMLElement
    try {
      target.setPointerCapture?.(e.pointerId)
    } catch {
      // A synthetic or already-released pointer: the drag still follows moves on the handle.
    }
    target.focus()
    drag = { id: e.pointerId, handle: i, start: untrack(shown), at: mainOf(e), container: untrack(orientation) === `horizontal` ? box.width : box.height }
    liveLatest = null
  }
  const onPointerMove = (e: PointerEvent) => {
    const d = drag
    if (!d || d.id !== e.pointerId) return
    const delta = dragDelta(mainOf(e) - d.at, d.container, untrack(count), untrack(orientation), ctx.direction, untrack(hairline))
    const next = resizePanels(d.start, d.handle, delta, untrack(limits))
    liveLatest = next
    setLive(next)
  }
  const onPointerEnd = (e: PointerEvent) => {
    const d = drag
    if (!d || d.id !== e.pointerId) return
    drag = null
    const next = liveLatest
    liveLatest = null
    setLive(null)
    if (next && next.some((v, k) => v !== d.start[k])) commit(next)
  }
  const onKey = (i: number) => (e: KeyboardEvent) => {
    if (!RESIZE_KEYS.has(e.key)) return
    e.preventDefault()
    const cur = untrack(shown)
    const next = keyboardResize(cur, i, e.key, untrack(orientation), ctx.direction, untrack(limits))
    if (next.some((v, k) => v !== cur[k])) commit(next)
  }
  // Panels keyed by child id (React's keys): a re-reduce keeps each subtree.
  const byId = createMemo(() => new Map(props.node.children.map((n) => [n.id, n])))
  const ids = createMemo(() => props.node.children.map((n) => n.id), undefined, { equals: sameKeys })
  return (
    <div ref={rootEl} {...props.rootProps} role="group" data-orientation={orientation()} data-dragging={live() ? `` : undefined}>
      <For each={ids()}>
        {(id, index) => {
          const child = createMemo<UiNode>((prev) => byId().get(id) ?? prev!)
          const size = () => shown()[index()] ?? 0
          const panelId = () => `${props.domId}.panel.${index()}`
          const lim = () => limits()[index()] ?? {}
          const active = () => live() !== null && drag?.handle === index()
          return (
            <>
              <div {...part.at(`panel`, index())} id={panelId()} data-size={size()} style={{ flex: `${size()} 1 0px` }}>
                <NodeView node={child()} />
              </div>
              <Show when={index() < count() - 1}>
                <div
                  {...part.at(`handle`, index(), active() && `pressed`)}
                  role="separator"
                  tabindex={0}
                  aria-orientation={orientation() === `horizontal` ? `vertical` : `horizontal`}
                  aria-valuenow={Math.round(size())}
                  aria-valuemin={typeof lim().min === `number` ? lim().min : undefined}
                  aria-valuemax={typeof lim().max === `number` ? lim().max : undefined}
                  aria-controls={panelId()}
                  aria-label={ctx.t(`resize`)}
                  onPointerDown={(e) => onPointerDown(index())(e)}
                  onPointerMove={onPointerMove}
                  onPointerUp={onPointerEnd}
                  onPointerCancel={onPointerEnd}
                  onKeyDown={(e) => onKey(index())(e)}
                >
                  <span class="xui-resize-hit" aria-hidden="true" style={css(orientation() === `horizontal` ? { width: RESIZE_HANDLE_HIT } : { height: RESIZE_HANDLE_HIT })} />
                  {bool(props.props.handle) ? <span {...part.at(`grip`, index())} aria-hidden="true" /> : null}
                </div>
              </Show>
            </>
          )
        }}
      </For>
    </div>
  )
}

export const NATIVES_RESIZABLE: Record<string, Component<NativeProps>> = {
  Resizable: ResizableNative,
}
