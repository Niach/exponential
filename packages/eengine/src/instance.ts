// VAPP-269: an `AppInstance`, one running app over a host's providers, the
// twin of the crate's `instance.rs`: load and verify its package file,
// drive its lifecycle (lifecycle.json) and carry out the effects, resolve
// `records:<kind>` sources with genUI's one records query, dispatch
// `records_*`, `app_navigate` and capability calls (the broker, the E5
// bridge), and keep O1's outbox when the host has a sync provider
// (outbox.json: direct writes otherwise).

import {
  kindComputed,
  kindRefs,
  kindTransitions,
  navigateTarget,
  packageScreens,
  parseRecordsSource,
  parseSource,
  recordPolicy,
  recordsValue,
  bridgePackage,
  validatePackage,
  verifyPackageFile,
  type PackageIssue,
  type RecordOp,
  type RecordRow,
  type RecordsValue,
} from "@exponential_app/eui/host"

import { Broker, type BrokerAnswer, type CallAudit, type ConsentSheet, type Failure, type Vault } from "./broker"
import type { BrokerVerdict, Grants, GrantValue, ManifestEntry } from "./capabilities"
import { grantsOnPackage, grantsRecord, type ConsentAnswer, type GrantVia, type SurfaceComponent } from "./consent"
import { lifecycleStep, STOPPED, type LifecycleEffect, type LifecycleEvent, type LifecycleState } from "./lifecycle"
import { outboxFlush, outboxMode, outboxOutcome, outboxPrune, outboxRequest, outboxRows, outboxWrites, type OutboxCall, type OutboxEntry, type OutboxResult, type OutboxWrite } from "./outbox"
import { engineRecordRefusal, engineViewer, type EngineIdentity, type NetworkPolicy } from "./providers"
import type { EngineRecordStore, WriteBatch } from "./stores"
import type { BlobStore, NativeCapability } from "./tools"

export type Presentation = `replace` | `push` | `sheet`

/** Maps screens to windows (desktop), nav stacks and sheets (phones), the page (the web). */
export interface WindowHost {
  open(app: string, screen: string, params: Record<string, string>, how: Presentation): void
}

/** An optional transport to an authority; absent = the store is the authority, no outbox. */
export interface SyncTransport {
  push(app: string, call: OutboxCall, input: Record<string, unknown>): Promise<OutboxResult>
  /** `push` with the server's answer, for a call that awaits its own entry. */
  send?(app: string, call: OutboxCall, input: Record<string, unknown>): Promise<Pushed>
}

/** One replay as a transport answers it. */
export interface Pushed {
  result: OutboxResult
  answer?: unknown
  message?: string
  invalid?: boolean
}

/** What a host hands the Engine (providers.json; no telemetry or update key: none exists). */
export interface EngineHostConfig {
  host: { kind: string; platform: string; formFactor: string; locale: string }
  storage: EngineRecordStore
  blobs: BlobStore
  identity: EngineIdentity
  vault: Vault
  window: WindowHost
  /** ISO 8601 UTC with milliseconds (tests pin it). */
  clock?: () => string
  network: NetworkPolicy
  sync?: SyncTransport | null
  capabilities: readonly NativeCapability[]
  audit?: CallAudit | null
  newId?: () => string
}

/** Why a package could not load: `signature` (signed, does not verify) or `invalid` (genUI refuses it). */
export class LoadError extends Error {
  constructor(
    readonly kind: `signature` | `invalid`,
    readonly issues: PackageIssue[],
  ) {
    super(`${kind}: ${issues.map((i) => `${i.path || `/`} ${i.message}`).join(`; `)}`)
    this.name = `LoadError`
  }
}

/** What a host function call answers. */
export type CallResult =
  | { value: unknown }
  /** The person decides first: paint the sheet, then `answerConsent` with the same call. */
  | { ask: BrokerVerdict; sheet: ConsentSheet }
  /** Refused (the failed-action toast's reason). */
  | { refused: Failure }

