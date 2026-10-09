// VAPP-91: the Exponential web app as an Exponential UI HOST. Everything the
// app adds goes through the SDK's public host API (`ExponentialHost` +
// `<HostSurface>`), like a third party's host would:
//   - functions: `harness.toast`, `harness.openIssue`, `harness.openDevice`,
//     `harness.mcp` (a tools/call on the app's own /api/mcp as the viewer);
//   - bindings: the `exp:` scheme over the synced Electric collections
//     (`exp:devices`, `exp:issues?board=…&limit=…`);
//   - extensions: the app extension (`@exp/ui` appExtensionCatalog +
//     appReactExtension) and its declarative packages (Devices);
//   - policy: `harness.mcp` asks first (the consent card), urls through the
//     URL policy (same-origin app links navigate in place);
//   - status: `host_offline` + the catalog-update banner (`HostBanner`).
// No private hook: if one is ever needed, the SDK's API is incomplete.

import { useSyncExternalStore } from "react"
import { ExponentialHost } from "@exponential-at/ui"
import type { FunctionCallInfo, ParsedSource, SourceEmit, Transport } from "@exponential-at/ui"
import { useHostStatus } from "@exponential-at/ui-react"
import { Prompt, appExtensionCatalog, devicesPackage, toast } from "@exp/ui"
import type { VappPackage } from "@exponential-at/ui"
import { getDeviceIconName } from "@exp/ui"
import { boardCollection, deviceCollection, issueCollection, teamCollection } from "@/lib/collections"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { readinessAgo } from "@/lib/coding-readiness"
import { readJsonRpcAnswer } from "@/lib/mcp-oauth/json-rpc-reader"
import { exponentialUiConsentPrompt, promptActions } from "@/lib/prompts"

/** The consent card's store: one pending `ask` call at a time, answered
 *  by the person (Allow / Deny). */
export function createConsentGate() {
  let pending: { call: FunctionCallInfo; resolve: (ok: boolean) => void } | null = null
  const listeners = new Set<() => void>()
  const emit = () => listeners.forEach((l) => l())
  return {
    ask(call: FunctionCallInfo): Promise<boolean> {
      pending?.resolve(false)
      return new Promise<boolean>((resolve) => {
        pending = { call, resolve }
        emit()
      })
    },
    answer(ok: boolean) {
      pending?.resolve(ok)
      pending = null
      emit()
    },
    current: () => pending?.call ?? null,
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => listeners.delete(listener)
    },
  }
}
export type ConsentGate = ReturnType<typeof createConsentGate>

/** Online-ness ages out on a clock, not on a delta: `exp:devices` re-derives
 *  on this beat too (the desktop host's `LIVENESS_TICK`, the Devices page's
 *  `useNow(30_000)`), standing still while the tab is hidden. */
export const LIVENESS_TICK_MS = 30_000

/** `exp:devices` rows at `now`: `Online` within the contract's
 *  `device.onlineWindowSeconds`, else `Last seen <readinessAgo>`. */
export function devicesValue(
  devices: readonly {
    id: string
    label: string
    kind: string
    platform: string | null
    version: string | null
    icon: string | null
    lastSeenAt: Date | string | null
  }[],
  now: Date
) {
  const rows = devices
    .slice()
    .sort((a, b) => a.label.localeCompare(b.label))
    .map((d) => {
      const online = d.lastSeenAt ? deviceRowIsOnline(d.lastSeenAt, now) : false
      const seen = d.lastSeenAt ? new Date(d.lastSeenAt) : null
      return {
        id: d.id,
        label: d.label,
        detail: [d.kind === `server` ? `Server` : `Desktop`, d.platform, d.version ? `v${d.version}` : null].filter(Boolean).join(` · `),
        icon: getDeviceIconName(d),
        discTone: online ? `success` : `muted`,
        tone: online ? `live` : `idle`,
        status: online ? `Online` : seen ? `Last seen ${readinessAgo(now.getTime(), seen.getTime())}` : `Offline`,
      }
    })
  return { rows, count: rows.length, online: rows.filter((r) => r.tone === `live`).length }
}

