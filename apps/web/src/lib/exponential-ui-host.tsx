// VAPP-91: the Exponential web app as an Exponential UI HOST. Everything the
// app adds goes through the SDK's public host API (`ExponentialHost` +
// `<HostSurface>`), like a third party's host would:
//   - functions: `harness.toast`, `harness.openIssue`, `harness.openDevice`,
//     `harness.mcp` (a tools/call on the app's own /api/mcp as the viewer);
//   - bindings: the `exp:` scheme over the synced Electric collections
//     (`exp:devices`; VAPP-119 `exp:issues`, `exp:boards`, `exp:members`,
//     `exp:sessions`, `exp:comments`, `exp:attachments` off fixtures/app-sources.json;
//     VAPP-122 `exp:records?kind=…` over the app's own rows: fixtures/app-records.json;
//     VAPP-161 the format's neutral `records:<kind>?…` + `records.*` = the same);
//   - extensions: the app extension (`@exp/ui` appExtensionCatalog +
//     appReactExtension) and its declarative packages (Devices);
//   - policy: `harness.mcp` asks first (the consent card), urls through the
//     URL policy (same-origin app links navigate in place; VAPP-107: a
//     private origin, the localhost dev setup, is opted back in for the
//     app's own loads and nothing else);
//   - VAPP-157: media narrowed to `appMediaAllowed` (fixtures/apps.json
//     `mediaPolicy`: a data: image/audio/video or the instance's own
//     attachment) by overriding the host's public `mediaRequest`;
//   - status: `host_offline` + the catalog-update banner (`HostBanner`).
// No private hook: if one is ever needed, the SDK's API is incomplete.

import { useState, useSyncExternalStore } from "react"
import { ExponentialHost, isPrivateHost } from "@exponential_app/ui"
import type { FunctionCallInfo, HostFunction, HostOptions, MediaRequest, ParsedSource, SourceEmit, Transport } from "@exponential_app/ui"
import { useHostStatus } from "@exponential_app/ui-react"
import { Button, CodeBlock, CodeBlockContent, Collapsible, CollapsibleContent, CollapsibleTrigger, Prompt, appExtensionCatalog, toast } from "@exp/ui"
import type { AppPackage } from "@exponential_app/ui"
import { getDeviceIconName } from "@exp/ui"
import {
  appCollection,
  appRecordCollection,
  attachmentCollection,
  boardCollection,
  commentCollection,
  deviceCollection,
  issueCollection,
  sessionCollection,
  teamCollection,
  teamMemberCollection,
  userCollection,
} from "@/lib/collections"
import { parseRecordsQuery, recordsSourceParams, recordsValue, type AppRecordRow } from "@exp/domain-contract/app-records"
import { APP_SOURCE_KINDS, sourceValue, type SourceStore } from "@exp/domain-contract/app-sources"
import { APP_ACTIONS, actionConsent, grantAllows, parseActionName } from "@exp/domain-contract/app-actions"
import { appMediaAllowed } from "@exp/domain-contract/apps"
import { grantableTools } from "@/lib/app-grants"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { readinessAgo } from "@/lib/coding-readiness"
import { readJsonRpcAnswer } from "@/lib/mcp-oauth/json-rpc-reader"
import { destructiveTool, exponentialUiConsentPrompt, promptActions } from "@/lib/prompts"

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

const iso = (v: Date | string | null | undefined): string => (v === null || v === undefined ? `` : new Date(v).toISOString())
const day = (v: Date | string | null | undefined): string | null => (v === null || v === undefined ? null : typeof v === `string` ? v.slice(0, 10) : new Date(v).toISOString().slice(0, 10))

/** VAPP-119: the team's synced rows as the contract's `SourceStore` (the
 *  fixture's shapes), read off the collections at call time. */