/** Where the instance keeps its last screen (lifecycle.json `persist`). */
export const screenKey = (app: string): string => `eengine/screen/${app}`

type Pkg = Record<string, unknown> & { id?: unknown; name?: unknown; records?: unknown; capabilities?: unknown; screens?: unknown }

const isObject = (v: unknown): v is Record<string, unknown> => !!v && typeof v === `object` && !Array.isArray(v)

/** The package a file carries (`package`), or the value itself when it is a bare package. */
export const packageOf = (file: unknown): unknown => (isObject(file) && isObject(file.package) ? file.package : file)

/** VAPP-271: app UI reads a capability's answer as its `structuredContent`
 *  (an action step's `results.<name>`); an `isError` result is the call's
 *  refusal, its text the toast. */
const brokered = (answer: BrokerAnswer): CallResult => {
  if (!(`result` in answer)) return answer
  const { result } = answer
  if (result.isError) return { refused: { code: `failed`, message: result.content[0]?.text ?? `failed` } }
  return { value: result.structuredContent ?? null }
}

const failure = (code: string, error: unknown): Failure =>
  isObject(error) && typeof error.code === `string` && typeof error.message === `string` ? { code: error.code, message: error.message } : { code, message: error instanceof Error ? error.message : String(error) }

const parseLast = (stored: string | null): { screen: string; params: Record<string, unknown> } | null => {
  if (!stored) return null
  try {
    const v = JSON.parse(stored)
    return isObject(v) && typeof v.screen === `string` && isObject(v.params) ? { screen: v.screen, params: v.params } : null
  } catch {
    return null
  }
}

/** One running app. */
export class AppInstance {
  private stateNow: LifecycleState = STOPPED
  private outboxNow: OutboxEntry[] = []
  private lastScreen: string | null

  private constructor(
    private readonly config: EngineHostConfig,
    readonly pkg: Pkg,
    readonly appId: string,
    readonly signer: string | null,
    readonly broker: Broker,
    lastScreen: string | null,
  ) {
    this.lastScreen = lastScreen
  }

  /** Load a package file (or a bare package): verify its signature when it
   *  carries one, validate the package, reconcile its grants. */
  static async load(config: EngineHostConfig, file: unknown): Promise<AppInstance> {
    const verification = await verifyPackageFile(file)
    if (verification.status === `invalid`) throw new LoadError(`signature`, verification.issues)
    // Round 9 §12: format 5 through the bridge while ACCEPT_V09 holds.
    const pkg = bridgePackage(packageOf(file))
    const issues = validatePackage(pkg)
    if (issues.length) throw new LoadError(`invalid`, issues)
    const p = pkg as Pkg
    const appId = typeof p.id === `string` ? p.id : ``
    const signer = verification.status === `valid` ? verification.author.key : null
    const manifest: ManifestEntry[] = Array.isArray(p.capabilities) ? (p.capabilities as ManifestEntry[]) : []
    const key = grantsRecord(appId, signer ?? ``, manifest, {}).key
    const [storedGrants, lastScreen] = await Promise.all([config.vault.read(key).catch(() => null), config.vault.read(screenKey(appId)).catch(() => null)])
    const broker = new Broker({
      appId,
      appName: typeof p.name === `string` ? p.name : appId,
      signer: signer ?? ``,
      manifest,
      grants: grantsOnPackage(storedGrants, signer ?? ``, manifest),
      capabilities: config.capabilities,
      vault: config.vault,
      blobs: config.blobs,
      screens: packageScreens(p).map((s) => s.template),
      audit: config.audit ?? null,
    })
    return new AppInstance(config, p, appId, signer, broker, lastScreen)
  }

  get manifest(): readonly ManifestEntry[] {
    return this.broker.manifest
  }

  grants(): Grants {
    return this.broker.grants()
  }

  get state(): LifecycleState {
    return this.stateNow
  }

