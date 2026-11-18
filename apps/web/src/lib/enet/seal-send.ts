// EXP-1459: a person's `sessions.send` / `sessions.answer` sealed before it
// leaves the browser (fixtures/sealed-content.json; wire names: #EXP-1458
// contract addenda 1+2):
//   - `text` = a body envelope to the target's ROOT key;
//   - a root WITHOUT a key: an existing one gets it first
//     (`sessionKeys.create`); a root the send creates (a new chat, the
//     first message to an agent or an app) carries it inline as
//     `seal: {installId, publicKey, epoch: 1, wraps}`, which the server
//     stores in the same transaction;
//   - `name` → `sealedName` (writes `sessions.sealed_title`);
//   - `@email` mentions → clear `mentionUserIds` (the server can no longer
//     read the body); attachments stay the clear `attachmentIds`, their
//     lines go INSIDE the text (#EXP-1683, `withAttachmentEmbeds`);
//   - an answer to a SEALED card = `sealedCard` (the full card with the
//     answer in it, re-sealed) beside the clear `decision` / `fresh`.
// #EDE-97 (sealed-content.json `clearTree`): NO silent plaintext. Only a
// root an outside MCP client started (`clear_tree`) takes plaintext; any
// other text waits for this tab's key holder (`holderWaitMs`) and, for a
// keyless tree this install may not key, for its device's key
// (`keyWaitMs`); a text that still cannot be sealed FAILS with
// `NO_KEY_REFUSAL` (the failed-send toast), nothing leaves in clear.
// #EXP-1960 (`clearTree.keylessCases`): a failed `sessionKeys.create` is
// read (`createFailure` -> `keylessStep`): a blip is retried, a wrap the
// server refuses is retried with this account's installs only, and a tree
// this install may not key asks its DEVICES to key it
// (`sessionKeys.requestKey`) before its key is waited for; the bare wait
// is left only for a real NOT_FOUND (or an older server).

import type { SessionsSendInput } from "@exp/db-schema/domain"
import {
  createFailure,
  isStaleTreeRefusal,
  keylessStep,
  type CreateFailure,
  type RequestKeyAnswer,
  NO_KEY_REFUSAL,
  SEAL_CREATE_ATTEMPTS,
  SEAL_CREATE_RETRY_MS,
  SEAL_HOLDER_WAIT_MS,
  SEAL_KEY_WAIT_MS,
  sealBody,
  sealCard,
  withAttachmentEmbeds,
  type AttachmentUpload,
} from "@exp/domain-contract/sealed-content"
import { extractIssueRefs } from "@/lib/issue-refs"
import { extractMentionEmails } from "@/lib/mention-refs"
import { answeredCard, type CardAnswerInput } from "@/lib/sessions/card-answer"
import type { KeyHolder, ReaderInstall } from "./key-holder"
import { openCardCached } from "./sealed-open"

type Card = Record<string, unknown>

/** The synced `sessions` columns sealing reads. */
export type SealSessionRow = {
  id: string
  rootId: string | null
  parentId: string | null
  teamId: string
  userId: string
  deviceId: string | null
  agentId?: string | null
  appId?: string | null
  shared: boolean
  sealPublicKey?: string | null
  sealEpoch?: number | null
  /** #EDE-97: an outside MCP client's root: plaintext for life. */
  clearTree?: boolean | null
}

/** An install a tree key may be wrapped for (`installs.mine` / `installs.list`). */
export type InstallRow = ReaderInstall & { userId?: string; deviceId?: string | null; revokedAt?: unknown }