/** `exp:devices` → `{rows, count, online}` (the Devices template's data),
 *  on every collection change AND every liveness tick (a value equal to the
 *  last one is not re-emitted). */
function devicesSource(_source: ParsedSource, emit: SourceEmit): () => void {
  let last: string | null = null
  const push = () => {
    const value = devicesValue(deviceCollection.toArray, new Date())
    const key = JSON.stringify(value)
    if (key === last) return
    last = key
    emit(value)
  }
  const sub = deviceCollection.subscribeChanges(push, { includeInitialState: true })
  push()
  let timer: ReturnType<typeof setInterval> | null = null
  const visible = () => typeof document === `undefined` || document.visibilityState === `visible`
  const start = () => {
    if (timer === null) timer = setInterval(push, LIVENESS_TICK_MS)
  }
  const stop = () => {
    if (timer !== null) clearInterval(timer)
    timer = null
  }
  const onVisibility = () => {
    if (!visible()) return stop()
    push()
    start()
  }
  if (visible()) start()
  if (typeof document !== `undefined`) document.addEventListener(`visibilitychange`, onVisibility)
  return () => {
    stop()
    if (typeof document !== `undefined`) document.removeEventListener(`visibilitychange`, onVisibility)
    sub.unsubscribe()
  }
}

/** `exp:issues?board=<id>&limit=<n>` → `{rows, count}`. */
function issuesSource(source: ParsedSource, emit: SourceEmit): () => void {
  const limit = Number(source.params.limit ?? 50)
  const push = () => {
    const rows = issueCollection.toArray
      .filter((i) => !source.params.board || i.boardId === source.params.board)
      .sort((a, b) => a.sortOrder - b.sortOrder)
      .slice(0, Number.isFinite(limit) ? limit : 50)
      .map((i) => ({ id: i.id, identifier: i.identifier, title: i.title, status: i.status }))
    emit({ rows, count: rows.length })
  }
  const sub = issueCollection.subscribeChanges(push, { includeInitialState: true })
  push()
  return () => sub.unsubscribe()
}

/** A JSON-RPC `error` reply to `harness.mcp`: the server's message, with
 *  its `code` (the desktop twin's `mcp_result` rejects the same way). */
export class McpCallError extends Error {
  readonly code: number | undefined
  readonly data: unknown
  constructor(error: { code?: number; message?: string; data?: unknown }) {
    super(error.message || `MCP error`)
    this.name = `McpCallError`
    this.code = error.code
    this.data = error.data
  }
}

let mcpRequestId = 0

/** A JSON-RPC tools/call on the app's own MCP endpoint, as the signed-in
 *  viewer (the session cookie); the consent card has already said yes.
 *  Resolves the answer's `result`; rejects on HTTP failure, a JSON-RPC
 *  `error`, or no answer with the request's id (an SSE body is read until
 *  the message whose `id` is ours, like the server's MCP client). */
export async function callMcp(tool: string, args: Record<string, unknown>): Promise<unknown> {
  const id = ++mcpRequestId
  const res = await fetch(`/api/mcp`, {
    method: `POST`,
    credentials: `include`,
    headers: { "content-type": `application/json`, accept: `application/json, text/event-stream` },
    body: JSON.stringify({ jsonrpc: `2.0`, id, method: `tools/call`, params: { name: tool, arguments: args } }),
  })
  if (!res.ok) {
    await res.body?.cancel().catch(() => undefined)
    throw new Error(`MCP ${res.status}`)
  }
  const message = await readJsonRpcAnswer(res, id)
  if (!message) throw new Error(`MCP: no answer`)
  if (message.error) throw new McpCallError(message.error)
  return message.result ?? null
}