  /** The outbox entries still pending or acked (queue mode). */
  get outbox(): readonly OutboxEntry[] {
    return this.outboxNow
  }

  get mode(): `queue` | `direct` {
    return outboxMode(!!this.config.sync)
  }

  private me(): string {
    return engineViewer(this.config.identity).id ?? ``
  }

  private now(): string {
    return this.config.clock?.() ?? new Date().toISOString()
  }

  /** The person answered the sheet of an Ask for this call (the answer is the gesture). */
  async answerConsent(name: string, args: unknown, verdict: BrokerVerdict, answer: ConsentAnswer): Promise<CallResult> {
    return brokered(await this.broker.answer(`ui`, null, name, args, verdict, answer))
  }

  /** The Permissions sheet's Switch: only the ConsentUi, after a gesture. Answers the refusal, or null. */
  setGrant(via: GrantVia, gesture: boolean, scope: string, value: GrantValue | null): string | null {
    return this.broker.setGrant(via, gesture, scope, value)
  }

  /** The Permissions sheet (a genUI surface). */
  permissionsSheet(): SurfaceComponent[] {
    return this.broker.permissionsSheet()
  }

  /** One lifecycle event: the next state, with its effects carried out
   *  (open → the WindowHost, persist → the Vault, flush → the outbox).
   *  Answers the effects (sources and automations are the host's). */
  async event(event: LifecycleEvent): Promise<LifecycleEffect[]> {
    const { state, effects } = lifecycleStep(this.stateNow, event, { pkg: { id: this.appId, screens: this.pkg.screens }, lastScreen: parseLast(this.lastScreen) })
    this.stateNow = state
    this.broker.setForeground(state.phase === `foreground`)
    for (const effect of effects) {
      if (effect.effect === `open`) this.config.window.open(this.appId, effect.screen, effect.params, `replace`)
      else if (effect.effect === `persist`) {
        this.lastScreen = JSON.stringify({ screen: effect.screen, params: effect.params })
        await this.config.vault.write(screenKey(this.appId), this.lastScreen).catch(() => {})
      } else if (effect.effect === `flush`) await this.flush()
    }
    return effects
  }

  /** A `records:<kind>?…` source's value (genUI's records query over the
   *  store's rows with the outbox laid over them); undefined = not a records source. */
  async recordsValue(source: string): Promise<RecordsValue | null | undefined> {
    const parsed = parseSource(source)
    const query = parsed ? parseRecordsSource(parsed) : null
    if (!query) return undefined
    if (query.app !== null) return null
    let rows: RecordRow[] = await this.config.storage.rows(this.appId)
    if (this.outboxNow.length) rows = outboxRows(rows, this.outboxNow, this.me())
    const records = this.pkg.records
    const def = query.from && isObject(records) ? records[query.from] : undefined
    const viewer = engineViewer(this.config.identity)
    return recordsValue(rows, query, {
      me: viewer.id,
      now: this.now(),
      names: {},
      computed: kindComputed(records, query.from),
      refs: kindRefs(records, query.from),
      transitions: kindTransitions(def),
      role: viewer.role,
    })
  }

  /** Called with every change to the app's rows (re-resolve the bound sources). */
  subscribe(on: () => void): () => void {
    return this.config.storage.subscribe(this.appId, () => on())
  }

  /** A host function call from the app's UI: `records_upsert` /
   *  `records_delete` / `records_batch`, `app_navigate`, or a capability
   *  (`<capability>.<tool>`, through the broker). `newIds` = the client ids of
   *  its creates, in order. */
  async call(name: string, args: unknown, newIds: readonly string[] = []): Promise<CallResult> {
    const a = isObject(args) ? args : {}
    switch (name) {
      case `records_upsert`:
        return this.write(`upsert`, a, newIds)
      case `records_delete`:
        return this.write(`delete`, a, newIds)
      case `records_batch`:
        return this.write(`batch`, a, newIds)
      case `app_navigate`: {
        const target = navigateTarget(this.pkg, args)
        if (`error` in target) return { refused: { code: `BAD_REQUEST`, message: target.error } }
        this.config.window.open(this.appId, target.screen, target.params, `replace`)
        this.stateNow = { ...this.stateNow, screen: target.screen, params: target.params }
        return { value: null }
      }
      default:
        return brokered(await this.broker.call(`ui`, null, name, args))
    }
  }