export type SealDeps = {
  holder: Pick<KeyHolder, `userId` | `installId` | `newTreeKey` | `keep` | `createTree` | `current` | `keysFor`> | undefined
  me: string
  session: (id: string) => SealSessionRow | undefined
  sessions: () => readonly SealSessionRow[]
  /** The devices rows (uuid) of a machine id (`sessions.device_id`). */
  deviceRowIds: (machineId: string) => string[]
  myInstalls: () => Promise<InstallRow[]>
  teamInstalls: (teamId: string) => Promise<InstallRow[]>
  userIdsByEmail: (emails: string[]) => string[]
  /** Issue ids of `#IDENT` refs in the team (unknown ones dropped). */
  issueIdsByIdentifier: (teamId: string | undefined, identifiers: string[]) => string[]
  /** #FEED-248: the server's word on the caller's conversation with an
   *  agent / app (`sessions.sendTarget`): the synced store is a cache. */
  sendTarget: (input: { agentId?: string; appId?: string }) => Promise<ServerTarget>
  /** The server's current key of a root (`sessionKeys.publicKeys`), if keyed. */
  publicKey: (rootId: string) => Promise<TreePublicKey | undefined>
  /** #EXP-1960: `sessionKeys.requestKey`: the tree's devices key it. */
  requestKey?: (rootId: string) => Promise<RequestKeyResult>
  /** #EDE-97: this tab's key holder once it runs (registering its install
   *  when it has none), within `ms`; undefined = none came. Absent = no wait. */
  awaitHolder?: (ms: number) => Promise<SealDeps[`holder`]>
  /** Test seam: waits between key polls (default: a timer). */
  sleep?: (ms: number) => Promise<void>
  /** Test seam: the key waits (default: the fixture's). */
  waits?: { holderMs: number; keyMs: number; pollMs: number; createRetryMs?: number }
}

export type TreePublicKey = { epoch: number; publicKey: string }
export type RequestKeyResult =
  | { state: `keyed`; epoch: number; publicKey: string }
  | { state: `clear` }
  | { state: `requested` }
  | { state: `noHost` }
export type ServerTarget = {
  rootId: string | null
  epoch: number | null
  publicKey: string | null
  /** #EDE-97: an outside MCP client's conversation (absent on an older server = not). */
  clearTree?: boolean
}

export type SealedSendInput = SessionsSendInput & {
  seal?: { installId: string; publicKey: string; epoch: 1; wraps: Array<{ installId: string; wrapped: string }> }
  sealedName?: string
  mentionUserIds?: string[]
  refIssueIds?: string[]
}

export type SealedSend = {
  input: SealedSendInput
  /** Call with the send's result: keeps a tree key the send created. */
  after: (result: { sessionId: string }) => Promise<void>
}

const uploads = new Map<string, AttachmentUpload>()

/** #EXP-1683: a session upload this tab made, for its sealed attachment line. */
export function rememberSessionUpload(id: string, upload: AttachmentUpload): void {
  uploads.set(id, upload)
}

const noop = async (_result: { sessionId: string }) => {}

/** #EDE-97: a text that cannot be sealed: the send fails, nothing leaves. */
export class NoKeyError extends Error {
  constructor() {
    super(NO_KEY_REFUSAL)
    this.name = `NoKeyError`
  }
}

const CLEAR = Symbol(`clear`)

/** A tRPC error's code (`TRPCClientError.data.code`); none = no answer. */
function errorCode(error: unknown): string | undefined {
  const data = (error as { data?: { code?: unknown } } | null)?.data
  return typeof data?.code === `string` ? data.code : undefined
}

/** The root a send lands in, when it exists already. */
export function targetRoot(input: SessionsSendInput, deps: Pick<SealDeps, `session` | `sessions` | `me`>): SealSessionRow | undefined {
  const rootOf = (id: string | undefined) => {
    const row = id ? deps.session(id) : undefined
    return row ? (deps.session(row.rootId ?? row.id) ?? (row.parentId === null ? row : undefined)) : undefined
  }
  if (input.sessionId) return rootOf(input.sessionId)
  if (input.parentId) return rootOf(input.parentId)
  if (input.agentId || input.appId) {
    // The caller's own conversation with the agent / editor of the app.
    return deps
      .sessions()
      .find(
        (row) =>
          row.parentId === null &&
          row.userId === deps.me &&
          (input.agentId ? row.agentId === input.agentId : row.appId === input.appId)
      )
  }
  return undefined
}

