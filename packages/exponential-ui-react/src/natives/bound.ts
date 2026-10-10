// Round 1 (contract §1, "Natives with bindable state"): when a native's
// state prop is a DataBinding, the renderer writes the user's change to that
// path itself AND fires `change`. `useBoundState` is that rule for every
// controlled native: a local mirror (a press shows at once), the host's /
// data model's value wins whenever it changes, the write goes to the bound
// path (relative paths against the node's scope).

import { useCallback, useEffect, useRef, useState } from "react"
import { useSurfaceContext } from "../context"
import { boundPath } from "../data"
import type { NativeProps } from "../node-view"

export function useBoundState<T>(node: NativeProps[`node`], scope: string, prop: string, external: T, key: (v: T) => string = (v) => JSON.stringify(v) ?? ``): [T, (next: T, written?: unknown) => void, string | undefined] {
  const ctx = useSurfaceContext()
  const path = boundPath(node.props, prop, scope)
  const [value, setValue] = useState<T>(external)
  const externalKey = key(external)
  const last = useRef(externalKey)
  useEffect(() => {
    if (last.current === externalKey) return
    last.current = externalKey
    setValue(external)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [externalKey])
  const set = useCallback(
    // `written` = the value the bound path receives when it differs from
    // the local shape (a multiple Select bound to a string: the list shows,
    // the comma-joined string is written).
    (next: T, written: unknown = next) => {
      setValue(next)
      last.current = key(next)
      if (path) ctx.setData(path, written)
    },
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [path, ctx.setData]
  )
  return [value, set, path]
}
