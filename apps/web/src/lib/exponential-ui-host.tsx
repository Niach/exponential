// VAPP-91: the Exponential web app as an Exponential UI HOST. Everything the
// app adds goes through the SDK's public host API (`ExponentialHost` +
// `<HostSurface>`), like a third party's host would:
//   - functions: `harness_toast`, `harness_mcp` (a tools/call on the app's own
//     /api/mcp as the viewer); a link goes through the URL policy (`~/` = the
//     app's team) and app-link.json's classifier;
//   - bindings: the `exp:` scheme over the synced Electric collections
//     (`exp:devices`; VAPP-119 `exp:issues`, `exp:boards`, `exp:members`,
//     `exp:sessions`, `exp:comments`, `exp:attachments` off fixtures/app-sources.json);
//   - records: the format's `records:<kind>?…` over the app's own rows
//     (VAPP-189: the eUI evaluator, @exponential_app/eui/host/query +
//     fixtures/records-query.json) and `records.upsert/delete/batch`;
//   - extensions: the app extension (`@exp/ui` appExtensionCatalog +
//     appReactExtension) and its declarative packages (Devices);
//   - policy: `harness_mcp` asks first (the consent card), urls through the
//     URL policy (same-origin app links navigate in place; VAPP-107: a
//     private origin, the localhost dev setup, is opted back in for the
//     app's own loads and nothing else);
//   - VAPP-157: media narrowed to `appMediaAllowed` (fixtures/apps.json
//     `mediaPolicy`: a data: image/audio/video or the instance's own
//     attachment) by overriding the host's public `mediaRequest`;
//   - upstream reads (#VAPP-230): `upstream:<server>/<tool>?…` through the
//     server broker (`apps.upstreamRead`, re-read every 60 s), the
//     "Connect <server>" banner when the viewer is not connected;
//   - status: `host_offline` + the catalog-update banner (`HostBanner`).
// No private hook: if one is ever needed, the SDK's API is incomplete.

import { toastRecordUndo } from "@/lib/apps/record-undo"
import { appLinkPath, classifyAppLink, crossAppLink, isRoutedAppPath, issueTextLinks, LINK_NO_ACCESS } from "@/lib/app-link"
import { useState, useSyncExternalStore } from "react"
import { eq } from "@tanstack/react-db"
import { ExponentialHost, isPrivateHost, readPointer } from "@exponential_app/eui"
import type { FunctionCallInfo, HostFunction, HostOptions, MediaRequest, ParsedSource, SourceEmit, Transport } from "@exponential_app/eui"
import { useHostStatus } from "@exponential_app/eui-react"
import { Button, conceptIcon, CodeBlock, CodeBlockContent, Collapsible, CollapsibleContent, CollapsibleTrigger, Prompt, appExtensionCatalog, toast } from "@exp/ui"
import type { AppPackage } from "@exponential_app/eui"
import { getDeviceIconName } from "@exp/ui"
import {
  appCollection,
  appRecordCollection,
  attachmentCollection,
  boardCollection,
  commentCollection,
  deviceCollection,
  issueCollection,
  issueEventCollection,
  sessionCollection,
  sessionIssueCollection,
  teamCollection,
  teamMemberCollection,
  userCollection,
} from "@/lib/collections"
import { RECORDS_SCHEME, kindComputed, kindRefs, parseRecordsQuery, recordAuthor, recordsValue, type RecordRow } from "@exponential_app/eui/host/query"
import { APP_SOURCE_KINDS, sourceValue, type SourceStore } from "@exp/domain-contract/app-sources"
import { APP_ACTIONS, APP_ACTION_ALIASES, actionConsent, grantAllows, parseActionName } from "@exp/domain-contract/app-actions"
import { APP_FILES, APPS_COPY, appMediaAllowed, appUploadPath, appUploadTarget, withUploadedUrl } from "@exp/domain-contract/apps"
import { hostZone, zoneDay } from "@exp/domain-contract/team-zone"
import { VIA_APP_HEADER, viaAppClaim, viaAppHeaderValue, type ViaAppClaim } from "@exp/domain-contract/via-app"
import { RecordRefusal, kindTransitions, migrateRecord, packageMigrations, readableRows, type AppAccessLevel, type RecordMigration } from "@exponential_app/eui/host"
import { trpcErrorCode, trpcErrorMessage } from "@/lib/trpc-error"
import { grantableTools } from "@/lib/app-grants"
import { failedAction, SKIP_ERROR_TOAST } from "@/lib/action-error"
import { trpc } from "@/lib/trpc-client"
import { deviceRowIsOnline } from "@/lib/steer-devices"
import { readinessAgo } from "@/lib/coding-readiness"
import { readJsonRpcAnswer } from "@/lib/emcp/upstream/json-rpc-reader"
import { UPSTREAM_BINDING, upstreamHostBanners, type UpstreamHostBanner, type UpstreamReadAnswer } from "@exp/emcp/upstream"
import { incrementalTable, memoByVersions, type ChangeSource, type IncrementalTable } from "@/lib/incremental-table"
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

