// VAPP-279: the strip Tabs reorder in Solid, replaying
// interactions-round1.json `tabsReorder` like React's
// interactions-fixture.test.tsx (jsdom has no layout: each item's rect comes
// from the core-written fixtures/tabs-strip.json `layout` case at 900 px).

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import stripFixture from "@exponential_app/eui/fixtures/tabs-strip.json"
import { ExponentialSurface } from "./surface"
import type { SurfaceActionEvent } from "./host"

afterEach(() => cleanup())

const tree = (node: NestedNode) => reduceNested(node, { catalogId: CORE_CATALOG_ID }).root

interface ReorderStep {
  do: `drag` | `key`
  phase?: `start` | `move` | `end`
  delta?: number
  key?: string
  order?: string[]
}

describe(`fixtures/interactions-round1.json tabsReorder`, () => {
  const labels = [`Inbox`, `Release notes`, `Calendar`, `Settings`, `Search`, `Drafts`]
  const icons: Record<number, string> = { 0: `message-circle`, 1: `pencil` }
  const frames = (stripFixture as unknown as { layout: { cases: Record<string, { expect: { frames: Record<string, number[]> } }> } }).layout.cases[`900`]!.expect.frames
  for (const c of (fixture as unknown as { tabsReorder: { name: string; tab: number; direction?: string; steps: ReorderStep[]; expect: unknown[] }[] }).tabsReorder) {
    it(c.name, () => {
      const rtl = c.direction === `rtl`
      const actions: SurfaceActionEvent[] = []
      const tabs = labels.map((label, i) => ({ label, value: `t${i}`, ...(icons[i] ? { icon: icons[i] } : {}) }))
      const root = tree({ id: `tabs`, component: `Tabs`, props: { variant: `strip`, tabs, value: `t4`, closable: `hover`, addable: true }, on: { reorder: { event: { name: `reorder` } } } } as NestedNode)
      const { container } = render(() => <ExponentialSurface root={root} theme="neutral" id="tr" direction={rtl ? `rtl` : `ltr`} host={{ onAction: (e) => void actions.push(e) }} />)
      const item = (i: number) => container.querySelector(`[data-xui-id="tabs.item.${i}"]`) as HTMLElement
      for (let i = 0; i < labels.length; i++) {
        const [x, y, w, h] = frames[`tabs.item.${i}`]!
        const left = rtl ? 900 - x! - w! : x!
        item(i).getBoundingClientRect = () => ({ left, top: y!, right: left + w!, bottom: y! + h!, width: w!, height: h!, x: left, y: y!, toJSON: () => ({}) }) as DOMRect
      }
      const list = container.querySelector(`[data-xui-id="tabs.list"]`) as HTMLElement
      list.getBoundingClientRect = () => ({ left: 0, top: 0, right: 900, bottom: 32, width: 900, height: 32, x: 0, y: 0, toJSON: () => ({}) }) as DOMRect
      const trigger = container.querySelector(`[data-xui-id="tabs.tab.${c.tab}"]`) as HTMLElement
      const x = 100
      for (const s of c.steps) {
        if (s.do === `key`) {
          trigger.focus()
          fireEvent.keyDown(trigger, { key: s.key, shiftKey: true, ctrlKey: true })
        } else if (s.phase === `start`) {
          fireEvent.pointerDown(trigger, { pointerId: 1, button: 0, pointerType: `mouse`, clientX: x, clientY: 10 })
        } else {
          fireEvent.pointerMove(window, { pointerId: 1, pointerType: `mouse`, clientX: x + s.delta!, clientY: 10 })
          if (s.phase === `end`) fireEvent.pointerUp(window, { pointerId: 1, pointerType: `mouse`, clientX: x + s.delta!, clientY: 10 })
        }
        if (s.order) {
          const order = Array.from(container.querySelectorAll(`[data-xui-part="Tabs/item"]`)).map((el) => (el.getAttribute(`data-xui-id`) ?? ``).replace(`tabs.item.`, `t`))
          expect(order, `${c.name}: order after ${JSON.stringify(s)}`).toEqual(s.order)
        }
      }
      const fired = actions.filter((a) => a.name === `reorder`).map((a) => ({ from: a.context.from, to: a.context.to, value: a.context.value }))
      expect(fired).toEqual(c.expect)
    })
  }
})
