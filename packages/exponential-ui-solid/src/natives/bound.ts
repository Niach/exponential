// FEED-128 (React's natives/bound.ts): when a native's state prop is a
// DataBinding, the renderer writes the user's change to that path itself
// AND fires `change`. `createBoundState` is that rule for every controlled
// native: a local mirror (a press shows at once), the host's / data
// model's value wins whenever it changes (by `key`), the write goes to the
// bound path (relative paths against the node's scope).

import { createComputed, createMemo, createSignal, on, untrack, type Accessor } from "solid-js"
import { boundPath } from "@exponential-at/ui/web"
import type { UiNode } from "@exponential-at/ui"
import { useSurfaceContext } from "../context"

export function createBoundState<T>(node: Accessor<UiNode>, scope: Accessor<string>, prop: string, external: Accessor<T>, key: (v: T) => string = (v) => JSON.stringify(v) ?? ``): [Accessor<T>, (next: T, written?: unknown) => void, Accessor<string | undefined>] {
  const ctx = useSurfaceContext()
  const path = createMemo(() => boundPath(node().props, prop, scope()))
  const [value, setValue] = createSignal<T>(untrack(external))
  const externalKey = createMemo(() => key(external()))
  let last = untrack(externalKey)
  createComputed(
    on(
      externalKey,
      (k) => {
        if (last === k) return
        last = k
        setValue(() => untrack(external))
      },
      { defer: true }
    )
  )
  // `written` = the value the bound path receives when it differs from the
  // local shape (a multiple Select bound to a string).
  const set = (next: T, written: unknown = next) => {
    setValue(() => next)
    last = key(next)
    const p = untrack(path)
    if (p) ctx.setData(p, written)
  }
  return [value, set, path]
}