type Row<C> = C extends { toArray: readonly (infer R)[] } ? R : never
/** A collection as the incremental table's change source (TanStack DB's
 *  change messages carry the full row). */
const changes = <C extends { subscribeChanges: unknown }>(c: C) => c as unknown as ChangeSource<Row<C>>

/** The `SourceStore` tables (app-sources.json) and the collections a
 *  source reads: only those are kept. */
type StoreTable = keyof SourceStore
const SOURCE_TABLES: Record<string, readonly StoreTable[]> = {
  issues: [`issues`, `boards`, `users`],
  boards: [`boards`],
  members: [`members`, `users`],
  me: [`members`, `users`],
  sessions: [`sessions`],
  comments: [`comments`, `issues`, `boards`, `users`],
  attachments: [`attachments`, `issues`, `boards`, `users`],
}
/** FEED-243: `exp:issues?touched=` also reads who touched what. */
const TOUCH_TABLES: readonly StoreTable[] = [`issueEvents`, `comments`, `sessions`, `sessionIssues`]
const sourceTables = (source: ParsedSource): readonly StoreTable[] =>
  source.name === `issues` && source.params.touched ? [...SOURCE_TABLES.issues!, ...TOUCH_TABLES] : (SOURCE_TABLES[source.name] ?? [])

const STORE_COLLECTIONS: Record<StoreTable, { preload: () => Promise<unknown> }> = {
  boards: boardCollection,
  issues: issueCollection,
  members: teamMemberCollection,
  users: userCollection,
  sessions: sessionCollection,
  comments: commentCollection,
  attachments: attachmentCollection,
  issueEvents: issueEventCollection,
  sessionIssues: sessionIssueCollection,
}

/** VAPP-119 / VAPP-159: the team's synced rows as the contract's
 *  `SourceStore` (the fixture's shapes), kept INCREMENTALLY: each table maps
 *  only the rows a change names (`incrementalTable`), the store's arrays are
 *  rebuilt once per change batch, and only the `tables` a source reads are
 *  subscribed. Issues follow the team through its boards (memoized on both
 *  tables' versions). */
export function createTeamStore(teamId: string | null, tables: readonly StoreTable[], onChange: () => void) {
  const want = new Set(tables)
  const kept: IncrementalTable<unknown>[] = []
  const keep = <T, R>(name: StoreTable, source: ChangeSource<T>, map: (row: T) => R | null): IncrementalTable<R> | null => {
    if (!want.has(name)) return null
    const table = incrementalTable(source, map, () => onChange())
    kept.push(table as IncrementalTable<unknown>)
    return table
  }
  const boards = keep(`boards`, changes(boardCollection), (b) =>
    b.teamId === teamId ? { id: b.id, slug: b.slug, name: b.name, prefix: b.prefix, icon: b.icon ?? null, color: b.color ?? null, sortOrder: b.sortOrder, deletedAt: b.deletedAt ? iso(b.deletedAt) : null } : null
  )
  const issues = keep(`issues`, changes(issueCollection), (i) => ({
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
  }))
  const members = keep(`members`, changes(teamMemberCollection), (m) => (m.teamId === teamId ? { userId: m.userId, role: m.role } : null))
  const users = keep(`users`, changes(userCollection), (u) => ({ id: u.id, name: u.name, email: u.email ?? null, image: u.image ?? null }))
  const sessions = keep(`sessions`, changes(sessionCollection), (x) =>
    x.teamId === teamId
      ? {
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
        }
      : null
  )
  const comments = keep(`comments`, changes(commentCollection), (c) =>
    c.teamId === teamId ? { id: c.id, issueId: c.issueId, authorId: c.authorId ?? null, parentId: c.parentId ?? null, source: String(c.source), body: c.body, createdAt: iso(c.createdAt) } : null
  )
  const attachments = keep(`attachments`, changes(attachmentCollection), (a) =>
    a.teamId === teamId
      ? {
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
        }
      : null
  )
  const issueEvents = keep(`issueEvents`, changes(issueEventCollection), (e) =>
    e.teamId === teamId ? { id: e.id, issueId: e.issueId, actorUserId: e.actorUserId ?? null, createdAt: iso(e.createdAt) } : null
  )
  const sessionIssues = keep(`sessionIssues`, changes(sessionIssueCollection), (l) => (l.teamId === teamId ? { sessionId: l.sessionId, issueId: l.issueId } : null))
  const teamIssues = memoByVersions(
    () => [boards?.version() ?? 0, issues?.version() ?? 0],
    () => {
      const boardIds = new Set((boards?.rows() ?? []).map((b) => b.id))
      return (issues?.rows() ?? []).filter((i) => boardIds.has(i.boardId))
    }
  )
  return {
    store: (): SourceStore => ({
      boards: boards?.rows() ?? [],
      issues: teamIssues(),
      members: members?.rows() ?? [],
      users: users?.rows() ?? [],
      sessions: sessions?.rows() ?? [],
      comments: comments?.rows() ?? [],
      attachments: attachments?.rows() ?? [],
      issueEvents: issueEvents?.rows() ?? [],
      sessionIssues: sessionIssues?.rows() ?? [],
    }),
    unsubscribe: () => kept.forEach((t) => t.unsubscribe()),
  }
}

