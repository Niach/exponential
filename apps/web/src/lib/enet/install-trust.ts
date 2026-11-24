// #EXP-1950 (fixtures/install-trust.json): this browser install's side of
// install approval and the four acceptance rules, around the pure reference
// `@exp/domain-contract/install-trust`. It follows `installs.trust` into
// the pin store (trust-store.ts), claims its account's root when there is no
// chain, asks once to be approved when there is one, and answers for the key
// holder (key-holder.ts) whether a wrap is kept (R1), a key sealed to (R2,
// R3) and a target wrapped for (R4). Release 1 is RECORD-ONLY (`TRUST_MODE`)
// for keys; an install that asked and is not approved gets no wraps from it.
// It APPROVES only when a person presses Approve (`approve`), never by itself.
// An older server without `installs.trust` (NOT_FOUND): `unavailable`, every
// target is wrapped as before and recorded `legacy`.

import {
  emptyTrustStore,
  keepWrap,
  reconcileTrust,
  recordKey,
  sealTo,
  signApproval,
  TRUST_MODE,
  wrapTarget,
  type ApprovalKind,
  type KeepVerdict,
  type KeyRecord,
  type SealVerdict,
  type ServedApproval,
  type ServedInstall,
  type TargetVerdict,
  type TrustMode,
  type TrustRow,
  type TrustStatus,
  type TrustStore,
} from "@exp/domain-contract/install-trust"
import { endpointIdOf, type ServedWrap } from "@exp/domain-contract/wrap-signature"
import type { TrustStoreIO } from "./trust-store"

/** tRPC `installs` (lib/trpc/installs.ts), the approval part. */
export type TrustApi = {
  trust: (input: { installId: string }) => Promise<{ installs: ServedInstall[]; approvals: ServedApproval[] }>
  requestApproval: (input: { installId: string }) => Promise<unknown>
  claimRoot: (input: { installId: string; signature: string }) => Promise<unknown>
  approve: (input: { installId: string; subjectInstallId: string; kind: `approve` | `reset` | `decline`; signature: string }) => Promise<unknown>
  revoke: (input: { installId: string }) => Promise<unknown>
}

/** What a target of a wrap is known as (`pending`'s pair, an install row). */
export type WrapTargetInput = { installId: string; x25519Public: string; endpointId?: string | null; userId?: string | null }

/** One recorded decision (the newest last, at most `DECISION_LOG` kept). */
export type TrustDecision = { rule: `R1` | `R3` | `R4`; subject: string; verdict: string }
const DECISION_LOG = 200

export type InstallTrustState = TrustStatus | `unknown` | `unavailable`

const errorCode = (error: unknown) => (error as { data?: { code?: unknown } } | null)?.data?.code

export class InstallTrust {
  store: TrustStore
  status: InstallTrustState = `unknown`
  rows: TrustRow[] = []
  decisions: TrustDecision[] = []
  readonly endpointId: string
  private served = new Map<string, ServedInstall>()
  private roots = new Set<string>()
  private listeners = new Set<() => void>()
  private asked = false
  private syncing: Promise<void> | undefined

  constructor(
    readonly userId: string,
    readonly installId: string,
    private readonly identity: Uint8Array,
    private readonly deps: { api: TrustApi; io: TrustStoreIO; mode?: TrustMode }
  ) {
    this.endpointId = endpointIdOf(identity)
    this.store = emptyTrustStore(this.endpointId, userId)
  }

  get mode(): TrustMode {
    return this.deps.mode ?? TRUST_MODE
  }

