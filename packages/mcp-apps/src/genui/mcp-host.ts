// #VAPP-292 — a team app's host INSIDE an MCP Apps host (Claude, ChatGPT,
// the ext-apps basic host). The web run view's host (apps/web
// lib/exponential-ui-host.tsx) reads the synced Electric collections; this
// iframe has no network of its own, so every read and write is a
// `tools/call` the MCP Apps host forwards to Exponential's /api/mcp as the
// connected person:
//   - `records:<kind>?…`  exponential_apps records_list (the app's rows),
//                         evaluated by THE eUI evaluator (`recordsValue`)
//                         with the members' names, the team zone's today,
//                         the kind's computed fields and transitions;
//   - `exp:<source>`      boards / issues / people / sessions lists into the
//                         contract's SourceStore (`sourceValue`); comments
//                         and attachments read empty here;
//   - records_upsert / records_delete / records_batch → exponential_apps;
//   - harness_mcp + the typed actions → the named tool, behind the SAME
//     consent rule as web (a granted, non-destructive action runs; anything
//     else asks: `consent`);
//   - app.navigate → the view's screen shell; a link → `ui/open-link`.
// Every write re-reads the bindings (the web host repaints off sync).

import { ExponentialHost, type AppPackage, type FunctionCallInfo, type HostFunction, type HostOptions, type MediaRequest, type ParsedSource, type SourceEmit } from "@exponential_app/eui"
import { RECORDS_SCHEME, kindComputed, kindRefs, parseRecordsQuery, recordsValue, type RecordRow } from "@exponential_app/eui/host/query"
import { kindTransitions } from "@exponential_app/eui/host"
import { APP_SOURCE_KINDS, sourceValue, type SourceStore } from "@exp/domain-contract/app-sources"
import { APP_ACTIONS, APP_ACTION_ALIASES, actionGrantable, grantAllows, parseActionName } from "@exp/domain-contract/app-actions"
import { appMediaAllowed } from "@exp/domain-contract/apps"
import { hostZone, zoneDay } from "@exp/domain-contract/team-zone"
import { appExtensionCatalog } from "@exp/ui"
import { toolJson, type GenuiAppRead } from "./result"

export interface ToolAnswer {
  content?: Array<{ type: string; text?: string }>
  structuredContent?: unknown
  isError?: boolean
}

export type CallTool = (name: string, args: Record<string, unknown>) => Promise<ToolAnswer>

/** A tool's JSON, or its error as a throw. */
export async function callJson<T = unknown>(call: CallTool, name: string, args: Record<string, unknown>): Promise<T> {
  const decoded = toolJson(await call(name, args))
  if (!decoded.ok) throw new Error(decoded.message)
  return decoded.data as T
}

/** The rows one `records_list` page may carry (records-query.json limitMax). */
const RECORDS_PAGE = 500
/** Pages read per app (the web shape's on-demand subset has no cap; a
 *  conversation's view stops at this many rows). */
const RECORDS_PAGES = 4
const LIST_LIMIT = 200

/** The team's data the bindings read, fetched once and re-read after a
 *  write (`invalidate`); every live binding re-emits then. */
export class McpDataStore {
  private cache = new Map<string, Promise<unknown>>()
  private listeners = new Set<() => void>()
  constructor(
    private readonly call: CallTool,
    readonly teamId: string
  ) {}

  private once<T>(key: string, load: () => Promise<T>): Promise<T> {
    let hit = this.cache.get(key) as Promise<T> | undefined
    if (!hit) {
      hit = load()
      // A failed read is retried on the next ask.
      hit.catch(() => this.cache.delete(key))
      this.cache.set(key, hit)
    }
    return hit
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => void this.listeners.delete(listener)
  }

  /** A write landed: every read is stale. */
  invalidate(): void {
    this.cache.clear()
    for (const listener of [...this.listeners]) listener()
  }