function live(rows: InstallRow[]): InstallRow[] {
  return rows.filter((row) => !row.revokedAt)
}

/** Who a writer wraps a tree key for: the installs it can name as readers
 *  (the server refuses anything else). Holders fill the rest via `pending`. */
async function readersFor(
  root: { teamId: string; userId: string; shared: boolean; deviceId: string | null },
  deps: SealDeps
): Promise<ReaderInstall[]> {
  const out = new Map<string, ReaderInstall>()
  const add = (row: InstallRow) => out.set(row.installId, { installId: row.installId, x25519Public: row.x25519Public })
  if (root.userId === deps.me || root.shared) for (const row of live(await deps.myInstalls())) add(row)
  // `installs.list` names an install's device by its MACHINE id (the
  // `sessions.device_id`); a devices row uuid is accepted too (#EXP-1960:
  // matching only row uuids never wrapped for a teammate's shared host).
  const deviceIds = root.deviceId ? new Set([root.deviceId, ...deps.deviceRowIds(root.deviceId)]) : new Set<string>()
  if (root.shared || root.deviceId) {
    const team = live(await deps.teamInstalls(root.teamId).catch(() => []))
    for (const row of team) {
      if (root.shared || (row.deviceId && deviceIds.has(row.deviceId))) add(row)
    }
  }
  return [...out.values()]
}

/** `mentionUserIds` for the text's `@email` tokens (none = omitted). */
export function mentionUserIds(text: string | undefined, deps: Pick<SealDeps, `userIdsByEmail`>): string[] | undefined {
  const emails = text ? extractMentionEmails(text) : []
  if (emails.length === 0) return undefined
  const ids = [...new Set(deps.userIdsByEmail(emails))]
  return ids.length > 0 ? ids : undefined
}

/** `refIssueIds` for the text's `#IDENT` refs (none = omitted). */
export function refIssueIds(
  text: string | undefined,
  teamId: string | undefined,
  deps: Pick<SealDeps, `issueIdsByIdentifier`>
): string[] | undefined {
  const identifiers = text ? extractIssueRefs(text) : []
  if (identifiers.length === 0) return undefined
  const ids = [...new Set(deps.issueIdsByIdentifier(teamId, identifiers))]
  return ids.length > 0 ? ids : undefined
}

/** #FEED-248 (sealed-content.json `staleTree`): `fresh` = the store was
 *  wrong once (the server refused the key write): resolve the tree from
 *  the server alone. */