/** VAPP-119: `exp:issues`, `exp:boards`, `exp:members`, `exp:sessions`,
 *  `exp:comments`, `exp:attachments` → the fixture's value over the team's
 *  rows (fixtures/app-sources.json), on every change of the tables the
 *  source reads (kept incrementally, `createTeamStore`); a value equal to
 *  the last one is not re-emitted. */
function teamSource(source: ParsedSource, emit: SourceEmit, scope: RecordsScope): () => void {
  const tables = sourceTables(source)
  let last: string | null = null
  let ready = false
  const push = () => {
    if (!ready) return
    const value = sourceValue(source.name, source.params, team.store(), {
      me: scope.me,
      access: scope.access,
      timeZone: scope.teamId ? (teamCollection.get(scope.teamId)?.timeZone ?? null) : null,
    })
    const key = JSON.stringify(value)
    if (key === last) return
    last = key
    emit(value)
  }
  const team = createTeamStore(scope.teamId, tables, push)
  ready = true
  // A change subscription alone never starts a shape: an app opened before
  // any board would bind an empty list for good (VAPP-110).
  void Promise.all(tables.map((t) => STORE_COLLECTIONS[t].preload())).then(push)
  push()
  return () => team.unsubscribe()
}

/** VAPP-201: the team zone's offset at an instant (the device's without a
 *  team zone); one offset function per zone name, so a formatter is not
 *  rebuilt on every date. */
const zoneFns = new Map<string, (ms: number) => number>()
function teamZoneOffset(teamId: string | null, ms: number): number {
  const name = (teamId ? teamCollection.get(teamId)?.timeZone : null) ?? ``
  let fn = zoneFns.get(name)
  if (!fn) {
    fn = hostZone(name)
    zoneFns.set(name, fn)
  }
  return fn(ms)
}

/** The app whose records `records:` reads: the hosting app, or (`app=`)
 *  another app of the same team by slug. */
export interface RecordsScope {
  appId: string | null
  teamId: string | null
  me: string | null
  /** VAPP-206: the viewer's app access (`exp:me`). */
  access?: string
  /** VAPP-198 (Run as): only the rows this member may read (record-policy.json `readable`). */
  readable?: (rows: readonly RecordRow[]) => RecordRow[]
  /** VAPP-214: the hosting package's kinds (their `computed` fields apply on read). */
  records?: Record<string, unknown>
  /** VAPP-222 (R6): the viewer's team role (the rows' `transitions`). */
  role?: () => string | null
  /** eApps R8: the hosting package's migrations + the version its records are in (a draft preview). */
  migrate?: { migrations: RecordMigration[]; from: string } | null
}

/** eApps R8: one stored row as the draft's migrations would leave it (a failing row as stored). */
function migratedRow(row: RecordRow, migrate: { migrations: RecordMigration[]; from: string }): RecordRow {
  const out = migrateRecord(migrate.migrations, migrate.from, row.kind, row.data, Date.now())
  return `error` in out || out.applied === 0 ? row : { ...row, kind: out.kind, data: out.data }
}