  /** An app's rows (the hosting app's, or another's of the team by slug). */
  records(slug: string): Promise<RecordRow[]> {
    return this.once(`records:${slug}`, async () => {
      const rows: RecordRow[] = []
      for (let page = 0; page < RECORDS_PAGES; page++) {
        const answer = await callJson<{ records?: RecordRow[] }>(this.call, `exponential_apps`, { op: `records_list`, slug, teamId: this.teamId, limit: RECORDS_PAGE, offset: page * RECORDS_PAGE })
        const batch = Array.isArray(answer?.records) ? answer.records : []
        rows.push(...batch)
        if (batch.length < RECORDS_PAGE) break
      }
      return rows
    })
  }

  members(): Promise<Array<{ id: string; name: string; email: string | null; image: string | null; role: string }>> {
    return this.once(`members`, async () => {
      const rows = await callJson<unknown>(this.call, `exponential_people`, { op: `list`, teamId: this.teamId, limit: LIST_LIMIT })
      return Array.isArray(rows) ? (rows as Array<{ id: string; name: string; email: string | null; image: string | null; role: string }>) : []
    })
  }

  /** The team zone (`teams get` → timeZone), null = the device's. */
  zone(): Promise<string | null> {
    return this.once(`zone`, async () => {
      const team = await callJson<{ timeZone?: string | null } | null>(this.call, `exponential_teams`, { op: `get`, id: this.teamId }).catch(() => null)
      return team?.timeZone ?? null
    })
  }

  /** One `exp:` table's rows (each read once, until a write). */
  private table(name: SourceTable): Promise<Record<string, unknown>[]> {
    return this.once(`exp:${name}`, async () => {
      if (name === `members`) return (await this.members()) as unknown as Record<string, unknown>[]
      const [tool, args] = SOURCE_READS[name]
      const rows = await callJson<unknown>(this.call, tool, { op: `list`, teamId: this.teamId, ...args }).catch(() => [])
      return Array.isArray(rows) ? (rows as Record<string, unknown>[]) : []
    })
  }

  /** `exp:<name>`'s rows (fixtures/app-sources.json's SourceStore): only
   *  the tables that source reads (the web host's SOURCE_TABLES). */
  async sourceStore(name: string): Promise<SourceStore> {
    const tables = SOURCE_TABLES[name] ?? []
    const read = (t: SourceTable) => (tables.includes(t) ? this.table(t) : Promise.resolve([]))
    const [boards, issues, members, sessions] = await Promise.all([read(`boards`), read(`issues`), read(`members`), read(`sessions`)])
    const people = members as unknown as Array<{ id: string; name: string; email: string | null; image: string | null; role: string }>
    return {
      boards: boards.map((b) => ({ deletedAt: null, sortOrder: 0, icon: null, color: null, ...b }) as unknown as SourceStore[`boards`][number]),
      issues: issues as unknown as SourceStore[`issues`],
      members: people.map((m) => ({ userId: m.id, role: m.role })),
      users: people.map((m) => ({ id: m.id, name: m.name, email: m.email ?? null, image: m.image ?? null })),
      sessions: sessions as unknown as SourceStore[`sessions`],
      comments: [],
      attachments: [],
    }
  }
}

type SourceTable = `boards` | `issues` | `members` | `sessions`
/** What each `exp:` source reads (comments / attachments: none over MCP yet). */
const SOURCE_TABLES: Record<string, readonly SourceTable[]> = {
  issues: [`issues`, `boards`, `members`],
  boards: [`boards`],
  members: [`members`],
  me: [`members`],
  sessions: [`sessions`],
  comments: [`issues`, `boards`, `members`],
  attachments: [`issues`, `boards`, `members`],
}
const SOURCE_READS: Record<Exclude<SourceTable, `members`>, [string, Record<string, unknown>]> = {
  boards: [`exponential_boards`, { limit: LIST_LIMIT }],
  issues: [`exponential_issues`, { includeClosed: true, limit: 1000 }],
  sessions: [`exponential_sessions`, { limit: LIST_LIMIT }],
}

