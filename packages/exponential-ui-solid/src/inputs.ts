// FEED-128: HOST-OWNED inputs on Solid (React's inputs.ts). The rules
// (revision counter, debounce, echo acceptance: a host push is adopted only
// when the field is not focused AND every revision was acknowledged, the
// VAPP-4 typing rule) are the web layer's `HostOwnedController`; this keeps
// one per field and exposes its state as live getters.

import { createEffect, createSignal, on, onCleanup, untrack, type Accessor } from "solid-js"
import { DEBOUNCE_MS, HostOwnedController } from "@exponential_app/ui/web"

export { DEBOUNCE_MS }

export interface HostOwnedOptions<T> {
  /** The value the host currently holds (resolved prop or data model). */
  external: Accessor<T>
  /** Sends an edit; a returned promise acknowledges the revision on settle.
   *  Read at call time (a getter on the options object is followed). */
  send: (value: T, revision: number, kind: `change` | `commit`) => void | Promise<void>
  /** Optional local write-through (the bound data model path). */
  writeLocal?: (value: T) => void
  debounceMs?: number
}

export interface HostOwnedValue<T> {
  readonly value: T
  readonly revision: number
  /** True while a revision the host has not acknowledged exists. */
  readonly pending: boolean
  readonly focused: boolean
  onFocus: () => void
  onBlur: () => void
  /** A local edit. */
  edit: (next: T) => void
  /** Flush the debounce and send a `commit`. */
  commit: () => void
}

export function createHostOwnedValue<T>(options: HostOwnedOptions<T>): HostOwnedValue<T> {
  const [version, bump] = createSignal(0, { equals: false })
  const c = new HostOwnedController<T>({
    initial: untrack(options.external),
    send: (value, revision, kind) => options.send(value, revision, kind),
    writeLocal: (value) => options.writeLocal?.(value),
    debounceMs: options.debounceMs ?? DEBOUNCE_MS,
    onChange: () => bump(0),
  })
  c.attach()
  onCleanup(() => c.dispose())
  // The host's push: adopted only when idle and fully acknowledged.
  createEffect(on(options.external, (external) => void c.push(external), { defer: true }))
  return {
    get value() {
      version()
      return c.value
    },
    get revision() {
      version()
      return c.revision
    },
    get pending() {
      version()
      return c.pending
    },
    get focused() {
      version()
      return c.focused
    },
    onFocus: () => c.focus(),
    onBlur: () => c.blur(),
    edit: (next) => c.edit(next),
    commit: () => c.commit(),
  }
}