/** A synced issue's detail url by identifier (null when not synced). */
function issueHref(identifier: string): string | null {
  const issue = issueCollection.toArray.find((i) => i.identifier === identifier)
  const board = issue ? boardCollection.toArray.find((b) => b.id === issue.boardId) : undefined
  const team = board ? teamCollection.toArray.find((t) => t.id === board.teamId) : undefined
  return issue && board && team ? `/t/${team.slug}/boards/${board.slug}/issues/${issue.identifier}` : null
}

export interface AppHostOptions {
  consent: ConsentGate
  /** Router navigation for same-origin links and harness.openIssue. */
  navigate: (href: string) => void
  /** harness.openDevice; defaults to a toast with the device's name. */
  openDevice?: (id: string) => void
  transport?: Transport
  packages?: readonly VappPackage[]
}

export function createAppHost(options: AppHostOptions): ExponentialHost {
  const origin = typeof window === `undefined` ? `http://localhost` : window.location.origin
  return new ExponentialHost({
    transport: options.transport,
    extensions: [appExtensionCatalog],
    packages: [devicesPackage as unknown as VappPackage, ...(options.packages ?? [])],
    sources: {
      exp: (source, emit) => {
        if (source.name === `devices`) return devicesSource(source, emit)
        if (source.name === `issues`) return issuesSource(source, emit)
        emit(null)
      },
    },
    functions: {
      "harness.toast": ({ message, tone }) => {
        const text = String(message ?? ``)
        if (tone === `error`) toast.error(text)
        else toast(text)
      },
      "harness.openIssue": ({ identifier }) => {
        const href = issueHref(String(identifier ?? ``))
        if (href) options.navigate(href)
        else toast.error(`${String(identifier ?? ``)} is not synced here`)
      },
      "harness.openDevice": ({ id, label }) => (options.openDevice ? options.openDevice(String(id)) : toast(String(label ?? id ?? ``))),
      "harness.mcp": ({ tool, arguments: args }) => callMcp(String(tool), (args ?? {}) as Record<string, unknown>),
    },
    policy: {
      functions: { ask: [`harness.mcp`] },
      onFunctionCall: (call) => options.consent.ask(call),
      media: { baseUrl: origin },
      urls: { baseUrl: origin },
      openUrl: (url) => {
        if (url.startsWith(`${origin}/`)) options.navigate(url.slice(origin.length))
        else window.open(url, `_blank`, `noopener,noreferrer`)
      },
    },
  })
}

/** The consent card: what a surface wants to run, Deny (focused, Enter) /
 *  Allow, worded by the contract's `exponential-ui-consent` prompt. */
export function ConsentCard({ gate }: { gate: ConsentGate }) {
  const call = useSyncExternalStore(gate.subscribe, gate.current, gate.current)
  if (!call) return null
  const tool = call.name === `harness.mcp` ? String(call.args.tool ?? ``) : call.name
  const copy = exponentialUiConsentPrompt(tool)
  return (
    <Prompt
      open
      onOpenChange={(open) => !open && gate.answer(false)}
      data-testid="exponential-ui-consent"
      title={copy.title}
      body={copy.body}
      onDismiss={() => gate.answer(false)}
      actions={promptActions(copy, {
        deny: { onSelect: () => gate.answer(false) },
        allow: { onSelect: () => gate.answer(true) },
      })}
    />
  )
}

/** `host_offline` and the catalog-update banner above a host's surfaces. */
export function HostBanner({ host }: { host: ExponentialHost }) {
  const { status, unsupportedCatalog } = useHostStatus(host)
  const offline = host.hasTransport && (status === `error` || status === `closed`)
  if (unsupportedCatalog)
    return (
      <p className="rounded-md border border-amber-500/40 bg-amber-500/10 px-3 py-2 text-sm text-amber-600 dark:text-amber-300" data-testid="exponential-ui-catalog-update">
        This surface needs a newer Exponential. Update the app to show it.
      </p>
    )
  if (offline)
    return (
      <p className="rounded-md border border-border px-3 py-2 text-sm text-muted-foreground" data-testid="exponential-ui-host-offline">
        The surface's host is offline. Reconnecting…
      </p>
    )
  return null
}
