// VAPP-87: HOST-OWNED inputs on React. The rules (revision counter,
// debounce, echo acceptance: a host push is adopted only when the field is
// not focused AND every revision was acknowledged, the VAPP-4 typing rule;
// #VAPP-288: or when it comes with a moved write epoch, a programmatic
// write such as an action `set`, which wins even while focused) are the web layer's `HostOwnedController` (`@exponential_app/eui/web`
// inputs.ts, FEED-128); this hook keeps one per field and re-renders on its
// changes.

import { useCallback, useEffect, useReducer, useRef, useSyncExternalStore } from "react"
import { DEBOUNCE_MS, HostOwnedController } from "@exponential_app/eui/web"

export { DEBOUNCE_MS }

export interface HostOwnedOptions<T> {
  /** The value the host currently holds (resolved prop or data model). */
  external: T
  /** Sends an edit; a returned promise acknowledges the revision on settle. */
  send: (value: T, revision: number, kind: `change` | `commit`) => void | Promise<void>
  /** Optional local write-through (the bound data model path). */
  writeLocal?: (value: T) => void
  debounceMs?: number
  /** #VAPP-288: the surface's write epoch (`ctx.writeEpoch`, `useWriteEpoch`);
   *  a push with a moved epoch is a programmatic write. */
  epoch?: number
}

export interface HostOwnedValue<T> {
  value: T
  revision: number
  /** True while a revision the host has not acknowledged exists. */
  pending: boolean
  focused: boolean
  onFocus: () => void
  onBlur: () => void
  /** A local edit. */
  edit: (next: T) => void
  /** Flush the debounce and send a `commit`. */
  commit: () => void
}

export function useHostOwnedValue<T>({ external, send, writeLocal, debounceMs = DEBOUNCE_MS, epoch }: HostOwnedOptions<T>): HostOwnedValue<T> {
  const [, rerender] = useReducer((n: number) => n + 1, 0)
  const ref = useRef<HostOwnedController<T> | null>(null)
  if (!ref.current) ref.current = new HostOwnedController<T>({ initial: external, send, writeLocal, debounceMs, onChange: rerender, epoch })
  const c = ref.current
  c.configure({ send, writeLocal, debounceMs })
  useEffect(() => {
    c.attach()
    return () => c.dispose()
  }, [c])
  // The host's push: adopted only when idle and fully acknowledged, or
  // with a moved write epoch (a programmatic write).
  useEffect(() => {
    c.push(external, epoch)
  }, [c, external, epoch])
  const onFocus = useCallback(() => c.focus(), [c])
  const onBlur = useCallback(() => c.blur(), [c])
  const edit = useCallback((next: T) => c.edit(next), [c])
  const commit = useCallback(() => c.commit(), [c])
  return { value: c.value, revision: c.revision, pending: c.pending, focused: c.focused, onFocus, onBlur, edit, commit }
}

/** #VAPP-288: the surface's write epoch as React state (re-renders the
 *  field on every programmatic write, even one to another path, so the
 *  field records it and a later host write stays under the echo rule). */
export function useWriteEpoch(ctx: { subscribe: (listener: () => void) => () => void; writeEpoch: number }): number {
  const read = () => ctx.writeEpoch
  return useSyncExternalStore(ctx.subscribe, read, read)
}