export async function sealSendInput(
  input: SessionsSendInput,
  deps: SealDeps,
  options: { fresh?: boolean } = {}
): Promise<SealedSend> {
  const hasText = typeof input.text === `string` && input.text.length > 0
  if (!hasText && !input.name) return { input, after: noop }
  const mentions = mentionUserIds(input.text, deps)
  const root = targetRoot(input, deps)
  const refs = refIssueIds(input.text, root?.teamId ?? input.teamId, deps)
  const clearMeta = { ...(mentions ? { mentionUserIds: mentions } : {}), ...(refs ? { refIssueIds: refs } : {}) }

  const plain = { input: { ...input, ...clearMeta }, after: noop }
  // #EDE-97: an outside MCP client's tree is the one place for plaintext.
  if (root?.clearTree) return plain
  const waits = deps.waits ?? { holderMs: SEAL_HOLDER_WAIT_MS, keyMs: SEAL_KEY_WAIT_MS, pollMs: 1000 }
  const sleep = deps.sleep ?? ((ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms)))
  const holder = deps.holder ?? (await deps.awaitHolder?.(waits.holderMs).catch(() => undefined))
  if (!holder) throw new NoKeyError()
  /** A keyless tree this install may not key: its device's claim keys it. */
  const awaitKey = async (rootId: string) => {
    const deadline = Date.now() + waits.keyMs
    for (;;) {
      const key = await deps.publicKey(rootId).catch(() => undefined)
      if (key) return key
      const left = deadline - Date.now()
      if (left <= 0) throw new NoKeyError()
      await sleep(Math.min(left, waits.pollMs))
    }
  }
  const fresh = options.fresh === true
  // #FEED-248: the conversation with an agent / app the store does not
  // have (a missed sync, another device started it) may well exist: ask.
  const conversation = !input.sessionId && !input.parentId && !!(input.agentId || input.appId)
  const target =
    conversation && (!root || fresh)
      ? await deps.sendTarget({ agentId: input.agentId, appId: input.appId }).catch(() => undefined)
      : undefined
  // #EDE-97: a clear conversation the store does not know as one yet.
  if (target?.clearTree) return plain
  /** #EXP-1960: a tree this install may not key: its devices are asked
   *  to; their key is waited for, a tree no device hosts fails at once. */
  const askDevice = async (rootId: string) => {
    let answer: RequestKeyAnswer = `error`
    let result: RequestKeyResult | undefined
    try {
      result = deps.requestKey ? await deps.requestKey(rootId) : undefined
      answer = result?.state ?? `error`
    } catch (error) {
      answer = errorCode(error) === `NOT_FOUND` ? `notFound` : `error`
    }
    const step = keylessStep({ answer })
    if (step === `plain`) return CLEAR
    if (step === `failed`) throw new NoKeyError()
    if (step === `sealed` && result?.state === `keyed`) return { epoch: result.epoch, publicKey: result.publicKey }
    return awaitKey(rootId)
  }
  /** A keyless root this install keys first (`sessionKeys.create`); a
   *  failure takes the fixture's `keylessStep`, never a silent wait. */
  const keyFirst = async (rootId: string, readers: Parameters<typeof readersFor>[0]) => {
    let ownOnly = false
    for (let attempt = 1; ; attempt++) {
      let failure: CreateFailure
      try {
        const wraps = ownOnly ? live(await deps.myInstalls()) : await readersFor(readers, deps)
        // Not among the readers it can name: this install may not key it.
        if (!wraps.some((row) => row.installId === holder.installId)) failure = `notReader`
        else return await holder.createTree(rootId, wraps)
      } catch (error) {
        failure = createFailure({ code: errorCode(error), message: error instanceof Error ? error.message : undefined })
      }
      switch (keylessStep({ failure, attempt, ownOnly })) {
        case `plain`:
          return CLEAR
        case `serverKey`:
          return (await deps.publicKey(rootId).catch(() => undefined)) ?? askDevice(rootId)
        case `retryOwn`:
          ownOnly = true
          break
        case `retry`:
          await sleep((waits.createRetryMs ?? SEAL_CREATE_RETRY_MS) * attempt)
          break
        case `requestKey`:
          return askDevice(rootId)
        default:
          throw new NoKeyError()
      }
      if (attempt >= SEAL_CREATE_ATTEMPTS + 1) throw new NoKeyError()
    }
  }

  let key: { epoch: number; publicKey: string }
  let after = noop
  let seal: SealedSendInput[`seal`]
  if (target?.rootId) {
    const known = target.epoch && target.publicKey ? { epoch: target.epoch, publicKey: target.publicKey } : undefined
    const held =
      known ??
      (await keyFirst(target.rootId, {
        teamId: root?.teamId ?? input.teamId ?? ``,
        userId: deps.me,
        shared: false,
        deviceId: root?.deviceId ?? null,
      }))
    if (held === CLEAR) return plain
    key = held
  } else if (root && !(target && fresh)) {
    const synced = root.sealPublicKey && root.sealEpoch ? { epoch: root.sealEpoch, publicKey: root.sealPublicKey } : undefined
    const known = fresh ? ((await deps.publicKey(root.id).catch(() => undefined)) ?? synced) : synced
    const held = known ?? holder.current(root.id) ?? (await keyFirst(root.id, root))
    if (held === CLEAR) return plain
    key = held
  } else if (fresh && (input.sessionId || input.parentId) && !conversation) {
    // A session the store does not have: its key, when it is a keyed root.
    key = await awaitKey((input.sessionId ?? input.parentId)!)
  } else {
    // A root this send creates: its first key rides inline.
    const created = holder.newTreeKey(
      await readersFor({ teamId: input.teamId ?? ``, userId: deps.me, shared: false, deviceId: input.deviceId ?? null }, deps)
    )
    key = { epoch: created.epoch, publicKey: created.publicKey }
    seal = { installId: holder.installId, publicKey: created.publicKey, epoch: 1, wraps: created.wraps }
    after = (result: { sessionId: string }) => holder.keep(result.sessionId, created.epoch, created.secret)
  }

  const { name, ...rest } = input
  const sealed: SealedSendInput = {
    ...rest,
    ...(hasText
      ? { text: sealBody(key.publicKey, key.epoch, withAttachmentEmbeds(input.text!, input.attachmentIds ?? [], (id) => uploads.get(id))) }
      : {}),
    ...(name ? { sealedName: sealBody(key.publicKey, key.epoch, name) } : {}),
    ...clearMeta,
    ...(seal ? { seal } : {}),
  }
  return { input: sealed, after }
}

