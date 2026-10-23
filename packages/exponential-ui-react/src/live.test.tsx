// VAPP-141 + VAPP-137: a change re-renders only the nodes that read it
// (`live.ts`): a data write the nodes whose bindings it touches, a streamed
// `updateComponents` the nodes it changed, a breakpoint the nodes with a
// responsive prop.

import { afterEach, describe, expect, it } from "vitest"
import { act, cleanup, render } from "@testing-library/react"
import { useEffect } from "react"
import type { FlatComponent } from "@exponential-at/ui"
import { ExponentialSurface } from "./surface"
import { useSurface, type SurfaceState } from "./use-surface"
import { NATIVES } from "./natives"
import type { NativeProps } from "./node-view"
import { share } from "./live"

afterEach(cleanup)

/** Every Text and Badge paint, by node id. */
function counting() {
  const painted: string[] = []
  const wrap = (name: string) => {
    const real = NATIVES[name]
    NATIVES[name] = (p: NativeProps) => {
      painted.push(p.domId)
      return real(p)
    }
    return () => {
      NATIVES[name] = real
    }
  }
  const restore = [wrap(`Text`), wrap(`Badge`)]
  return { painted, restore: () => restore.forEach((r) => r()) }
}

const BASE: FlatComponent[] = [
  { id: `root`, component: `Box`, children: [`label`, `transcript`] },
  { id: `label`, component: `Text`, text: { path: `/status/label` } },
  { id: `transcript`, component: `Box`, children: [`m0`] },
  { id: `m0`, component: `Text`, text: `first` },
] as unknown as FlatComponent[]

function Streamed({ onState }: { onState: (s: SurfaceState) => void }) {
  const surface = useSurface({ surfaceId: `s`, data: { status: { label: `a` } } })
  useEffect(() => {
    onState(surface)
  })
  useEffect(() => {
    surface.apply({ updateComponents: { surfaceId: `s`, components: BASE } })
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])
  return <ExponentialSurface surface={surface} theme="neutral" id="s" />
}

describe(`live re-renders`, () => {
  it(`a data write repaints only the node bound to it`, () => {
    const c = counting()
    try {
      let state!: SurfaceState
      const { container } = render(<Streamed onState={(s) => (state = s)} />)
      expect(container.querySelector(`[data-xui-id="label"]`)?.textContent).toBe(`a`)
      c.painted.length = 0
      act(() => state.apply({ updateDataModel: { surfaceId: `s`, path: `/status`, value: { label: `b` } } }))
      expect(container.querySelector(`[data-xui-id="label"]`)?.textContent).toBe(`b`)
      expect(c.painted).toEqual([`label`])
    } finally {
      c.restore()
    }
  })
  it(`a streamed updateComponents repaints only the new node`, () => {
    const c = counting()
    try {
      let state!: SurfaceState
      const { container } = render(<Streamed onState={(s) => (state = s)} />)
      c.painted.length = 0
      act(() =>
        state.apply({
          updateComponents: {
            surfaceId: `s`,
            components: [{ id: `transcript`, component: `Box`, children: [`m0`, `m1`] }, { id: `m1`, component: `Text`, text: `second` }] as unknown as FlatComponent[],
          },
        })
      )
      expect(container.querySelector(`[data-xui-id="m1"]`)?.textContent).toBe(`second`)
      expect(c.painted).toEqual([`m1`])
      c.painted.length = 0
      act(() => state.apply({ updateComponents: { surfaceId: `s`, components: [{ id: `m0`, component: `Text`, text: `changed` }] as unknown as FlatComponent[] } }))
      expect(container.querySelector(`[data-xui-id="m0"]`)?.textContent).toBe(`changed`)
      expect(c.painted).toEqual([`m0`])
    } finally {
      c.restore()
    }
  })
  it(`a literal Table row reads the live model`, () => {
    const root = {
      id: `t`,
      component: `Table`,
      props: { columns: [{ key: `name`, label: `Name`, type: `slot`, slot: `cell` }], rows: [{ name: `x` }] },
      children: [],
      slots: { cell: { id: `cell`, component: `Text`, props: { text: { path: `/greeting` } }, children: [] } },
    }
    const { container, rerender } = render(<ExponentialSurface root={root as never} data={{ greeting: `hi` }} theme="neutral" id="lt" />)
    expect(container.querySelector(`[data-xui-c="Text"]`)?.textContent).toBe(`hi`)
    rerender(<ExponentialSurface root={root as never} data={{ greeting: `ho` }} theme="neutral" id="lt" />)
    expect(container.querySelector(`[data-xui-c="Text"]`)?.textContent).toBe(`ho`)
  })
})

describe(`share`, () => {
  it(`keeps every unchanged part of the old value`, () => {
    const prev = { id: `r`, children: [{ id: `a`, props: { x: 1 } }, { id: `b`, props: { x: 2 } }] }
    const next = { id: `r`, children: [{ id: `b`, props: { x: 2 } }, { id: `a`, props: { x: 3 } }, { id: `c`, props: {} }] }
    const out = share(prev, next)
    expect(out).toEqual(next)
    expect(out.children[0]).toBe(prev.children[1])
    expect(out.children[1]).not.toBe(prev.children[0])
    expect(share(prev, structuredClone(prev))).toBe(prev)
  })
})
