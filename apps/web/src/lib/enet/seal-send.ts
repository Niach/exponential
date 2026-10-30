// EXP-1459: a person's `sessions.send` / `sessions.answer` sealed before it
// leaves the browser (fixtures/sealed-content.json; wire names: #EXP-1458
// contract addenda 1+2):
//   - `text` = a body envelope to the target's ROOT key;
//   - a root WITHOUT a key: an existing one gets it first
//     (`sessionKeys.create`); a root the send creates (a new thread, the
//     first message to an agent or an app) carries it inline as
//     `seal: {installId, publicKey, epoch: 1, wraps}`, which the server
//     stores in the same transaction;
//   - `name` → `sealedName` (writes `sessions.sealed_title`);
//   - `@email` mentions → clear `mentionUserIds` (the server can no longer
//     read the body); attachments stay the clear `attachmentIds`;
//   - an answer to a SEALED card = `sealedCard` (the full card with the
//     answer in it, re-sealed) beside the clear `decision` / `fresh`.
// Without a key holder (this browser has no eNet install yet) nothing is
// sealed: plaintext stays legal.

import type { SessionsSendInput } from "@exp/db-schema/domain"
import { sealBody, sealCard } from "@exp/domain-contract/sealed-content"
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
}

export type SealedSendInput = SessionsSendInput & {
  seal?: { installId: string; publicKey: string; epoch: 1; wraps: Array<{ installId: string; wrapped: string }> }
  sealedName?: string
  mentionUserIds?: string[]
}

export type SealedSend = {
  input: SealedSendInput
  /** Call with the send's result: keeps a tree key the send created. */
  after: (result: { sessionId: string }) => Promise<void>
}

const noop = async (_result: { sessionId: string }) => {}

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
  const deviceRows = root.deviceId ? new Set(deps.deviceRowIds(root.deviceId)) : new Set<string>()
  if (root.shared || deviceRows.size > 0) {
    const team = live(await deps.teamInstalls(root.teamId).catch(() => []))
    for (const row of team) {
      if (root.shared || (row.deviceId && deviceRows.has(row.deviceId))) add(row)
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

export async function sealSendInput(input: SessionsSendInput, deps: SealDeps): Promise<SealedSend> {
  const holder = deps.holder
  const hasText = typeof input.text === `string` && input.text.length > 0
  if (!holder || (!hasText && !input.name)) return { input, after: noop }
  const mentions = mentionUserIds(input.text, deps)
  const root = targetRoot(input, deps)

  let key: { epoch: number; publicKey: string }
  let after = noop
  let seal: SealedSendInput[`seal`]
  if (root) {
    const known = root.sealPublicKey && root.sealEpoch ? { epoch: root.sealEpoch, publicKey: root.sealPublicKey } : undefined
    const held = known ?? holder.current(root.id)
    if (held) key = held
    else {
      try {
        key = await holder.createTree(root.id, await readersFor(root, deps))
      } catch {
        // An older keyless root this install may not key (not a reader):
        // plaintext stays legal there.
        return { input: mentions ? { ...input, mentionUserIds: mentions } : input, after: noop }
      }
    }
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
    ...(hasText ? { text: sealBody(key.publicKey, key.epoch, input.text!) } : {}),
    ...(name ? { sealedName: sealBody(key.publicKey, key.epoch, name) } : {}),
    ...(mentions ? { mentionUserIds: mentions } : {}),
    ...(seal ? { seal } : {}),
  }
  return { input: sealed, after }
}

/** `sessions.answer`'s input: a SEALED card's answer re-sealed inside the
 *  full card (`sealedCard`), else the plaintext answer as before. */
export function sealAnswerInput(
  message: { card: Card | null; rootId: string },
  root: SealSessionRow | undefined,
  input: CardAnswerInput,
  holder: Pick<KeyHolder, `keysFor` | `current`> | undefined
): CardAnswerInput | { decision?: `approved` | `rejected`; fresh?: boolean; sealedCard: Card } {
  if (!message.card || typeof message.card.sealed !== `string`) return input
  if (!holder) throw new Error(`This card is sealed and this browser holds no key for it yet`)
  const opened = openCardCached(message.card, holder.keysFor(message.rootId))
  if (opened.status !== `opened`) throw new Error(`This card is sealed and this browser holds no key for it yet`)
  const answered = answeredCard(opened.card as never, input)
  if (!answered.ok) throw new Error(answered.message)
  const key =
    root?.sealPublicKey && root.sealEpoch
      ? { epoch: root.sealEpoch, publicKey: root.sealPublicKey }
      : holder.current(message.rootId)
  if (!key) throw new Error(`This card is sealed and this browser holds no key for it yet`)
  const sealedCard = sealCard(key.publicKey, key.epoch, answered.card as Card)
  return {
    ...(input.decision ? { decision: input.decision } : {}),
    ...(input.fresh !== undefined ? { fresh: input.fresh } : {}),
    sealedCard,
  }
}
