// FEED-130: `window.__xuiVList(case)` for the element harness. The one probe
// that is NOT a surface: it mounts the bare Solid `WindowedList` (the
// renderer primitive the element paints its lists with) in a scroller of
// `viewport` px, exactly as the Solid harness does (harness/main.tsx there).

import { render } from "solid-js/web"
import { WindowedList } from "@exponential_app/eui-solid"
import type { WindowedListHandle } from "@exponential_app/eui-solid"

export interface VListCase {
  extents: number[]
  gap: number
  viewport: number
  overscan?: number
  axis?: `vertical` | `horizontal`
  stickyRows?: number[]
}
export interface VListProbe {
  scroll: (at: number) => Promise<{ start: number; end: number; before: number; after: number; total: number; pinned: { row: number; offset: number } | null }>
  scrollToIndex: (index: number, align: `start` | `center` | `end` | `nearest`) => Promise<number>
}

const px = (v: number) => `${v}px`
const frames = (n = 2) =>
  new Promise<void>((resolve) => {
    const step = (left: number) => (left <= 0 ? resolve() : requestAnimationFrame(() => step(left - 1)))
    step(n)
  })

/** Paints the case into `el` (a fresh container); returns the probe and the
 *  Solid disposer. */
export async function mountVList(el: HTMLElement, c: VListCase): Promise<{ probe: VListProbe; dispose: () => void }> {
  const horizontal = c.axis === `horizontal`
  const main = horizontal ? `width` : `height`
  const cross = horizontal ? `height` : `width`
  let list: WindowedListHandle | null = null
  const estimate = (i: number) => c.extents[i]
  // Built outside the JSX: the Solid compiler drops computed keys of an
  // inline `style={{…}}` literal.
  const box = (m: string, x: string, extra: Record<string, string> = {}): Record<string, string> => ({ ...extra, [main]: m, [cross]: x })
  const scrollerStyle = box(px(c.viewport), `200px`, { overflow: `auto`, display: horizontal ? `flex` : `block` })
  const padStyle = box(px(c.viewport), `1px`, { flex: `none` })
  const dispose = render(
    () => (
      <div id="vl-scroller" style={scrollerStyle}>
        <WindowedList ref={(h) => (list = h)} count={c.extents.length} itemKey={(i) => String(i)} estimatedItemHeight={estimate} overscan={c.overscan} axis={c.axis} gap={c.gap} stickyRows={c.stickyRows} renderItem={(i) => <div style={box(px(c.extents[i()]), `20px`)} />} />
        <div style={padStyle} />
      </div>
    ),
    el
  )
  await frames(3)
  const scroller = el.querySelector<HTMLElement>(`#vl-scroller`)!
  const win = () => scroller.querySelector<HTMLElement>(`.xui-list-window`)!
  const pos = (item: HTMLElement) => (horizontal ? item.getBoundingClientRect().left - win().getBoundingClientRect().left : item.getBoundingClientRect().top - win().getBoundingClientRect().top)
  const offset = () => (horizontal ? scroller.scrollLeft : scroller.scrollTop)
  const probe: VListProbe = {
    scroll: async (at) => {
      if (horizontal) scroller.scrollLeft = at
      else scroller.scrollTop = at
      await frames(3)
      const w = win()
      const start = Number(w.dataset.windowStart)
      const end = Number(w.dataset.windowEnd)
      const item = (i: number) => w.querySelector<HTMLElement>(`:scope > .xui-list-item[data-index="${i}"]`)!
      const painted = Number.parseFloat(w.style[main])
      const span = end > start ? { before: pos(item(start)), after: painted - (pos(item(end - 1)) + c.extents[end - 1]) } : { before: 0, after: painted }
      const pin = Array.from(w.querySelectorAll<HTMLElement>(`:scope > .xui-list-item`)).find((x) => x.style.zIndex === `1`)
      return { start, end, ...span, total: painted, pinned: pin ? { row: Number(pin.dataset.index), offset: pos(pin) } : null }
    },
    scrollToIndex: async (index, align) => {
      ;(list as WindowedListHandle | null)!.scrollToIndex(index, align)
      await frames(8)
      return offset()
    },
  }
  return { probe, dispose }
}