/** One pending consent (the web host's card, answered Allow / Deny). */
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
    current: () => pending?.call ?? null,
    answer(ok: boolean) {
      const was = pending
      pending = null
      was?.resolve(ok)
      emit()
    },
    subscribe(listener: () => void) {
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
  }
}
export type ConsentGate = ReturnType<typeof createConsentGate>

export interface McpAppHostOptions {
  app: GenuiAppRead
  call: CallTool
  /** The Exponential instance (attachments, in-app links). */
  origin: string
  consent: ConsentGate
  navigateScreen?: HostOptions[`navigate`]
  openLink: (url: string) => void
  notify: (text: string, tone?: `error`) => void
  /** Tests: the clock. */
  now?: () => number
}

/** The media rule of every app host (apps.json `mediaPolicy`). */
class McpAppHost extends ExponentialHost {
  constructor(
    options: HostOptions,
    private readonly appOrigin: string,
    readonly store: McpDataStore
  ) {
    super(options)
  }
  override mediaRequest(url: string): MediaRequest | null {
    return appMediaAllowed(url, this.appOrigin) ? super.mediaRequest(url) : null
  }
}
export type { McpAppHost }

/** A source that re-reads on every invalidation; a value equal to the last
 *  is not re-emitted. */
function liveSource(store: McpDataStore, emit: SourceEmit, read: () => Promise<unknown>): () => void {
  let live = true
  let last: string | null = null
  const push = () =>
    void read()
      .then((value) => {
        if (!live) return
        const key = JSON.stringify(value)
        if (key === last) return
        last = key
        emit(value)
      })
      .catch(() => {
        // A refused read keeps the binding's default.
      })
  const off = store.subscribe(push)
  push()
  return () => {
    live = false
    off()
  }
}

