import type { MessageCard, ReplyItem, SuggestionItem } from "@exp/db-schema/domain"

// EXP-1328: the `suggestions` card's pure rules (the writers live in
// `suggestions.ts`). An item is open until ONE of start / do it here / file /
// dismiss stamps it; a second action on it is a CONFLICT.

export type SuggestionState = `open` | `started` | `here` | `filed` | `dismissed`

/** Pure: an item's state (contract `SuggestionRow.state` + dismissed). */
export function suggestionState(item: SuggestionItem): SuggestionState {
  if (item.sessionId) return `started`
  if (item.here) return `here`
  if (item.issueId) return `filed`
  if (item.dismissed) return `dismissed`
  return `open`
}

/** The request a started item carries: its title, then its text. */
export function suggestionRequest(item: Pick<SuggestionItem, `title` | `text`>): string {
  return `${item.title}\n\n${item.text}`
}

/** Pure: the card with `itemId` patched, or the refusal. */
export function patchSuggestion(
  card: MessageCard | null,
  itemId: string,
  patch: Partial<SuggestionItem>
): { ok: true; card: MessageCard; item: SuggestionItem } | { ok: false; code: `BAD_REQUEST` | `NOT_FOUND` | `CONFLICT`; message: string } {
  if (!card || card.kind !== `suggestions`) {
    return { ok: false, code: `BAD_REQUEST`, message: `This message suggests nothing` }
  }
  const item = card.items.find((entry) => entry.id === itemId)
  if (!item) return { ok: false, code: `NOT_FOUND`, message: `Suggestion not found` }
  if (suggestionState(item) !== `open`) {
    return { ok: false, code: `CONFLICT`, message: `This suggestion was already handled` }
  }
  const next = { ...item, ...patch }
  return {
    ok: true,
    card: { ...card, items: card.items.map((entry) => (entry.id === itemId ? next : entry)) },
    item: next,
  }
}

// EXP-1211: the `replies` card (quick replies above the composer). A reply
// is open until a tap sends it or the row is dismissed (both stamp `sent`).

/** Pure: the replies card with `itemId` stamped sent (null = every open
 *  item, the dismiss), or the refusal. */
export function patchReplies(
  card: MessageCard | null,
  itemId: string | null
): { ok: true; card: MessageCard; item: ReplyItem | null } | { ok: false; code: `BAD_REQUEST` | `NOT_FOUND` | `CONFLICT`; message: string } {
  if (!card || card.kind !== `replies`) {
    return { ok: false, code: `BAD_REQUEST`, message: `This message offers no replies` }
  }
  if (itemId === null) {
    return {
      ok: true,
      card: { ...card, items: card.items.map((entry) => ({ ...entry, sent: true })) },
      item: null,
    }
  }
  const item = card.items.find((entry) => entry.id === itemId)
  if (!item) return { ok: false, code: `NOT_FOUND`, message: `Reply not found` }
  if (item.sent) return { ok: false, code: `CONFLICT`, message: `This reply was already sent` }
  const next = { ...item, sent: true }
  return {
    ok: true,
    card: { ...card, items: card.items.map((entry) => (entry.id === itemId ? next : entry)) },
    item: next,
  }
}
