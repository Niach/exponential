import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { eq, useLiveQuery } from "@tanstack/react-db"
import {
  composerDraftField,
  composerDraftKey,
  type ComposerDraftTarget,
} from "@exp/domain-contract/composer-draft"
import { composerDraftCollection } from "@/lib/collections"
import { trpc } from "@/lib/trpc-client"
import { SKIP_ERROR_TOAST } from "@/lib/action-error"
import {
  flushAllComposerDrafts,
  flushComposerDraft,
  localComposerDraft,
  setComposerDraftTransport,
  writeComposerDraft,
} from "@/lib/composer-drafts"

// #FEED-219 (fixture `composer-draft.json`): a composer field backed by the
// caller's synced draft for its TARGET. The field shows the draft until the
// person edits it (`composerDraftField`), every edit is written debounced,
// and the pending write is flushed when the composer goes away or the page
// hides. `target` null (an edit composer, no team yet) = a plain field.

let installed = false
function installTransport() {
  if (installed || typeof window === `undefined`) return
  installed = true
  setComposerDraftTransport({
    upsert: (input) => trpc.composerDrafts.upsert.mutate(input, SKIP_ERROR_TOAST),
    delete: (input) => trpc.composerDrafts.delete.mutate(input, SKIP_ERROR_TOAST),
    awaitTxId: (txId) => composerDraftCollection.utils.awaitTxId(txId, 5000),
  })
  window.addEventListener(`pagehide`, flushAllComposerDrafts)
  document.addEventListener(`visibilitychange`, () => {
    if (document.visibilityState === `hidden`) flushAllComposerDrafts()
  })
}

/** The synced draft text of one key (null = no row). */
function useSyncedComposerDraft(key: string | null): string | null {
  const { data } = useLiveQuery(
    (query) =>
      key
        ? query
            .from({ d: composerDraftCollection })
            .where(({ d }) => eq(d.key, key))
        : undefined,
    [key]
  )
  return useMemo(() => {
    const row = (data ?? [])[0] as { text: string } | undefined
    return row ? row.text : null
  }, [data])
}

/** Every key the caller holds a synced draft for. */
export function useComposerDraftKeys(): ReadonlySet<string> {
  const { data } = useLiveQuery((query) => query.from({ d: composerDraftCollection }), [])
  return useMemo(
    () => new Set(((data ?? []) as Array<{ key: string }>).map((row) => row.key)),
    [data]
  )
}

export type ComposerTextUpdate = string | ((current: string) => string)

export interface ComposerDraftField {
  text: string
  /** The live field (read when a send settles). */
  textRef: { readonly current: string }
  /** An edit; `now` writes at once (a send settled, a discard). */
  setText: (next: ComposerTextUpdate, options?: { now?: boolean }) => void
}

export function useComposerDraft(
  target: ComposerDraftTarget | null,
  teamId: string | null | undefined
): ComposerDraftField {
  installTransport()
  const key = target && teamId ? composerDraftKey(target) : null
  const synced = useSyncedComposerDraft(key)
  const [state, setState] = useState({ key, field: ``, edited: false })
  // A new target is a new field (reset during render, no stale frame).
  const current = state.key === key ? state : { key, field: ``, edited: false }
  if (current !== state) setState(current)
  const text = key
    ? composerDraftField({
        field: current.field,
        edited: current.edited,
        local: localComposerDraft(key),
        synced,
      })
    : current.field
  const textRef = useRef(text)
  textRef.current = text
  const keyRef = useRef(key)
  keyRef.current = key
  const teamRef = useRef(teamId)
  teamRef.current = teamId

  const setText = useCallback(
    (next: ComposerTextUpdate, options?: { now?: boolean }) => {
      const value = typeof next === `function` ? next(textRef.current) : next
      textRef.current = value
      const at = keyRef.current
      setState({ key: at, field: value, edited: true })
      if (at && teamRef.current) writeComposerDraft(at, teamRef.current, value, options)
    },
    []
  )

  // Leaving the target (unmount, another key): write what is pending now.
  useEffect(() => {
    if (!key) return
    return () => void flushComposerDraft(key)
  }, [key])

  return { text, textRef, setText }
}