export function sourceStore(teamId: string | null): SourceStore {
  const boards = boardCollection.toArray.filter((b) => b.teamId === teamId)
  const boardIds = new Set(boards.map((b) => b.id))
  return {
    boards: boards.map((b) => ({ id: b.id, slug: b.slug, name: b.name, prefix: b.prefix, icon: b.icon ?? null, color: b.color ?? null, sortOrder: b.sortOrder, deletedAt: b.deletedAt ? iso(b.deletedAt) : null })),
    issues: issueCollection.toArray
      .filter((i) => boardIds.has(i.boardId))
      .map((i) => ({
        id: i.id,
        boardId: i.boardId,
        identifier: i.identifier,
        title: i.title,
        status: i.status,
        priority: i.priority ?? null,
        assigneeId: i.assigneeId ?? null,
        creatorId: i.creatorId ?? null,
        dueDate: day(i.dueDate),
        sortOrder: i.sortOrder,
        createdAt: iso(i.createdAt),
        updatedAt: iso(i.updatedAt),
      })),
    members: teamMemberCollection.toArray.filter((m) => m.teamId === teamId).map((m) => ({ userId: m.userId, role: m.role })),
    users: userCollection.toArray.map((u) => ({ id: u.id, name: u.name, email: u.email ?? null, image: u.image ?? null })),
    sessions: sessionCollection.toArray
      .filter((x) => x.teamId === teamId)
      .map((x) => ({
        id: x.id,
        parentId: x.parentId ?? null,
        agentId: x.agentId ?? null,
        appId: x.appId ?? null,
        userId: x.userId ?? null,
        status: x.status,
        busy: Boolean(x.busy),
        needsInput: Boolean(x.needsInput),
        title: x.title ?? null,
        createdAt: iso(x.createdAt),
        updatedAt: iso(x.updatedAt),
      })),
    comments: commentCollection.toArray
      .filter((c) => c.teamId === teamId)
      .map((c) => ({ id: c.id, issueId: c.issueId, authorId: c.authorId ?? null, parentId: c.parentId ?? null, source: String(c.source), body: c.body, createdAt: iso(c.createdAt) })),
    attachments: attachmentCollection.toArray
      .filter((a) => a.teamId === teamId)
      .map((a) => ({
        id: a.id,
        issueId: a.issueId ?? null,
        commentId: a.commentId ?? null,
        uploaderId: a.uploaderId ?? null,
        filename: a.filename,
        contentType: a.contentType,
        sizeBytes: a.sizeBytes,
        width: a.width ?? null,
        height: a.height ?? null,
        createdAt: iso(a.createdAt),
      })),
  }
}

/** The collections a source reads (subscribed for the binding's life). */
const SOURCE_COLLECTIONS: Record<string, { subscribeChanges: (cb: () => void, o?: { includeInitialState?: boolean }) => { unsubscribe: () => void }; preload: () => Promise<unknown> }[]> = {
  issues: [issueCollection, boardCollection, userCollection],
  boards: [boardCollection],
  members: [teamMemberCollection, userCollection],
  sessions: [sessionCollection],
  comments: [commentCollection, issueCollection, boardCollection, userCollection],
  attachments: [attachmentCollection, issueCollection, boardCollection, userCollection],
}

/** VAPP-119: `exp:issues`, `exp:boards`, `exp:members`, `exp:sessions`,
 *  `exp:comments`, `exp:attachments` → the fixture's value over the team's
 *  rows (fixtures/app-sources.json), on every change of the collections the
 *  source reads; a value equal to the last one is not re-emitted. */
function teamSource(source: ParsedSource, emit: SourceEmit, scope: RecordsScope): () => void {
  let last: string | null = null
  const push = () => {
    const value = sourceValue(source.name, source.params, sourceStore(scope.teamId), { me: scope.me })
    const key = JSON.stringify(value)
    if (key === last) return
    last = key
    emit(value)
  }
  const collections = SOURCE_COLLECTIONS[source.name] ?? []
  const subs = collections.map((c) => c.subscribeChanges(push))
  // A change subscription alone never starts a shape: an app opened before
  // any board would bind an empty list for good (VAPP-110).
  void Promise.all(collections.map((c) => c.preload())).then(push)
  push()
  return () => subs.forEach((sub) => sub.unsubscribe())
}

/** The app whose records `exp:records` reads: the hosting app, or (`app=`)
 *  another app of the same team by slug. */
export interface RecordsScope {
  appId: string | null
  teamId: string | null
  me: string | null
}

/** VAPP-122: `exp:records?kind=…` → the fixture's value over the synced
 *  `app_records` rows of the app (names off the users shape), on every
 *  change of the records, the users or the apps (an `app=` slug may sync
 *  later); a value equal to the last one is not re-emitted. */