/** #FEED-248: `sessions.send` through `sealSendInput`; a refused key write
 *  (the tree moved on behind this writer's store) re-resolves the tree from
 *  the server, re-seals and re-sends ONCE by itself. */
export async function sendSealed<R extends { sessionId: string }>(
  input: SessionsSendInput,
  deps: SealDeps,
  send: (input: SealedSendInput) => Promise<R>
): Promise<R> {
  const attempt = async (fresh: boolean) => {
    const sealed = await sealSendInput(input, deps, { fresh })
    const result = await send(sealed.input)
    await sealed.after(result).catch(() => {})
    return result
  }
  try {
    return await attempt(false)
  } catch (error) {
    if (!isStaleTreeRefusal(error instanceof Error ? error.message : undefined)) throw error
    return attempt(true)
  }
}

/** `sessions.answer`'s input: a SEALED card's answer re-sealed inside the
 *  full card (`sealedCard`), else the plaintext answer as before.
 *  `standIn` = an escalated origin this install cannot open: its mirror's
 *  opened copy, re-sealed to the ORIGIN tree's current key. */
export function sealAnswerInput(
  message: { card: Card | null; rootId: string },
  root: SealSessionRow | undefined,
  input: CardAnswerInput,
  holder: Pick<KeyHolder, `keysFor` | `current`> | undefined,
  standIn?: { card: Card; key: { epoch: number; publicKey: string } }
): CardAnswerInput | { decision?: `approved` | `rejected`; fresh?: boolean; sealedCard: Card } {
  if (!message.card || typeof message.card.sealed !== `string`) return input
  const locked = new Error(`This card is end-to-end encrypted and this browser holds no key for it yet`)
  const opened = holder ? openCardCached(message.card, holder.keysFor(message.rootId)) : null
  const full = opened?.status === `opened` ? opened.card : standIn?.card
  if (!full) throw locked
  const answered = answeredCard(full as never, input)
  if (!answered.ok) throw new Error(answered.message)
  const key =
    root?.sealPublicKey && root.sealEpoch
      ? { epoch: root.sealEpoch, publicKey: root.sealPublicKey }
      : (holder?.current(message.rootId) ?? standIn?.key)
  if (!key) throw locked
  const sealedCard = sealCard(key.publicKey, key.epoch, answered.card as Card)
  return {
    ...(input.decision ? { decision: input.decision } : {}),
    ...(input.fresh !== undefined ? { fresh: input.fresh } : {}),
    sealedCard,
  }
}