  /** The kind policy check for a write, as the identity sees it. */
  private async refusal(writes: OutboxWrite[]): Promise<string | null> {
    const rows = await this.config.storage.rows(this.appId).catch(() => [] as RecordRow[])
    const records = isObject(this.pkg.records) ? this.pkg.records : {}
    for (const w of writes) {
      const row = rows.find((r) => r.id === w.id)
      const op: RecordOp = w.op
      const kind = w.op === `delete` ? row?.kind : w.kind
      const policy = recordPolicy(kind ? records[kind] : undefined)
      const refusal = engineRecordRefusal(this.config.identity, policy, op, w.op === `create` ? null : row ? { authorId: row.authorId ?? null, createdBy: row.createdBy } : null)
      if (refusal) return refusal
    }
    return null
  }

  private async write(call: OutboxCall, args: Record<string, unknown>, newIds: readonly string[]): Promise<CallResult> {
    const writes = outboxWrites(call, args, newIds)
    const refusal = await this.refusal(writes)
    if (refusal) return { refused: { code: `FORBIDDEN`, message: refusal } }
    const entry: OutboxEntry = { writeId: this.config.newId?.() ?? crypto.randomUUID(), appId: this.appId, call, writes, at: this.now(), state: `pending` }
    const answer = call === `upsert` ? { id: writes[0]?.id } : call === `delete` ? null : { ids: writes.map((w) => w.id) }
    if (this.mode === `direct`) {
      try {
        await this.config.storage.write(this.appId, batchOf(entry), this.me())
        return { value: answer }
      } catch (error) {
        return { refused: failure(`STORE`, error) }
      }
    }
    this.outboxNow = [...this.outboxNow, entry]
    if (this.stateNow.phase === `foreground`) await this.flush()
    return { value: answer }
  }

  /** Flush the outbox through the sync provider (FIFO, a retry stops it). */
  async flush(): Promise<void> {
    const sync = this.config.sync
    if (!sync) return
    const results: Record<string, OutboxResult> = {}
    for (const entry of this.outboxNow.filter((e) => e.state === `pending`)) {
      const { call, input } = outboxRequest(entry)
      const result = await sync.push(this.appId, call, input).catch((): OutboxResult => ({ transport: true }))
      results[entry.writeId] = result
      if (outboxOutcome(entry, result) === `retry`) break
    }
    if (!Object.keys(results).length) return
    const now = this.now()
    const after = outboxFlush(this.outboxNow, results, now).after
    // Acked entries keep overlaying until the store reflects them (outboxPrune).
    const rows = await this.config.storage.rows(this.appId).catch(() => [] as RecordRow[])
    this.outboxNow = outboxPrune(after, rows, now)
  }
}

/** An outbox entry as one store batch. */
export function batchOf(entry: OutboxEntry): WriteBatch {
  return {
    writeId: entry.writeId,
    writes: entry.writes.map((w) => (w.op === `delete` ? { op: `delete`, id: w.id } : { op: `upsert`, id: w.id, kind: w.kind, data: w.data, create: w.op === `create` })),
    at: entry.at,
  }
}

/** How many client ids a call's creates take (the runner generates them). */
export function createsOf(name: string, args: unknown): number {
  if (!isObject(args)) return 0
  if (name === `records_upsert`) return typeof args.id === `string` ? 0 : 1
  if (name === `records_batch` && Array.isArray(args.writes)) return args.writes.filter((w) => isObject(w) && w.op !== `delete` && typeof w.id !== `string`).length
  return 0
}