function recordsSource(source: ParsedSource, emit: SourceEmit, scope: RecordsScope): () => void {
  const query = parseRecordsQuery(recordsSourceParams(source) ?? source.params)
  let last: string | null = null
  const push = () => {
    let appId = scope.appId
    if (query.app) {
      const other = appCollection.toArray.find((a) => a.slug === query.app && a.teamId === scope.teamId && !a.deletedAt)
      appId = other?.id ?? null
    }
    const rows: AppRecordRow[] = appId
      ? appRecordCollection.toArray
          .filter((r) => r.appId === appId)
          .map((r) => ({
            id: r.id,
            kind: r.kind,
            data: r.data,
            createdBy: r.createdBy,
            createdAt: new Date(r.createdAt).toISOString(),
            updatedAt: new Date(r.updatedAt).toISOString(),
          }))
      : []
    const names: Record<string, string> = {}
    for (const row of rows) {
      if (row.createdBy && !(row.createdBy in names)) names[row.createdBy] = userCollection.toArray.find((u) => u.id === row.createdBy)?.name ?? ``
    }
    const value = recordsValue(rows, query, { me: scope.me, now: new Date().toISOString(), names })
    const key = JSON.stringify(value)
    if (key === last) return
    last = key
    emit(value)
  }
  const subs = [appRecordCollection.subscribeChanges(push), userCollection.subscribeChanges(push), appCollection.subscribeChanges(push)]
  void appRecordCollection.preload().then(push)
  push()
  return () => subs.forEach((sub) => sub.unsubscribe())
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
  /** VAPP-122: the hosting app (its records) and the viewer. */
  appId?: string | null
  teamId?: string | null
  me?: string | null
  /** VAPP-120: the app's granted typed actions, read at call time (the
   *  settings page may change them while the host lives). */
  grantedTools?: () => readonly string[]
  /** Router navigation for same-origin links and harness.openIssue. */
  navigate: (href: string) => void
  /** harness.openDevice; defaults to a toast with the device's name. */
  openDevice?: (id: string) => void
  transport?: Transport
  packages?: readonly AppPackage[]
  /** The instance origin the media rule compares against (default: the
   *  page's own; tests pass the fixture's). */
  origin?: string
}

/** VAPP-157: an APP host. Every src an app surface loads (Image, Avatar,
 *  Video, Audio, markdown images: the renderers all ask the host's
 *  `mediaRequest` with the raw src, the web plugin sets no `resolveUrl`)
 *  must pass `appMediaAllowed` on the RAW src first, then the SDK's own
 *  media policy: the host narrows, never widens. Non-app hosts (plan cards,
 *  the specimen) are plain `ExponentialHost`s and keep the SDK default. */
export class AppHost extends ExponentialHost {
  private readonly appOrigin: string
  constructor(options: HostOptions, origin: string) {
    super(options)
    this.appOrigin = origin
  }
  override mediaRequest(url: string): MediaRequest | null {
    return appMediaAllowed(url, this.appOrigin) ? super.mediaRequest(url) : null
  }
}

/** VAPP-107: the private hosts the app's policy opts back in: its OWN
 *  origin when that is private (the localhost dev setup), else nothing (the
 *  cloud origin is public; the LAN stays denied either way). */
export function ownPrivateHosts(origin: string): string[] {
  try {
    const host = new URL(origin).hostname
    return isPrivateHost(host) ? [host] : []
  } catch {
    return []
  }
}

export function createAppHost(options: AppHostOptions): AppHost {
  const origin = options.origin ?? (typeof window === `undefined` ? `http://localhost` : window.location.origin)
  const allowPrivate = ownPrivateHosts(origin)
  const scope: RecordsScope = { appId: options.appId ?? null, teamId: options.teamId ?? null, me: options.me ?? null }
  const appRef = () => {
    if (!scope.appId) throw new Error(`No app to keep records for`)
    return { id: scope.appId }
  }
  // VAPP-122: the app's own rows: no consent card; a refusal (the kind's
  // schema, the caps, the plan quota) is a failed action = a toast.
  const recordsUpsert: HostFunction = async ({ kind, data, id }) => {
    try {
      const result = await trpc.apps.records.upsert.mutate(
        { ...appRef(), kind: String(kind ?? ``), data: (data && typeof data === `object` ? data : {}) as Record<string, unknown>, recordId: typeof id === `string` ? id : undefined },
        SKIP_ERROR_TOAST
      )
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      return { id: result.id }
    } catch (error) {
      failedAction(`save`, `the record`, error)
      throw error
    }
  }
  const recordsDelete: HostFunction = async ({ id }) => {
    try {
      const result = await trpc.apps.records.delete.mutate({ ...appRef(), recordId: String(id ?? ``) }, SKIP_ERROR_TOAST)
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    } catch (error) {
      failedAction(`delete`, `the record`, error)
      throw error
    }
  }
  return new AppHost({
    transport: options.transport,
    extensions: [appExtensionCatalog],
    packages: options.packages ?? [],
    sources: {
      exp: (source, emit) => {
        if (source.name === `devices`) return devicesSource(source, emit)
        if (source.name === `records`) return recordsSource(source, emit, scope)
        if (APP_SOURCE_KINDS.includes(source.name)) return teamSource(source, emit, scope)
        emit(null)
      },
      // VAPP-161: the package format's neutral spelling of `exp:records`.
      records: (source, emit) => recordsSource(source, emit, scope),
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
      // VAPP-120: every MCP operation as a typed function (fixtures/app-actions.json):
      // the same tools/call as the viewer, `op` + the args.
      ...Object.fromEntries(
        APP_ACTIONS.map((name) => {
          const target = parseActionName(name)!
          return [name, (args: Record<string, unknown>) => callMcp(target.tool, target.op ? { ...args, op: target.op } : args)]
        })
      ),
      "harness.records.upsert": recordsUpsert,
      "harness.records.delete": recordsDelete,
      // VAPP-161: the package format's neutral names, the same writes.
      "records.upsert": recordsUpsert,
      "records.delete": recordsDelete,
    },
    policy: {
      functions: { ask: [`harness.mcp`, `exponential.*`] },
      // A granted typed action runs without the card; anything else asks.
      onFunctionCall: (call) => (parseActionName(call.name) && grantAllows(grantableTools(options.grantedTools?.()), call.name, call.args) ? true : options.consent.ask(call)),
      media: { baseUrl: origin, allowPrivate },
      urls: { baseUrl: origin, allowPrivate },
      openUrl: (url) => {
        if (url.startsWith(`${origin}/`)) options.navigate(url.slice(origin.length))
        else window.open(url, `_blank`, `noopener,noreferrer`)
      },
    },
  }, origin)
}

