import { createContext, useContext, useEffect, useState, type ReactNode } from "react"
import type { ToolResult } from "./bridge"
import { decodeToolResult, type Decoded } from "./model"

// EXP-1183 — what a view may DO, handed down by the app shell: call one of
// Exponential's own MCP tools through the host (the MCP Apps `tools/call`
// bridge; the host may ask the person to allow it once per view) and open a
// link outside the frame. Views never fetch on their own.

export interface McpActions {
  /** Calls an Exponential tool and decodes its JSON (or its error text). */
  call: <T = unknown>(name: string, args: Record<string, unknown>) => Promise<Decoded<T>>
  /** Opens a URL through the host (a new tab where it has none). */
  openLink: (url: string) => void
}

const noop: McpActions = {
  call: async () => ({ kind: `error`, message: `No MCP Apps host.` }),
  openLink: () => {},
}

const McpActionsContext = createContext<McpActions>(noop)

export function McpActionsProvider({
  value,
  children,
}: {
  value: McpActions
  children: ReactNode
}) {
  return <McpActionsContext.Provider value={value}>{children}</McpActionsContext.Provider>
}

export function useMcpActions(): McpActions {
  return useContext(McpActionsContext)
}

// Hosts throttle a view: OpenClaw allows 4 requests in flight and ~30 tool
// calls a minute per view, refusing the rest ("concurrency limit reached").
// A view fires pictures, the diff and its poll together, so calls queue
// behind a small gate and a refusal is retried after a pause.
export const MAX_CALLS_IN_FLIGHT = 2
const BUSY_RETRIES = 3
const BUSY_PATTERN = /concurrency limit|rate limit|too many/i

/** Runs `task`s at most `limit` at a time, in call order. */
export function createGate(limit: number) {
  let active = 0
  const waiting: Array<() => void> = []
  return async <T,>(task: () => Promise<T>): Promise<T> => {
    if (active >= limit) await new Promise<void>((resolve) => waiting.push(resolve))
    active += 1
    try {
      return await task()
    } finally {
      active -= 1
      waiting.shift()?.()
    }
  }
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))

/** Builds the actions off the bridge's raw `callTool`. */
export function actionsFromBridge(
  callTool: (name: string, args: Record<string, unknown>) => Promise<ToolResult>,
  openLink: (url: string) => void,
  { retryDelayMs = 1_500 }: { retryDelayMs?: number } = {}
): McpActions {
  const gate = createGate(MAX_CALLS_IN_FLIGHT)
  const attempt = async <T,>(name: string, args: Record<string, unknown>) => {
    try {
      return decodeToolResult<T>(await callTool(name, args))
    } catch (error) {
      return {
        kind: `error` as const,
        message: error instanceof Error ? error.message : String(error),
      }
    }
  }
  return {
    call: <T,>(name: string, args: Record<string, unknown>) =>
      gate(async () => {
        let result = await attempt<T>(name, args)
        for (
          let retry = 1;
          retry <= BUSY_RETRIES && result.kind === `error` && BUSY_PATTERN.test(result.message);
          retry += 1
        ) {
          await sleep(retryDelayMs * retry)
          result = await attempt<T>(name, args)
        }
        return result
      }),
    openLink,
  }
}

/** The attachment id in an app attachment URL (`/api/attachments/{id}`,
 *  relative or absolute, with or without a query). */
export function attachmentIdFromUrl(url: string | null | undefined): string | null {
  const match = url?.match(/\/api\/attachments\/([0-9a-f-]{36})/i)
  return match ? match[1] : null
}

// Signed URLs live a few minutes; one lookup per attachment per view.
const signedCache = new Map<string, Promise<string | null>>()

/** The attachment's short-lived signed URL (`exponential_attachments_get`),
 *  shared per view: one lookup per attachment. */
export function signedAttachmentUrl(
  call: McpActions[`call`],
  id: string
): Promise<string | null> {
  let pending = signedCache.get(id)
  if (!pending) {
    pending = call<{ downloadUrl?: string }>(`exponential_attachments_get`, { id }).then(
      (result) => (result.kind === `ok` ? result.data.downloadUrl ?? null : null)
    )
    signedCache.set(id, pending)
  }
  return pending
}

/** `signedAttachmentUrl` as state: null while loading or when the caller may
 *  not read the attachment. */
export function useAttachmentUrl(id: string | null): string | null {
  const { call } = useMcpActions()
  const [url, setUrl] = useState<string | null>(null)
  useEffect(() => {
    if (!id) return
    let live = true
    void signedAttachmentUrl(call, id).then((value) => {
      if (live) setUrl(value)
    })
    return () => {
      live = false
    }
  }, [id, call])
  return url
}

/** An attachment picture, loaded through its signed URL. */
export function AttachmentImage({
  id,
  alt,
  className,
}: {
  id: string
  alt: string
  className?: string
}) {
  const url = useAttachmentUrl(id)
  if (!url) {
    return (
      <span className={`block animate-pulse rounded-md bg-muted ${className ?? ``}`} style={{ aspectRatio: `16 / 10` }} />
    )
  }
  return <img src={url} alt={alt} className={className} loading="lazy" />
}
