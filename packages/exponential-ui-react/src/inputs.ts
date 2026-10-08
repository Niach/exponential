// VAPP-87: HOST-OWNED inputs. The value lives in local React state; every
// edit bumps a revision and, after `DEBOUNCE_MS`, sends ONE `change` with
// the latest value; `commit` goes out at once on blur / Enter. A value
// pushed by the host (the data model, a new `value` prop) resets the local
// state only when the field is not focused AND no revision is outstanding
// (the host acknowledged everything it was sent). That is the VAPP-4 typing
// rule: a stale echo can never clobber a key in flight.

import { useCallback, useEffect, useRef, useState } from "react"

export const DEBOUNCE_MS = 150

export interface HostOwnedOptions<T> {
  /** The value the host currently holds (resolved prop or data model). */
  external: T
  /** Sends an edit; a returned promise acknowledges the revision on settle. */
  send: (value: T, revision: number, kind: `change` | `commit`) => void | Promise<void>
  /** Optional local write-through (the bound data model path). */
  writeLocal?: (value: T) => void
  debounceMs?: number
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

export function useHostOwnedValue<T>({ external, send, writeLocal, debounceMs = DEBOUNCE_MS }: HostOwnedOptions<T>): HostOwnedValue<T> {
  const [value, setValue] = useState<T>(external)
  const revision = useRef(0)
  const acked = useRef(0)
  const [pending, setPending] = useState(false)
  const [focused, setFocused] = useState(false)
  const focusedRef = useRef(false)
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null)
  const latest = useRef<T>(external)
  const mounted = useRef(true)
  useEffect(() => {
    mounted.current = true
    return () => {
      mounted.current = false
      if (timer.current) clearTimeout(timer.current)
    }
  }, [])

  const ack = useCallback((rev: number) => {
    if (rev > acked.current) acked.current = rev
    if (mounted.current && acked.current >= revision.current) setPending(false)
  }, [])

  const dispatch = useCallback(
    (kind: `change` | `commit`) => {
      const rev = revision.current
      let result: void | Promise<void>
      try {
        result = send(latest.current, rev, kind)
      } catch {
        result = undefined
      }
      if (result && typeof (result as Promise<void>).then === `function`) {
        ;(result as Promise<void>).then(
          () => ack(rev),
          () => ack(rev)
        )
      } else ack(rev)
    },
    [send, ack]
  )

  const edit = useCallback(
    (next: T) => {
      latest.current = next
      revision.current += 1
      setPending(true)
      setValue(next)
      writeLocal?.(next)
      if (timer.current) clearTimeout(timer.current)
      timer.current = setTimeout(() => {
        timer.current = null
        dispatch(`change`)
      }, debounceMs)
    },
    [dispatch, writeLocal, debounceMs]
  )

  const commit = useCallback(() => {
    if (timer.current) {
      clearTimeout(timer.current)
      timer.current = null
    }
    dispatch(`commit`)
  }, [dispatch])

  // The host's push: adopt only when idle and fully acknowledged.
  useEffect(() => {
    if (focusedRef.current) return
    if (revision.current > acked.current) return
    if (Object.is(external, latest.current)) return
    latest.current = external
    setValue(external)
  }, [external])

  const onFocus = useCallback(() => {
    focusedRef.current = true
    setFocused(true)
  }, [])
  const onBlur = useCallback(() => {
    focusedRef.current = false
    setFocused(false)
    commit()
  }, [commit])

  return { value, revision: revision.current, pending, focused, onFocus, onBlur, edit, commit }
}
