import {
  CARD_CLEAR_FIELDS,
  SEALED_BODY_PREFIX,
  parseBodyEnvelope,
  parseSealedValue,
} from "@exp/domain-contract/sealed-content"

// EXP-1458 (fixtures/sealed-content.json): the server never opens sealed
// content. A row whose body starts with `exp-sealed:` or whose card carries
// `sealed` is opaque: the server copies it whole (within its tree), reads
// only the card's CLEAR fields and the message's clear metadata
// (`attachment_ids`, `mention_user_ids`, `ref_issue_ids`), and never derives
// text from it (titles, briefs, notifications, digests, bus lines).

export { SEALED_BODY_PREFIX }

/** A body envelope (any version, well-formed or not: never shown as text). */
export function isSealedBody(body: string | null | undefined): boolean {
  return typeof body === `string` && body.startsWith(SEALED_BODY_PREFIX)
}

/** A card envelope: `{...clear, sealed}` (a non-string `sealed` counts too:
 *  it is never plaintext). */
export function isSealedCard(card: unknown): boolean {
  return !!card && typeof card === `object` && !Array.isArray(card) && `sealed` in (card as object)
}

/** THE check: a message body or card that is sealed. */
export function isSealed(value: string | Record<string, unknown> | null | undefined): boolean {
  return typeof value === `string` ? isSealedBody(value) : isSealedCard(value)
}

/** A message row is sealed when its body or its card is. */
export function isSealedMessage(row: { body?: string | null; card?: unknown }): boolean {
  return isSealedBody(row.body) || isSealedCard(row.card)
}

/** A WRITER's body is acceptable: plaintext, or a well-formed v1 envelope
 *  (a newer version passes too: readers show it as unsupported). Malformed
 *  `exp-sealed:` text is refused so it never lands as garbage. */
export function validSealedBody(body: string): boolean {
  if (!isSealedBody(body)) return true
  return parseBodyEnvelope(body).kind !== `malformed`
}

/** A writer's sealed card: a string `sealed` that parses, and nothing but
 *  the contract's clear fields beside it (the rest lives inside). */
export function validSealedCard(card: Record<string, unknown>): boolean {
  if (typeof card.sealed !== `string` || typeof card.kind !== `string`) return false
  if (parseSealedValue(card.sealed).kind === `malformed`) return false
  const clear = new Set<string>([...CARD_CLEAR_FIELDS, `sealed`])
  if (!Object.keys(card).every((key) => clear.has(key))) return false
  // `story` in clear (addendum 5) is the flag only.
  if (card.story !== undefined && typeof card.story !== `boolean`) return false
  // `pictures` in clear is the projection only: `{label?, attachmentId}`,
  // or `{label}` = a picture still to upload (addendum 4).
  if (card.pictures !== undefined) {
    if (!Array.isArray(card.pictures) || card.pictures.length === 0) return false
    return card.pictures.every((picture) => {
      if (!picture || typeof picture !== `object`) return false
      const entry = picture as Record<string, unknown>
      if (typeof entry.attachmentId !== `string` && typeof entry.label !== `string`) return false
      return Object.entries(entry).every(([key, value]) => (key === `attachmentId` || key === `label`) && typeof value === `string`)
    })
  }
  return true
}

/** The attachment ids a sealed card's clear `pictures` names. */
export function sealedCardAttachmentIds(card: Record<string, unknown>): string[] {
  return Array.isArray(card.pictures)
    ? card.pictures.flatMap((picture) => {
        const id = (picture as { attachmentId?: unknown } | null)?.attachmentId
        return typeof id === `string` ? [id] : []
      })
    : []
}

/** The clear part of a sealed card (everything but `sealed`). */
export function clearCard<T extends Record<string, unknown>>(card: T): Omit<T, `sealed`> {
  const { sealed: _sealed, ...clear } = card
  return clear
}

/** SQL fragment helpers' literal: `body LIKE 'exp-sealed:%'`. */
export const SEALED_BODY_LIKE = `${SEALED_BODY_PREFIX}%`