/** `/t/<team>` of an app's url (`…/t/<team>/apps/<slug>`), null when absent. */
export function teamPathOf(url: string | null | undefined): string | null {
  const match = url ? /\/t\/[^/?#]+/.exec(new URL(url, `http://x`).pathname) : null
  return match ? match[0] : null
}

export function createMcpAppHost(options: McpAppHostOptions): McpAppHost {
  const { app, call } = options
  const pkg = app.package as unknown as AppPackage
  const kinds = (app.package as { records?: Record<string, unknown> } | null)?.records
  const me = app.viewer?.id ?? null
  const access = app.viewer?.access ?? `write`
  const store = new McpDataStore(call, app.teamId)
  const now = options.now ?? Date.now
  const role = async () => (await store.members()).find((m) => m.id === me)?.role ?? null

  const recordsSource = (source: ParsedSource, emit: SourceEmit) => {
    const query = parseRecordsQuery(source.params, source.name)
    return liveSource(store, emit, async () => {
      const [rows, members, zone] = await Promise.all([store.records(query.app ?? app.slug), store.members(), store.zone()])
      const names: Record<string, string> = Object.fromEntries(members.map((m) => [m.id, m.name]))
      const computed = query.app ? undefined : kindComputed(kinds, query.from)
      const refs = query.app ? undefined : kindRefs(kinds, query.from)
      let issues: Record<string, Record<string, unknown>> | undefined
      if (refs && query.expand.some((f) => refs[f] === `issue`)) {
        const all = (await store.sourceStore(`issues`)).issues
        issues = Object.fromEntries(all.map((i) => [i.identifier, { id: i.id, identifier: i.identifier, title: i.title, status: i.status }]))
      }
      const transitions = query.app || !query.from ? null : kindTransitions(kinds?.[query.from])
      const ms = now()
      const offset = hostZone(zone)
      return recordsValue(rows, query, { me, now: new Date(ms).toISOString(), today: zoneDay(offset, ms), names, computed, refs, issues, transitions, role: transitions ? await role() : null })
    })
  }

  const expSource = (source: ParsedSource, emit: SourceEmit) => {
    if (!APP_SOURCE_KINDS.includes(source.name)) {
      emit(null)
      return
    }
    return liveSource(store, emit, async () => sourceValue(source.name, source.params, await store.sourceStore(source.name), { me, access }))
  }

  /** A write through the app's tools, then every binding re-reads. */
  const write = async (args: Record<string, unknown>) => {
    try {
      return await callJson<Record<string, unknown>>(call, `exponential_apps`, { slug: app.slug, teamId: app.teamId, ...args })
    } catch (error) {
      options.notify(error instanceof Error ? error.message : String(error), `error`)
      throw error
    } finally {
      store.invalidate()
    }
  }
  const recordsUpsert: HostFunction = async ({ kind, data, id, author }) => {
    const out = await write({ op: `records_upsert`, kind: String(kind ?? ``), data: data && typeof data === `object` ? data : {}, ...(typeof id === `string` ? { id } : {}), ...(typeof author === `string` ? { author } : {}) })
    return { id: out?.id }
  }
  const recordsDelete: HostFunction = async ({ id }) => {
    await write({ op: `records_delete`, id: String(id ?? ``) })
  }
  // A package action's writes, one call each (MCP has no batch op: the web
  // host's one transaction is the server's records.batch).
  const recordsBatch: HostFunction = async ({ writes }) => {
    const ids: unknown[] = []
    for (const w of (Array.isArray(writes) ? writes : []) as Array<Record<string, unknown>>) {
      if (w.op === `delete`) {
        await write({ op: `records_delete`, id: String(w.id ?? ``) })
        ids.push(w.id)
      } else {
        const out = await write({ op: `records_upsert`, kind: String(w.kind ?? ``), data: w.data ?? {}, ...(typeof w.id === `string` ? { id: w.id } : {}), ...(typeof w.author === `string` ? { author: w.author } : {}) })
        ids.push(out?.id)
      }
    }
    return { ids }
  }
  const tool = async (name: string, args: Record<string, unknown>) => {
    try {
      return await callJson(call, name, args)
    } finally {
      store.invalidate()
    }
  }
  const granted = (app.grantedTools ?? []).filter((name) => actionGrantable(name))
  // Dates in the TEAM zone once it is read (the device's until then).
  let zone = hostZone(null)
  void store.zone().then((name) => (zone = hostZone(name)), () => undefined)
  // `~/x` = the app's team (`/t/<slug>`, off the app's own url).
  const teamPath = teamPathOf(app.url)

  return new McpAppHost(
    {
      me,
      zone: (ms) => zone(ms),
      onActionFailed: ({ message }) => options.notify(message, `error`),
      navigate: options.navigateScreen,
      extensions: [appExtensionCatalog],
      packages: [pkg],
      sources: {
        exp: expSource,
        [RECORDS_SCHEME]: recordsSource,
      },
      functions: {
        harness_toast: ({ message, tone }) => options.notify(String(message ?? ``), tone === `error` ? `error` : undefined),
        harness_mcp: ({ tool: name, arguments: args }) => tool(String(name), (args ?? {}) as Record<string, unknown>),
        ...Object.fromEntries(
          [...APP_ACTIONS, ...Object.keys(APP_ACTION_ALIASES)].map((name) => {
            const target = parseActionName(name)!
            return [name, (args: Record<string, unknown>) => tool(target.tool, target.op ? { ...args, op: target.op } : args)]
          })
        ),
        records_upsert: recordsUpsert,
        records_delete: recordsDelete,
        records_batch: recordsBatch,
      },
      policy: {
        functions: { ask: [`harness_mcp`, `exponential_*`] },
        onFunctionCall: (info) => (parseActionName(info.name) && grantAllows(granted, info.name, info.args) ? true : options.consent.ask(info)),
        media: { baseUrl: options.origin },
        urls: { baseUrl: options.origin, ...(teamPath ? { linkBase: teamPath } : {}) },
        openUrl: (url) => options.openLink(new URL(url, options.origin).href),
      },
    },
    options.origin,
    store
  )
}