/** VAPP-107 (the consent prompt's `slot`): what the card shows of a call:
 *  the tool it names, the arguments it carries (a `harness.mcp` call's MCP
 *  `arguments`, else the function's own args; an empty object = none, as
 *  compact JSON) and whether it sounds destructive. The desktop twin's
 *  `consent_words` derives the same three. */
export function consentWords(call: Pick<FunctionCallInfo, `name` | `args`>): { tool: string; args: string | null; destructive: boolean; title?: string } {
  // VAPP-120: a typed action is worded for its operation (the contract's rows).
  const typed = actionConsent(call.name, call.args)
  if (typed) return { tool: typed.tool, args: typed.args, destructive: typed.destructive, title: typed.title }
  const mcp = call.name === `harness.mcp`
  const tool = mcp ? String(call.args.tool ?? ``) : call.name
  const raw = mcp ? call.args.arguments : call.args
  const args = raw && typeof raw === `object` ? JSON.stringify(raw) : null
  const op = raw && typeof raw === `object` ? (raw as Record<string, unknown>).op : undefined
  return { tool, args: args === null || args === `{}` ? null : args, destructive: destructiveTool(tool, op) }
}

/** Past this many characters the arguments block collapses to a preview
 *  with a Show all disclosure (the prompt's `slot`). */
export const CONSENT_ARGS_PREVIEW = 160

/** The arguments block of the consent card: compact JSON in the CodeBlock
 *  (sm, json); past `CONSENT_ARGS_PREVIEW` characters a preview with a
 *  disclosure that shows the whole JSON pretty-printed. */
export function ConsentArguments({ args }: { args: string }) {
  const [open, setOpen] = useState(false)
  const block = (code: string) => (
    <CodeBlock size="sm" data-testid="exponential-ui-consent-arguments">
      <CodeBlockContent language="json">{code}</CodeBlockContent>
    </CodeBlock>
  )
  if (args.length <= CONSENT_ARGS_PREVIEW) return block(args)
  let pretty = args
  try {
    pretty = JSON.stringify(JSON.parse(args), null, 2)
  } catch {
    // Not JSON: the compact text stands.
  }
  return (
    <Collapsible open={open} onOpenChange={setOpen} className="flex flex-col gap-2">
      {open ? <CollapsibleContent forceMount>{block(pretty)}</CollapsibleContent> : block(`${args.slice(0, CONSENT_ARGS_PREVIEW)}…`)}
      <CollapsibleTrigger asChild>
        <Button variant="ghost" size="sm" className="self-start" data-testid="exponential-ui-consent-show-all">
          {open ? `Show less` : `Show all`}
        </Button>
      </CollapsibleTrigger>
    </Collapsible>
  )
}

/** The consent card: what a surface wants to run (the tool AND its
 *  arguments), Deny (focused, Enter) / Allow, worded by the contract's
 *  `exponential-ui-consent` prompt; a destructive-sounding call takes the
 *  `exponential-ui-consent-destructive` entry (Allow in the destructive
 *  role). */
export function ConsentCard({ gate }: { gate: ConsentGate }) {
  const call = useSyncExternalStore(gate.subscribe, gate.current, gate.current)
  if (!call) return null
  const { tool, args, destructive, title } = consentWords(call)
  const copy = { ...exponentialUiConsentPrompt(tool, destructive), ...(title ? { title } : {}) }
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
    >
      {args === null ? null : <ConsentArguments key={args} args={args} />}
    </Prompt>
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