/** VAPP-122: `records:<kind>?…` → the eUI evaluator's value over the synced
 *  `app_records` rows of the app (names off the users shape), on every
 *  change of the records, the users or the apps (an `app=` slug may sync
 *  later); a value equal to the last one is not re-emitted. VAPP-159: the
 *  records load ON DEMAND, narrowed by the app (app-records.json SYNC), and
 *  both tables are kept incrementally. */
function recordsSource(source: ParsedSource, emit: SourceEmit, scope: RecordsScope): () => void {
  const query = parseRecordsQuery(source.params, source.name)
  let last: string | null = null
  let ready = false
  let appId: string | null = null
  let records: IncrementalTable<RecordRow> | null = null
  const push = () => {
    if (!ready) return
    // eApps R8: a draft preview reads migrated copies of the hosting app's rows.
    const stored = records?.rows() ?? []
    const all = scope.migrate && !query.app ? stored.map((row) => migratedRow(row, scope.migrate!)) : stored
    const rows = scope.readable ? scope.readable(all) : all
    const names: Record<string, string> = {}
    for (const row of rows) {
      const author = recordAuthor(row)
      if (author && !(author in names)) names[author] = users.get(author) ?? ``
    }
    // VAPP-214: the hosting app's computed fields (another app's rows carry none).
    const computed = query.app ? undefined : kindComputed(scope.records, query.from)
    // Dogfood G: the month view's today + default month in the TEAM zone, the day today() reads.
    const ms = Date.now()
    // VAPP-219 (R5): what `expand` follows: member refs by name, issue refs by identifier.
    const refs = query.app ? undefined : kindRefs(scope.records, query.from)
    let issues: Record<string, Record<string, unknown>> | undefined
    for (const field of query.expand) {
      const target = refs?.[field]
      if (target !== `member` && target !== `issue`) continue
      for (const row of rows) {
        const id = row.kind === query.from ? row.data[field] : undefined
        if (typeof id !== `string` || !id) continue
        if (target === `member` && !(id in names) && users.get(id) !== undefined) names[id] = users.get(id) ?? ``
        if (target === `issue` && !issues?.[id]) {
          const i = issueCollection.toArray.find((x) => x.identifier === id && x.teamId === scope.teamId)
          if (i) (issues ??= {})[id] = { id: i.id, identifier: i.identifier, title: i.title, status: i.status }
        }
      }
    }
    // VAPP-222 (R6): the hosting app's state machine: each row's legal next states.
    const transitions = query.app || !query.from ? null : kindTransitions(scope.records?.[query.from])
    const value = recordsValue(rows, query, { me: scope.me, now: new Date(ms).toISOString(), today: zoneDay((at) => teamZoneOffset(scope.teamId, at), ms), names, computed, refs, issues, transitions, role: transitions ? (scope.role?.() ?? null) : null })
    const key = JSON.stringify(value)
    if (key === last) return
    last = key
    emit(value)
  }
  const users = incrementalTable(changes(userCollection), (u) => u.name, push)
  // VAPP-219: an expanded issue ref re-emits when the issues change.
  const issueRefs = !query.app && query.expand.some((f) => kindRefs(scope.records, query.from)?.[f] === `issue`) ? issueCollection.subscribeChanges(() => push()) : null
  /** The app the binding reads (the hosting one, or `app=` by slug); a
   *  move re-subscribes the records to that app's subset. */
  const resolve = () => {
    const next = query.app
      ? (appCollection.toArray.find((a) => a.slug === query.app && a.teamId === scope.teamId && !a.deletedAt)?.id ?? null)
      : scope.appId
    if (next === appId && (records || !next)) return
    appId = next
    records?.unsubscribe()
    records = next
      ? incrementalTable(
          changes(appRecordCollection),
          (r) =>
            r.appId === next
              ? {
                  id: r.id,
                  kind: r.kind,
                  data: r.data,
                  createdBy: r.createdBy,
                  authorId: r.authorId,
                  createdAt: new Date(r.createdAt).toISOString(),
                  updatedAt: new Date(r.updatedAt).toISOString(),
                }
              : null,
          push,
          (r: { appId: unknown }) => eq(r.appId, next)
        )
      : null
  }
  const apps = query.app
    ? appCollection.subscribeChanges(() => {
        resolve()
        push()
      })
    : null
  if (query.app) void appCollection.preload()
  resolve()
  ready = true
  push()
  return () => {
    apps?.unsubscribe()
    records?.unsubscribe()
    users.unsubscribe()
    issueRefs?.unsubscribe()
  }
}

