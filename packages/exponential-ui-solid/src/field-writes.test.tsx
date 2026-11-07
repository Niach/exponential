// #VAPP-288: interactions-round1.json `fieldWrites` on Solid, like React's
// interactions-fixture.test.tsx: an Input bound to /draft and a Button whose
// press `set`s, on a `createSurface` state (a host write = updateDataModel).
// A programmatic write (an action `set`) wins over a focused field; a host
// write stays under the echo rule.

import { afterEach, describe, expect, it } from "vitest"
import { cleanup, fireEvent, render } from "@solidjs/testing-library"
import { CORE_CATALOG_ID, reduceNested } from "@exponential_app/eui"
import type { NestedNode } from "@exponential_app/eui"
import fixture from "@exponential_app/eui/fixtures/interactions-round1.json"
import { ExponentialSurface } from "./surface"
import { createSurface, type SurfaceState } from "./create-surface"

afterEach(() => cleanup())

interface FieldWriteCase {
  name: string
  seed: string
  pressPath?: string
  pressValue?: unknown
  steps: { op: `focus` | `type` | `hostWrite` | `press` | `blur`; text?: string; path?: string; value?: unknown }[]
  expect: { text: string; data?: unknown }
}

const tree = (c: FieldWriteCase): NestedNode => ({
  id: `root`,
  component: `Stack`,
  children: [
    { id: `draft`, component: `Input`, props: { label: `Draft`, name: `draft`, value: { "@path": `/draft` } } },
    { id: `save`, component: `Button`, props: { label: `Save` }, ...(c.pressPath !== undefined ? { on: { press: { functionCall: { "@call": `set`, args: { path: c.pressPath, value: c.pressValue } } } } } : {}) },
  ],
}) as NestedNode

describe(`fixtures/interactions-round1.json fieldWrites`, () => {
  for (const c of (fixture as unknown as { fieldWrites: FieldWriteCase[] }).fieldWrites) {
    it(`fieldWrites: ${c.name}`, () => {
      let state!: SurfaceState
      const initial = tree(c)
      const { container } = render(() => {
        state = createSurface({ surfaceId: `fw`, data: { draft: c.seed }, initial })
        return <ExponentialSurface surface={state} theme="neutral" id="fw" />
      })
      const input = () => container.querySelector(`[data-xui-id="draft"] input`) as HTMLInputElement
      for (const step of c.steps) {
        if (step.op === `focus`) fireEvent.focus(input())
        else if (step.op === `type`) fireEvent.input(input(), { target: { value: step.text } })
        else if (step.op === `hostWrite`) state.apply({ version: `1.0`, updateDataModel: { surfaceId: `fw`, path: step.path, value: step.value } })
        else if (step.op === `press`) fireEvent.click(container.querySelector(`[data-xui-id="save"]`)!)
        else fireEvent.blur(input())
      }
      expect(input().value).toBe(c.expect.text)
      if (`data` in c.expect) expect((state.data as Record<string, unknown>).draft).toBe(c.expect.data)
    })
    // The same on a surface over local data (a `data` prop, no state).
    if (c.steps.every((s) => s.op !== `hostWrite`))
      it(`fieldWrites (local data): ${c.name}`, () => {
        const root = reduceNested(tree(c), { catalogId: CORE_CATALOG_ID }).root
        const { container } = render(() => <ExponentialSurface root={root} data={{ draft: c.seed }} theme="neutral" id="fwl" />)
        const input = () => container.querySelector(`[data-xui-id="draft"] input`) as HTMLInputElement
        for (const step of c.steps) {
          if (step.op === `focus`) fireEvent.focus(input())
          else if (step.op === `type`) fireEvent.input(input(), { target: { value: step.text } })
          else if (step.op === `press`) fireEvent.click(container.querySelector(`[data-xui-id="save"]`)!)
          else fireEvent.blur(input())
        }
        expect(input().value).toBe(c.expect.text)
      })
  }
})
