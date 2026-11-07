// Round 9 §6.8: an action with both: the function first, its event only
// when the function did not fail.

import { describe, expect, it } from "vitest"
import { runNodeAction } from "./node-view"
import type { SurfaceContextValue } from "./context"
import type { UiNode } from "@exponential_app/eui"

const node = { id: `b`, component: `Button`, props: {}, children: [], on: { press: { functionCall: { "@call": `lookup` }, event: { name: `done` } } } } as unknown as UiNode

async function press(outcome: unknown): Promise<string[]> {
  const order: string[] = []
  const ctx = {
    surfaceId: `s`,
    data: {},
    functions: {},
    host: {
      onFunctionCall: async () => {
        order.push(`function`)
        return outcome
      },
      onAction: (e: { name: string }) => void order.push(e.name),
    },
  } as unknown as SurfaceContextValue
  await runNodeAction(ctx, node, `b`, ``, `press`)
  return order
}

describe(`function, then event`, () => {
  it(`a success fires the event after the function`, async () => {
    expect(await press({ decision: `allow` })).toEqual([`function`, `done`])
  })
  it(`a failure, a deny or no function stops the event`, async () => {
    for (const outcome of [{ decision: `allow`, error: `boom` }, { decision: `deny` }, { decision: `not_found` }]) expect(await press(outcome)).toEqual([`function`])
  })
})