/** A JSON-RPC `error` reply to `harness_mcp`: the server's message, with
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
export async function callMcp(tool: string, args: Record<string, unknown>, viaApp?: ViaAppClaim | null): Promise<unknown> {
  const id = ++mcpRequestId
  const res = await fetch(`/api/mcp`, {
    method: `POST`,
    credentials: `include`,
    headers: { "content-type": `application/json`, accept: `application/json, text/event-stream`, ...(viaApp ? { [VIA_APP_HEADER]: viaAppHeaderValue(viaApp) } : {}) },
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
  /** Router navigation for same-origin links (a push: back returns here). */
  navigate: (href: string) => void
  /** VAPP-218: `app.navigate` opens one of the package's screens (the
   *  preview's shell); unset = the host refuses it. */
  navigateScreen?: HostOptions[`navigate`]
  transport?: Transport
  packages?: readonly AppPackage[]
  /** The instance origin the media rule compares against (default: the
   *  page's own; tests pass the fixture's). */
  origin?: string
  /** VAPP-198: the editor preview "Run as <member>" (owners): `me` is them,
   *  the records are what their role and app access may read, and every
   *  write or host call is refused with `copy.runAs.writesOff`. */
  runAs?: { id: string; name: string; role: string; access?: AppAccessLevel } | null
  /** VAPP-206: the viewer's app access (`exp:me`; Run as: theirs). */
  access?: AppAccessLevel
  /** eApps R8 (apps.json `migrationCases`): previewing a draft with
   *  `migrations`: the published package's `version`; the hosting app's
   *  rows read through `migrateRecord` (copies, nothing written). */
  migrateFrom?: string | null
}

/** #VAPP-230: every live upstream binding's last answer, for the banners
 *  (@exp/emcp `upstreamHostBanners`: one per server answering
 *  upstreamDisconnected, with Connect, or #VAPP-239 an offline device,
 *  without). */
export class UpstreamAnswers {
  private answers = new Map<number, UpstreamReadAnswer>()
  private listeners = new Set<() => void>()
  private snapshot: UpstreamHostBanner[] = []
  set(id: number, answer: UpstreamReadAnswer | null) {
    if (answer) this.answers.set(id, answer)
    else this.answers.delete(id)
    const next = upstreamHostBanners([...this.answers.values()])
    if (JSON.stringify(next) === JSON.stringify(this.snapshot)) return
    this.snapshot = next
    for (const l of this.listeners) l()
  }
  subscribe = (listener: () => void) => {
    this.listeners.add(listener)
    return () => void this.listeners.delete(listener)
  }
  banners = () => this.snapshot
}

let upstreamBindingIds = 0

/** #VAPP-230: `upstream:<server>/<tool>?…` → the broker's answer for the
 *  viewer, re-read every `cacheSeconds` while bound. A refusal or offline
 *  emits nothing (the path keeps the template's data: the binding's
 *  default); a disconnected one raises the "Connect <server>" banner. */
function upstreamSource(source: ParsedSource, emit: SourceEmit, scope: RecordsScope, answers: UpstreamAnswers): () => void {
  const id = ++upstreamBindingIds
  let live = true
  const read = async () => {
    if (!scope.appId) return
    try {
      const answer = await trpc.apps.upstreamRead.query({ id: scope.appId, source: source.uri }, SKIP_ERROR_TOAST)
      if (!live) return
      answers.set(id, answer as UpstreamReadAnswer)
      if (answer.ok) emit(answer.value ?? null)
    } catch {
      // Offline or refused: the binding keeps its default.
    }
  }
  void read()
  const timer = setInterval(read, UPSTREAM_BINDING.cacheSeconds * 1000)
  return () => {
    live = false
    clearInterval(timer)
    answers.set(id, null)
  }
}

/** VAPP-157: an APP host. Every src an app surface loads (Image, Avatar,
 *  Video, Audio, markdown images: the renderers all ask the host's
 *  `mediaRequest` with the raw src, the web plugin sets no `resolveUrl`)
 *  must pass `appMediaAllowed` on the RAW src first, then the SDK's own
 *  media policy: the host narrows, never widens. Non-app hosts (plan cards,
 *  the specimen) are plain `ExponentialHost`s and keep the SDK default. */