  /** The pin store this install kept (its own endpoint's; another is ignored). */
  async load(): Promise<void> {
    const kept = await this.deps.io.get(this.userId).catch(() => undefined)
    if (kept?.self === this.endpointId && kept.userId === this.userId) this.store = kept
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener)
    return () => this.listeners.delete(listener)
  }

  /** Follows `installs.trust` once (one at a time): claims the root when the
   *  account has no chain, asks once to be approved when it is not on it. */
  sync(): Promise<void> {
    this.syncing ??= this.syncOnce().finally(() => {
      this.syncing = undefined
    })
    return this.syncing
  }

  private async syncOnce(): Promise<void> {
    for (let round = 0; round < 2; round++) {
      let answer: Awaited<ReturnType<TrustApi[`trust`]>>
      try {
        answer = await this.deps.api.trust({ installId: this.installId })
      } catch (error) {
        if (errorCode(error) === `NOT_FOUND`) this.setStatus(`unavailable`)
        return
      }
      const view = reconcileTrust(this.store, answer)
      this.store = view.store
      this.rows = view.rows
      this.served = new Map(answer.installs.map((row) => [row.endpointId, row]))
      this.roots = new Set(answer.approvals.filter((a) => a.kind === `root` && a.approverEndpointId === a.subjectEndpointId).map((a) => a.subjectEndpointId))
      await this.persist()
      this.setStatus(view.status)
      if (view.status === `claimRoot`) {
        const me = this.served.get(this.endpointId)
        if (!me) return
        const signature = signApproval(this.identity, `root`, me)
        const claimed = await this.deps.api.claimRoot({ installId: this.installId, signature }).then(
          () => true,
          () => false
        )
        if (claimed) continue
        // CONFLICT: another install started the chain meantime: ask instead.
      }
      if (view.status !== `trusted` && !this.asked && !this.served.get(this.endpointId)?.approvalRequested) {
        this.asked = await this.deps.api.requestApproval({ installId: this.installId }).then(
          () => true,
          () => false
        )
      }
      return
    }
  }

  /** A person pressed Approve on a row. */
  async approve(row: TrustRow): Promise<void> {
    if (this.status !== `trusted`) throw new Error(`this install is not trusted`)
    const subject = this.served.get(row.endpointId)
    if (!subject) throw new Error(`install not found`)
    const kind: ApprovalKind = row.kind === `reset` ? `reset` : `approve`
    await this.deps.api.approve({ installId: this.installId, subjectInstallId: row.installId, kind, signature: signApproval(this.identity, kind, subject) })
    await this.sync()
  }

  /** A person pressed Remove: an own install is revoked (its wraps go, its
   *  trees rotate); a team-mate's reset is declined. */
  async remove(row: TrustRow): Promise<void> {
    if (row.kind === `newInstall`) {
      await this.deps.api.revoke({ installId: row.installId })
    } else {
      const subject = this.served.get(row.endpointId)
      if (!subject) throw new Error(`install not found`)
      await this.deps.api.approve({
        installId: this.installId,
        subjectInstallId: row.installId,
        kind: `decline`,
        signature: signApproval(this.identity, `decline`, subject),
      })
    }
    await this.sync()
  }

  /** R1: whether a served wrap is kept, recorded in the store. */
  keep(wrap: ServedWrap, installSecret: Uint8Array): KeepVerdict {
    const verdict = keepWrap(wrap, { identity: this.identity, installSecret }, this.store, this.mode)
    this.log(`R1`, `${wrap.rootId}#${wrap.epoch}`, verdict.keep ? verdict.record : `refused:${verdict.reason}`)
    if (verdict.keep) this.record(wrap.rootId, wrap.epoch, verdict.record)
    return verdict
  }

  /** What this install recorded about a key it holds (`legacy` = held before, decision c). */
  recordOf(rootId: string, epoch: number): KeyRecord {
    return this.store.roots[rootId]?.epochs[String(epoch)] ?? `legacy`
  }

  /** Records a key (its own new keys are `verified`); persisted. */
  record(rootId: string, epoch: number, record: KeyRecord) {
    if (this.store.roots[rootId]?.epochs[String(epoch)] === record) return
    this.store = recordKey(this.store, rootId, epoch, record)
    void this.persist()
  }

  /** R2 + R3: whether this install seals to (root, epoch, key). */
  seal(input: Parameters<typeof sealTo>[0]): SealVerdict {
    const verdict = sealTo(input, this.store, this.mode)
    this.log(`R3`, `${input.rootId}#${input.epoch}`, verdict.seal ? verdict.record : `refused:${verdict.reason}`)
    return verdict
  }

  /** R4: whether this install wraps a key for `target`. */
  target(target: WrapTargetInput): TargetVerdict {
    if (this.status === `unavailable`) return { wrap: true, record: `legacy` }
    const known = target.endpointId ? this.served.get(target.endpointId) : [...this.served.values()].find((row) => row.installId === target.installId)
    const served: ServedInstall = {
      installId: target.installId,
      userId: target.userId ?? known?.userId ?? ``,
      endpointId: target.endpointId ?? known?.endpointId ?? ``,
      x25519Public: target.x25519Public,
      registrationSignature: known?.registrationSignature ?? null,
      registrationSignedAt: known?.registrationSignedAt ?? null,
      approvalRequested: known?.approvalRequested ?? false,
    }
    // The registration binds the key the server lists in `installs.trust`;
    // a target served elsewhere with another key is checked against it.
    if (known && known.x25519Public !== target.x25519Public && !this.store.pins[served.endpointId]) {
      this.log(`R4`, target.installId, `refused:keyMismatch`)
      return { wrap: false, reason: `keyMismatch` }
    }
    const verdict = wrapTarget(served, this.store, this.mode, this.roots)
    this.log(`R4`, target.installId, verdict.wrap ? verdict.record : `refused:${verdict.reason}`)
    return verdict
  }

  private log(rule: TrustDecision[`rule`], subject: string, verdict: string) {
    this.decisions.push({ rule, subject, verdict })
    if (this.decisions.length > DECISION_LOG) this.decisions.splice(0, this.decisions.length - DECISION_LOG)
  }

  private async persist() {
    await this.deps.io.put(this.userId, this.store).catch(() => {})
  }

  private setStatus(status: InstallTrustState) {
    this.status = status
    for (const listener of this.listeners) listener()
  }
}
