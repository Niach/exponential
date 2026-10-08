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

const formatAgo = (date: Date, now: Date) => {
  const minutes = Math.round((now.getTime() - date.getTime()) / 60_000)
  if (minutes < 60) return `${Math.max(minutes, 1)} min ago`
  const hours = Math.round(minutes / 60)
  return hours < 48 ? `${hours} h ago` : `${Math.round(hours / 24)} d ago`
}

/** `exp:devices` → `{rows, count, online}` (the Devices template's data). */
function devicesSource(_source: ParsedSource, emit: SourceEmit): () => void {
  const push = () => {
    const now = new Date()
    const rows = deviceCollection.toArray
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
          status: online ? `Online` : seen ? `Last seen ${formatAgo(seen, now)}` : `Offline`,
        }
      })
    emit({ rows, count: rows.length, online: rows.filter((r) => r.tone === `live`).length })
  }
  const sub = deviceCollection.subscribeChanges(push, { includeInitialState: true })
  push()
  return () => sub.unsubscribe()
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

/** A JSON-RPC tools/call on the app's own MCP endpoint, as the signed-in
 *  viewer (the session cookie); the consent card has already said yes. */
async function callMcp(tool: string, args: Record<string, unknown>): Promise<unknown> {
  const res = await fetch(`/api/mcp`, {
    method: `POST`,
    credentials: `include`,
    headers: { "content-type": `application/json`, accept: `application/json, text/event-stream` },
    body: JSON.stringify({ jsonrpc: `2.0`, id: 1, method: `tools/call`, params: { name: tool, arguments: args } }),
  })
  if (!res.ok) throw new Error(`MCP ${res.status}`)
  const text = await res.text()
  const json = text.trimStart().startsWith(`{`) ? JSON.parse(text) : JSON.parse(text.split(`\n`).find((l) => l.startsWith(`data:`))?.slice(5) ?? `{}`)
  return (json as { result?: unknown }).result
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

/** The consent card: what a surface wants to run, Allow / Deny. */
export function ConsentCard({ gate }: { gate: ConsentGate }) {
  const call = useSyncExternalStore(gate.subscribe, gate.current, gate.current)
  if (!call) return null
  const tool = call.name === `harness.mcp` ? String(call.args.tool ?? ``) : call.name
  return (
    <Prompt
      open
      onOpenChange={(open) => !open && gate.answer(false)}
      data-testid="exponential-ui-consent"
      title={`Allow this surface to run ${tool}?`}
      body={`It acts as you, with your access to this team.`}
      onDismiss={() => gate.answer(false)}
      actions={[
        { label: `Deny`, role: `cancel`, onSelect: () => gate.answer(false) },
        { label: `Allow`, role: `primary`, onSelect: () => gate.answer(true) },
      ]}
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
