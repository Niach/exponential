// EXP-1459: the web's ONE opening layer for sealed session content
// (fixtures/sealed-content.json; the locked rules = its `placeholder`).
// Every reader of `messages.body` / `messages.card`, the digest and
// `sessions.sealed_title` goes through here before rendering: the adapters
// in lib/session-tree.ts and the digest in hooks/use-session-tree.ts.
//
// Opening costs an X25519 + ChaCha20 per envelope, and lists re-derive on
// every change, so results are memoized per TreeKeys map (KeyHolder swaps a
// root's map whenever a key arrives, which invalidates exactly that root)
// and per envelope string. A body that stays `noKey` asks the holder for its
// root's wrap (batched, rate-limited).

import {
  lockedReason,
  openBody,
  openCard,
  type LockedReason,
  type OpenedBody,
  type OpenStatus,
  type TreeKeys,
} from "@exp/domain-contract/sealed-content"
import { currentKeyHolder } from "./current-key-holder"
import type { KeyHolder } from "./key-holder"

type Card = Record<string, unknown>

/** Where the keys come from (the tab's KeyHolder; a fake in tests). */
export type KeySource = Pick<KeyHolder, `keysFor` | `request`>

const EMPTY: TreeKeys = new Map()
const MAX_PER_KEYS = 5000

const bodies = new WeakMap<TreeKeys, Map<string, OpenedBody>>()
const cards = new WeakMap<TreeKeys, Map<string, { status: OpenStatus; epoch: number | null; inner: Card | null }>>()

function memo<V>(store: WeakMap<TreeKeys, Map<string, V>>, keys: TreeKeys, key: string, make: () => V): V {
  let map = store.get(keys)
  if (!map) {
    map = new Map()
    store.set(keys, map)
  }
  let value = map.get(key)
  if (value === undefined) {
    if (map.size >= MAX_PER_KEYS) map.clear()
    value = make()
    map.set(key, value)
  }
  return value
}

/** `openBody`, memoized (plaintext is returned without touching a cache). */
export function openBodyCached(body: string, keys: TreeKeys): OpenedBody {
  if (!body.startsWith(`exp-sealed:`)) return { status: `plain`, epoch: null, text: body }
  return memo(bodies, keys, body, () => openBody(body, keys))
}

/** `openCard`, memoized on the sealed value; the clear fields are laid over
 *  the sealed card each time (the server may change them without re-sealing). */
export function openCardCached(card: Card, keys: TreeKeys): { status: OpenStatus; epoch: number | null; card: Card } {
  if (typeof card.sealed !== `string`) return openCard(card, keys)
  const { sealed, ...clear } = card
  const opened = memo(cards, keys, `${String(clear.kind)}\n${sealed}`, () => {
    const result = openCard({ kind: clear.kind, sealed }, keys)
    return { status: result.status, epoch: result.epoch, inner: result.status === `opened` ? result.card : null }
  })
  if (opened.status !== `opened`) return { status: opened.status, epoch: opened.epoch, card: clear }
  return { status: `opened`, epoch: opened.epoch, card: { ...opened.inner!, ...clear } }
}

/** A message opened with its root's keys (`openMessage`'s rules, the fixture's
 *  `placeholder.messageCases`); a `noKey` asks the holder for the wrap. */
export function openMessageWith<M extends { body: string; card: Card | null }>(
  message: M,
  rootId: string,
  source: KeySource | undefined
): M & { locked: LockedReason | null } {
  const sealedBody = message.body.startsWith(`exp-sealed:`)
  const sealedCard = !!message.card && `sealed` in message.card
  if (!sealedBody && !sealedCard) return { ...message, locked: null }
  const keys = source?.keysFor(rootId) ?? EMPTY
  const body = openBodyCached(message.body, keys)
  const card = message.card ? openCardCached(message.card, keys) : null
  const reasons = [lockedReason(body.status), card ? lockedReason(card.status) : null]
  const locked: LockedReason | null = reasons.includes(`noKey`)
    ? `noKey`
    : reasons.includes(`unreadable`)
      ? `unreadable`
      : null
  if (locked === `noKey`) {
    source?.request(rootId, body.status === `noKey` ? body.epoch : (card?.epoch ?? null))
  }
  return {
    ...message,
    body: locked ? `` : (body.text ?? ``),
    card: card ? card.card : null,
    locked,
  }
}

/** A root's sealed title opened, else null (keep the neutral `title`). */
export function openTitleWith(
  sealedTitle: string | null | undefined,
  rootId: string,
  source: KeySource | undefined
): string | null {
  if (!sealedTitle) return null
  const opened = openBodyCached(sealedTitle, source?.keysFor(rootId) ?? EMPTY)
  if (opened.status === `noKey`) source?.request(rootId, opened.epoch)
  return opened.status === `opened` || opened.status === `plain` ? opened.text : null
}

/** The digest's messages of `sessionId` opened with its ROOT's keys (a
 *  session row not synced yet = itself as the root). */
export function openDigestMessages<M extends { body: string; card: Card | null }>(
  sessionId: string,
  messages: M[],
  rootOf: (sessionId: string) => string,
  source: KeySource | undefined = currentKeyHolder()
): Array<M & { locked: LockedReason | null }> {
  if (messages.length === 0) return []
  const rootId = rootOf(sessionId)
  return messages.map((message) => openMessageWith(message, rootId, source))
}
