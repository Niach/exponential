// VAPP-269: the capability runtime, the twin of the crate's `capability.rs`.
// ONE `Broker` per app instance answers app UI calls (`photos_pick`) and
// agent calls (`photos__pick`) the same way: the package's manifest is the
// ceiling, the person's consent next (the host paints the sheet and answers
// through `answer`), then one audit row. Grants persist in the Vault and
// change ONLY through `setGrant` with `consentUi` after a gesture.
//
// USER ACTIVATION (the web): `call` and `answer` reach the native provider
// without awaiting anything but microtasks (the grants live in memory, the
// Vault write runs after), so a picker opens inside the click's task.

import type { HubOutcome } from "@exp/emcp"

import { brokerAuditRow, brokerDecide, type BrokerAuditRow, type BrokerCall, type BrokerVerdict, type CallFrom, type Grants, type GrantValue, type ManifestEntry } from "./capabilities"
import { consentAnswer, consentSheet, grantsRecord, grantWrite, manageSheet, type ConsentAnswer, type GrantVia, type SurfaceComponent } from "./consent"
import { bridgeCall, type BlobStore, type McpCallResult, type NativeCapability } from "./tools"

/** A refusal or a failure (eCore's shape: a code + a message). */
export interface Failure {
  code: string
  message: string
}

/** Where secrets live (Keychain, Keystore, libsecret; the web: IndexedDB). */
export interface Vault {
  read(key: string): Promise<string | null>
  write(key: string, value: string): Promise<void>
  delete(key: string): Promise<void>
}

/** Where the broker's audit rows go. */
export interface CallAudit {
  record(row: BrokerAuditRow): void
}

/** The ask sheet the person answers (consent.json `consentSheet`): a genUI surface. */
export interface ConsentSheet {
  scope: string
  components: SurfaceComponent[]
  data: Record<string, unknown>
}

/** What a brokered call answers. */
export type BrokerAnswer =
  | { result: McpCallResult }
  /** The person decides first: paint the sheet, then `answer`. */
  | { ask: BrokerVerdict; sheet: ConsentSheet }
  /** `code` = `UNKNOWN_TOOL` for unknown, else `FORBIDDEN`; `message` = the reason. */
  | { refused: Failure }

export interface BrokerSetup {
  appId: string
  appName: string
  signer: string
  manifest: readonly ManifestEntry[]
  grants: Grants
  capabilities: readonly NativeCapability[]
  vault: Vault
  /** The app's files: what capabilities read (`fileId`) and store. */
  blobs: BlobStore
  /** The package's screen names (a notification's tap opens one). */
  screens: readonly string[]
  audit?: CallAudit | null
}

const refused = (verdict: BrokerVerdict): BrokerAnswer => ({ refused: { code: verdict.verdict === `unknown` ? `UNKNOWN_TOOL` : `FORBIDDEN`, message: verdict.reason ?? `` } })

/** One app instance's broker. */
export class Broker {
  private grantsNow: Grants
  private foreground = false
  /** The last grants write (tests and a closing page await it). */
  persisted: Promise<void> = Promise.resolve()

  constructor(private readonly setup: BrokerSetup) {
    this.grantsNow = { ...setup.grants }
  }

  get appId(): string {
    return this.setup.appId
  }

  get manifest(): readonly ManifestEntry[] {
    return this.setup.manifest
  }

  get capabilities(): readonly NativeCapability[] {
    return this.setup.capabilities
  }

  grants(): Grants {
    return { ...this.grantsNow }
  }

  /** The lifecycle tells the broker whether a sheet may show. */
  setForeground(foreground: boolean): void {
    this.foreground = foreground
  }

  private provided(): string[] {
    return this.setup.capabilities.map((c) => c.name)
  }

  private record(client: string | null, call: BrokerCall, verdict: BrokerVerdict, outcome: HubOutcome | null): void {
    this.setup.audit?.record(brokerAuditRow({ appId: this.setup.appId, client, call, verdict, outcome }))
  }

  private async run(client: string | null, call: BrokerCall, verdict: BrokerVerdict): Promise<BrokerAnswer> {
    const capability = this.setup.capabilities.find((c) => c.name === verdict.capability)
    if (!capability || !verdict.tool) {
      this.record(client, call, verdict, `unreachable`)
      return { refused: { code: `FORBIDDEN`, message: `unavailable` } }
    }
    const { result, outcome } = await bridgeCall({
      tool: `${capability.name}_${verdict.tool}`,
      arguments: call.arguments ?? {},
      context: { app: this.setup.appId, screens: this.setup.screens },
      blobs: this.setup.blobs,
      native: (tool, args) => capability.call(tool.slice(tool.indexOf(`_`) + 1), args),
    })
    this.record(client, call, verdict, outcome)
    return { result }
  }

  /** One call from app UI (`client` = null) or an agent run (`client` = its run id). */
  call(from: CallFrom, client: string | null, name: string, args: unknown): Promise<BrokerAnswer> {
    const call: BrokerCall = { from, name, arguments: args }
    const verdict = brokerDecide({ manifest: this.setup.manifest, grants: this.grantsNow, provided: this.provided(), foreground: this.foreground, call })
    if (verdict.verdict === `call`) return this.run(client, call, verdict)
    if (verdict.verdict === `ask`) {
      const entry = this.setup.manifest.find((m) => m.id === verdict.scope) ?? { id: verdict.scope ?? ``, why: `` }
      const { components, data } = consentSheet(this.setup.appName, entry)
      return Promise.resolve({ ask: verdict, sheet: { scope: entry.id, components, data } })
    }
    this.record(client, call, verdict, null)
    return Promise.resolve(refused(verdict))
  }

  /** The person's answer to the sheet for `verdict` (an Ask): store it per
   *  consent.json, then run or refuse the call. The answer is the gesture. */
  answer(from: CallFrom, client: string | null, name: string, args: unknown, verdict: BrokerVerdict, answer: ConsentAnswer): Promise<BrokerAnswer> {
    const call: BrokerCall = { from, name, arguments: args }
    if (!verdict.scope) return Promise.resolve(refused(verdict))
    const next = consentAnswer(this.grantsNow, verdict.scope, answer)
    this.grantsNow = next.grants
    this.persist(next.grants)
    const decided: BrokerVerdict = { ...verdict, verdict: next.verdict, reason: next.reason }
    if (next.verdict === `call`) return this.run(client, call, decided)
    this.record(client, call, decided, null)
    return Promise.resolve(refused(decided))
  }

  /** Change a grant (the Permissions sheet's Switch). Only the ConsentUi
   *  after a gesture; `value` null = revoke. Answers consent.json's refusal, or null. */
  setGrant(via: GrantVia, gesture: boolean, scope: string, value: GrantValue | null): string | null {
    const next = grantWrite(this.setup.manifest, this.grantsNow, { via, gesture, scope, value })
    if (next.refused) return next.refused
    this.grantsNow = next.grants
    this.persist(next.grants)
    return null
  }

  /** The Permissions sheet (consent.json `manageSheet`). */
  permissionsSheet(): SurfaceComponent[] {
    return manageSheet(this.setup.manifest, this.grantsNow)
  }

  private persist(grants: Grants): void {
    const { key, value } = grantsRecord(this.setup.appId, this.setup.signer, this.setup.manifest, grants)
    // A failed write only means the person is asked again next launch.
    this.persisted = this.persisted.then(() => this.setup.vault.write(key, value)).catch(() => {})
  }
}