export class AppHost extends ExponentialHost {
  private readonly appOrigin: string
  /** #VAPP-230: the live upstream reads' last answers (the banners). */
  readonly upstreams: UpstreamAnswers
  /** The team's slug (the banner's Settings › MCP link). */
  teamSlug: () => string | null = () => null
  /** VAPP-221: a FileUpload's picked files (the renderer plugin's
   *  `onUpload`): see `createAppHost`. */
  upload: (files: File[], target: { name: string }) => Promise<void> = async () => {}
  constructor(options: HostOptions, origin: string, upstreams: UpstreamAnswers = new UpstreamAnswers()) {
    super(options)
    this.appOrigin = origin
    this.upstreams = upstreams
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

/** VAPP-198: under Run as, every function but the toast and navigation is
 *  refused with `copy.runAs.writesOff` (a preview never acts as someone else). */
const RUN_AS_ALLOWED = new Set([`harness_toast`])
function refuseAsOther(runAs: AppHostOptions[`runAs`], functions: Record<string, HostFunction>): Record<string, HostFunction> {
  if (!runAs) return functions
  const refused: HostFunction = () => {
    const text = APPS_COPY.runAs.writesOff.replace(`{name}`, runAs.name)
    toast(text)
    throw new Error(text)
  }
  return Object.fromEntries(Object.entries(functions).map(([name, fn]) => [name, RUN_AS_ALLOWED.has(name) ? fn : refused]))
}

/** #EXP-1674: the app a link names, as a link may open it: its id and its
 *  PUBLISHED package (a draft runs for nobody); null = no such app here. */
function targetApp(teamSlug: string, appSlug: string): { id: string; package: { screens?: unknown } | null } | null {
  const team = teamCollection.toArray.find((t) => t.slug === teamSlug)
  const app = team && appCollection.toArray.find((a) => a.teamId === team.id && a.slug === appSlug)
  if (!app) return null
  return { id: app.id, package: app.publishedVersion > 0 ? (app.package as { screens?: unknown } | null) : null }
}

export function createAppHost(options: AppHostOptions): AppHost {
  const origin = options.origin ?? (typeof window === `undefined` ? `http://localhost` : window.location.origin)
  const allowPrivate = ownPrivateHosts(origin)
  const runAs = options.runAs ?? null
  const kinds = (options.packages?.[0] as { records?: Record<string, unknown> } | undefined)?.records
  const scope: RecordsScope = {
    appId: options.appId ?? null,
    teamId: options.teamId ?? null,
    me: runAs ? runAs.id : (options.me ?? null),
    access: runAs ? (runAs.access ?? `write`) : (options.access ?? `write`),
    readable: runAs ? (rows) => readableRows(rows, kinds, { id: runAs.id, role: runAs.role, access: runAs.access }) : undefined,
    records: kinds,
    migrate: options.migrateFrom && packageMigrations(options.packages?.[0]).length ? { migrations: packageMigrations(options.packages?.[0]), from: options.migrateFrom } : null,
    // VAPP-222 (R6): the viewer's team role (an owner takes every edge).
    role: () => (runAs ? runAs.role : (teamMemberCollection.toArray.find((m) => m.teamId === options.teamId && m.userId === options.me)?.role ?? null)),
  }
  const appRef = () => {
    if (!scope.appId) throw new Error(`No app to keep records for`)
    return { id: scope.appId }
  }
  // VAPP-231 (via-app.json `claim`): what this host's writes claim, read at
  // send time; the server checks it, it only adds the "via <app>" caption.
  const claim = () => viaAppClaim(scope.appId, appCollection.toArray.find((a) => a.id === scope.appId)?.publishedVersion)
  const claimed = () => {
    const viaApp = claim()
    return viaApp ? { viaApp } : {}
  }
  // VAPP-122: the app's own rows: no consent card; a refusal (the kind's
  // schema, the caps, the plan quota) is a failed action = a toast. VAPP-245:
  // a record form's invalid input shows under the form (app-records.json
  // `dataTab`, the same rule); `author` = an owner's on-behalf pick.
  const recordsUpsert: HostFunction = async ({ kind, data, id, author, form }) => {
    try {
      const result = await trpc.apps.records.upsert.mutate(
        { ...appRef(), kind: String(kind ?? ``), data: (data && typeof data === `object` ? data : {}) as Record<string, unknown>, recordId: typeof id === `string` ? id : undefined, ...(typeof author === `string` ? { author } : {}), ...claimed() },
        SKIP_ERROR_TOAST
      )
      await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
      return { id: result.id }
    } catch (error) {
      if (form && trpcErrorCode(error) === `BAD_REQUEST`) throw new RecordRefusal(trpcErrorMessage(error, `record is invalid`))
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
  // VAPP-194: a package action's writes as ONE transaction (the server
  // checks them again); a refusal lands nothing and is the action's failure.
  const recordsBatch: HostFunction = async ({ writes }) => {
    const result = await trpc.apps.records.batch.mutate({ ...appRef(), writes: (Array.isArray(writes) ? writes : []) as never, ...claimed() }, SKIP_ERROR_TOAST)
    await appRecordCollection.utils.awaitTxId(result.txid, 3000).catch(() => undefined)
    // VAPP-215: a package action's writes toast Undo for 10 s (app-records.json `history`).
    if (scope.appId) toastRecordUndo(scope.appId, `batch`, result.eventIds, `app`)
    return { ids: result.ids }
  }
  const upstreamAnswers = new UpstreamAnswers()
  const createTeamSlug = teamCollection.toArray.find((t) => t.id === options.teamId)?.slug
  const host = new AppHost({
    transport: options.transport,
    me: scope.me,
    // VAPP-201 (team-zone.json): dates in the TEAM zone (read live off the
    // synced team), else the device's.
    zone: (ms) => teamZoneOffset(scope.teamId, ms),
    onActionFailed: ({ message }) => void failedAction(`save`, `the record`, undefined, { description: message }),
    navigate: options.navigateScreen,
    extensions: [appExtensionCatalog],
    packages: options.packages ?? [],
    sources: {
      exp: (source, emit) => {
        if (source.name === `devices`) return devicesSource(source, emit)
        if (APP_SOURCE_KINDS.includes(source.name)) return teamSource(source, emit, scope)
        emit(null)
      },
      [RECORDS_SCHEME]: (source, emit) => recordsSource(source, emit, scope),
      [UPSTREAM_BINDING.scheme]: (source, emit) => upstreamSource(source, emit, scope, upstreamAnswers),
    },
    functions: refuseAsOther(runAs, {
      harness_toast: ({ message, tone }) => {
        const text = String(message ?? ``)
        if (tone === `error`) toast.error(text)
        else toast(text)
      },
      harness_mcp: ({ tool, arguments: args }) => callMcp(String(tool), (args ?? {}) as Record<string, unknown>, claim()),
      // VAPP-120: every MCP operation as a typed function (fixtures/app-actions.json):
      // the same tools/call as the viewer, `op` + the args. EXP-1657: the
      // aliases too, so a package published before the split keeps working.
      ...Object.fromEntries(
        [...APP_ACTIONS, ...Object.keys(APP_ACTION_ALIASES)].map((name) => {
          const target = parseActionName(name)!
          return [name, (args: Record<string, unknown>) => callMcp(target.tool, target.op ? { ...args, op: target.op } : args, claim())]
        })
      ),
      // VAPP-161: the package format's record writes.
      records_upsert: recordsUpsert,
      records_delete: recordsDelete,
      records_batch: recordsBatch,
    }),
    policy: {
      functions: { ask: [`harness_mcp`, `exponential_*`] },
      // A granted typed action runs without the card; anything else asks.
      onFunctionCall: (call) => (parseActionName(call.name) && grantAllows(grantableTools(options.grantedTools?.()), call.name, call.args) ? true : options.consent.ask(call)),
      media: { baseUrl: origin, allowPrivate },
      // #EXP-1674: `~/x` = the app's team.
      urls: { baseUrl: origin, allowPrivate, ...(createTeamSlug ? { linkBase: `/t/${encodeURIComponent(createTeamSlug)}` } : {}) },
      // #EXP-1674: the ONE classifier: an in-app route navigates, a same-origin
      // resource and an external link leave the app. A link at another app's
      // screen passes that app's `exposed` contract (crossAppLink).
      openUrl: (url) => {
        const classified = classifyAppLink(url, origin)
        if (classified.kind === `ignore`) return
        const link = options.appId ? crossAppLink(classified, options.appId, targetApp) : classified
        if (!link) return void toast.error(LINK_NO_ACCESS)
        const path = appLinkPath(link)
        if (path && (link.kind !== `app` || isRoutedAppPath(link.path))) options.navigate(path)
        else window.open(link.kind === `external` ? link.url : url, `_blank`, `noopener,noreferrer`)
      },
      // #EXP-1674: a Row subtitle's `#IDENT` refs that resolve in the app's team.
      textLinks: (text) => {
        const team = teamCollection.toArray.find((t) => t.id === options.teamId)?.slug
        return issueTextLinks(text, team, (identifier) => Boolean(issueHref(identifier.toUpperCase())?.startsWith(`/t/${team}/`)))
      },
    },
  }, origin, upstreamAnswers)
  host.teamSlug = () => teamCollection.toArray.find((t) => t.id === scope.teamId)?.slug ?? null
  // VAPP-221 (apps.json `appFiles`): each picked file goes to the app's
  // upload route, one at a time; its `url` lands in the FileUpload's bound
  // list. A refusal = the failed-action toast; nothing uploads under Run as
  // (a read-only viewer's FileUpload is disabled and never picks).
  host.upload = async (files, target) => {
    if (runAs || !scope.appId) return
    for (const file of files) {
      let url: string
      try {
        const body = new FormData()
        body.append(APP_FILES.partName, file, file.name)
        const response = await fetch(appUploadPath(scope.appId), { method: `POST`, body, credentials: `same-origin` })
        const answer = (await response.json().catch(() => null)) as { url?: unknown; message?: unknown; error?: unknown } | null
        if (!response.ok || typeof answer?.url !== `string`) throw new Error(typeof answer?.message === `string` ? answer.message : typeof answer?.error === `string` ? answer.error : `HTTP ${response.status}`)
        url = answer.url
      } catch (error) {
        failedAction(APP_FILES.failedVerb, APP_FILES.failedThing, error)
        continue
      }
      for (const surfaceId of host.surfaceIds()) {
        const store = host.surface(surfaceId)
        const path = store ? appUploadTarget(store.components as unknown as Record<string, unknown>[], target.name) : null
        if (!store || !path) continue
        const next = withUploadedUrl(readPointer(store.data, path), file.name, url)
        if (next) store.setData(path, next)
        break
      }
    }
  }
  return host
}

/** VAPP-107 (the consent prompt's `slot`): what the card shows of a call:
 *  the tool it names, the arguments it carries (a `harness_mcp` call's MCP
 *  `arguments`, else the function's own args; an empty object = none, as
 *  compact JSON) and whether it sounds destructive. The desktop twin's
 *  `consent_words` derives the same three. */
export function consentWords(call: Pick<FunctionCallInfo, `name` | `args`>): { tool: string; args: string | null; destructive: boolean; title?: string } {
  // VAPP-120: a typed action is worded for its operation (the contract's rows).
  const typed = actionConsent(call.name, call.args)
  if (typed) return { tool: typed.tool, args: typed.args, destructive: typed.destructive, title: typed.title }
  const mcp = call.name === `harness_mcp`
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
      // EXP-1631: above the surface's own eUI Dialog/Drawer (z-51), so a call
      // raised from an app's entry dialog is answerable.
      className="z-[52]"
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
const NO_BANNERS: UpstreamHostBanner[] = []
const WarningIcon = conceptIcon(`ui-warning`)
/** The banner's action (upstream.json `binding.connect`). */
const UPSTREAM_CONNECT = UPSTREAM_BINDING.connect
const noSubscribe = () => () => {}

export function HostBanner({ host }: { host: ExponentialHost }) {
  const { status, unsupportedCatalog } = useHostStatus(host)
  const upstreams = host instanceof AppHost ? host.upstreams : null
  const connect = useSyncExternalStore(upstreams?.subscribe ?? noSubscribe, upstreams?.banners ?? (() => NO_BANNERS), () => NO_BANNERS)
  const offline = host.hasTransport && (status === `error` || status === `closed`)
  // #VAPP-230: the viewer has no connection to an upstream the app reads.
  if (connect.length > 0 && host instanceof AppHost) {
    const slug = host.teamSlug()
    return (
      <>
        {connect.map((banner) => (
          // The amber notice of ×4 (the offline banner's tone, icon + one ghost action).
          <div key={banner.server} role="status" className="flex items-center gap-2 rounded-md border border-yellow-400/40 bg-yellow-400/10 px-3 py-1.5 text-sm text-yellow-400 dark:text-yellow-400" data-testid="exponential-ui-upstream-connect">
            <WarningIcon className="size-3.5 shrink-0" />
            <span className="min-w-0 flex-1">{banner.text}</span>
            {slug && banner.connect ? (
              <Button variant="ghost" size="sm" className="h-6 px-2 text-yellow-400 hover:bg-yellow-400/15 dark:text-yellow-400" onClick={() => window.location.assign(`/t/${slug}/settings/mcp`)}>
                {UPSTREAM_CONNECT}
              </Button>
            ) : null}
          </div>
        ))}
      </>
    )
  }
  if (unsupportedCatalog)
    return (
      <p className="rounded-md border border-yellow-400/40 bg-yellow-400/10 px-3 py-2 text-sm text-yellow-400 dark:text-yellow-400" data-testid="exponential-ui-catalog-update">
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
