// #NEW-24 — the view after the host's lease on it expired (OpenClaw keeps a
// view's HTML, arguments and results for ten minutes; a pinned or
// reconstructed card refetches the document but replays no result and grants
// no tool calls). The view then is READ-ONLY: the last screen it drew (kept
// in `sessionStorage` under the host's tool-call id), every action disabled,
// and ONE line, `EXPIRED_LINE`; no error toasts, no retry loop.
//
// A view is expired when
//   - the host answered `ui/initialize` but sent neither the call's input nor
//     its result within `RESULT_WAIT_MS` (with the input, the call is running:
//     `RUNNING_WAIT_MS`), or
//   - the host refused the view's FIRST `tools/call` (a JSON-RPC error from
//     the host, `HostRefusal`; a server error is a result with `isError`, and
//     a throttle is retried, actions.tsx).
// A result that arrives late makes the view live again.

import { useCallback, useEffect, useMemo, useRef, useState } from "react"
import { HostRefusal, type HostContext, type ToolResult } from "./bridge"

export const EXPIRED_LINE = `This view expired · relaunch it from the chat`
export const RESULT_WAIT_MS = 5_000
export const RUNNING_WAIT_MS = 45_000

/** A throttle (OpenClaw: 4 in flight, 30 calls a minute), not a refusal. */
export const BUSY_PATTERN = /concurrency limit|rate limit|too many/i

const KEY_PREFIX = `exp-view`
/** A screen larger than this is not kept (the storage quota is the host's). */
const MAX_STORED_CHARS = 2_000_000

/** True when the host itself refused the call for good. */
export function isLeaseRefusal(error: unknown): boolean {
  return error instanceof HostRefusal && !BUSY_PATTERN.test(error.message)
}

/** Where a view's last screen is kept: the host's tool-call id when it
 *  passes one (`hostContext.toolInfo.id`), else the view alone. */
export function screenKey(view: string, toolCallId: string | number | null | undefined): string {
  return `${KEY_PREFIX}:${view}:${toolCallId === null || toolCallId === undefined ? `` : String(toolCallId)}`
}

function storage(): Storage | null {
  try {
    return window.sessionStorage
  } catch {
    // A sandbox without `allow-same-origin` has no storage.
    return null
  }
}

export function saveScreen(key: string, screen: unknown, store: Storage | null = storage()): void {
  if (!store) return
  try {
    const text = JSON.stringify(screen)
    if (text.length <= MAX_STORED_CHARS) store.setItem(key, text)
  } catch {
    // Quota or a locked store: the view simply has no last screen.
  }
}

export function loadScreen<S>(key: string, store: Storage | null = storage()): S | null {
  if (!store) return null
  try {
    const text = store.getItem(key)
    return text ? (JSON.parse(text) as S) : null
  } catch {
    return null
  }
}

export interface ViewLease<S> {
  expired: boolean
  /** The screen kept for this call, once the lease expired (null = none). */
  restored: S | null
  /** `ui/initialize` answered (and every later context). */
  onContext: (context: HostContext) => void
  onToolInput: () => void
  onToolResult: () => void
  /** Keeps `screen` as the call's last screen (data screens only). */
  keep: (screen: S) => void
  /** Wraps the bridge's `callTool`: expired = no call; a refused first call
   *  expires the view. */
  guard: (call: (name: string, args: Record<string, unknown>) => Promise<ToolResult>) => (name: string, args: Record<string, unknown>) => Promise<ToolResult>
  /** For handlers that outlive a render. */
  isExpired: () => boolean
}

export function useViewLease<S>(view: string, waits: { result?: number; running?: number } = {}): ViewLease<S> {
  const resultWait = waits.result ?? RESULT_WAIT_MS
  const runningWait = waits.running ?? RUNNING_WAIT_MS
  const [expired, setExpired] = useState(false)
  const [restored, setRestored] = useState<S | null>(null)
  const state = useRef({ expired: false, called: false, settled: false, toolCallId: null as string | number | null, timer: null as ReturnType<typeof setTimeout> | null })

  const clear = () => {
    if (state.current.timer) clearTimeout(state.current.timer)
    state.current.timer = null
  }
  const expire = useCallback(() => {
    clear()
    if (state.current.expired) return
    state.current.expired = true
    setRestored(loadScreen<S>(screenKey(view, state.current.toolCallId)))
    setExpired(true)
  }, [view])
  const wait = useCallback((ms: number) => {
    clear()
    if (state.current.settled) return
    state.current.timer = setTimeout(expire, ms)
  }, [expire])
  useEffect(() => clear, [])

  return useMemo<ViewLease<S>>(() => ({
    expired,
    restored,
    onContext: (context) => {
      const id = context.toolInfo?.id
      if (typeof id === `string` || typeof id === `number`) state.current.toolCallId = id
      // The first context is the handshake's answer: the wait starts there.
      if (!state.current.timer && !state.current.settled && !state.current.expired) wait(resultWait)
    },
    onToolInput: () => {
      if (!state.current.expired) wait(runningWait)
    },
    onToolResult: () => {
      clear()
      state.current.settled = true
      state.current.expired = false
      setExpired(false)
      setRestored(null)
    },
    keep: (screen) => {
      if (!state.current.expired) saveScreen(screenKey(view, state.current.toolCallId), screen)
    },
    guard: (call) => async (name, args) => {
      if (state.current.expired) throw new HostRefusal(EXPIRED_LINE)
      try {
        const result = await call(name, args)
        state.current.called = true
        return result
      } catch (error) {
        if (!state.current.called && isLeaseRefusal(error)) expire()
        throw error
      }
    },
    isExpired: () => state.current.expired,
  }), [expired, restored, view, wait, expire, resultWait, runningWait])
}
