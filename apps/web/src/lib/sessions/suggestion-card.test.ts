import { describe, expect, it } from "vitest"
import type { MessageCard } from "@exp/db-schema/domain"
import { patchReplies, patchSuggestion, suggestionRequest, suggestionState } from "./suggestion-card"

// EXP-1328: an item is open until ONE action stamps it.
const card: MessageCard = {
  kind: `suggestions`,
  items: [
    { id: `a`, title: `Thumbnails`, text: `Add lazy thumbnails` },
    { id: `b`, title: `Docs`, text: `Document it`, sessionId: `s-1` },
  ],
}

describe(`suggestion card`, () => {
  it(`derives each item's state`, () => {
    expect(suggestionState({ id: `x`, title: `t`, text: `x` })).toBe(`open`)
    expect(suggestionState({ id: `x`, title: `t`, text: `x`, sessionId: `s` })).toBe(`started`)
    expect(suggestionState({ id: `x`, title: `t`, text: `x`, here: true })).toBe(`here`)
    expect(suggestionState({ id: `x`, title: `t`, text: `x`, issueId: `i` })).toBe(`filed`)
    expect(suggestionState({ id: `x`, title: `t`, text: `x`, dismissed: true })).toBe(`dismissed`)
  })

  it(`requests the title then the text`, () => {
    expect(suggestionRequest({ title: `Docs`, text: `Document it` })).toBe(`Docs\n\nDocument it`)
  })

  it(`stamps an open item and keeps the others`, () => {
    const patched = patchSuggestion(card, `a`, { sessionId: `s-2` })
    expect(patched.ok).toBe(true)
    if (!patched.ok || patched.card.kind !== `suggestions`) return
    expect(patched.item.sessionId).toBe(`s-2`)
    expect(patched.card.items.map((item) => item.sessionId ?? null)).toEqual([`s-2`, `s-1`])
  })

  it(`refuses a handled item, an unknown item and a non-suggestions card`, () => {
    expect(patchSuggestion(card, `b`, { dismissed: true })).toMatchObject({ ok: false, code: `CONFLICT` })
    expect(patchSuggestion(card, `zz`, { dismissed: true })).toMatchObject({ ok: false, code: `NOT_FOUND` })
    expect(
      patchSuggestion({ kind: `plan`, plan: `p` } as MessageCard, `a`, { dismissed: true })
    ).toMatchObject({ ok: false, code: `BAD_REQUEST` })
    expect(patchSuggestion(null, `a`, { dismissed: true })).toMatchObject({ ok: false })
  })
})

// EXP-1211: a quick reply is open until a tap or the dismiss stamps it sent.
describe(`replies card`, () => {
  const replies: MessageCard = {
    kind: `replies`,
    items: [
      { id: `a`, text: `Raise the version numbers` },
      { id: `b`, text: `Open the PR`, sent: true },
    ],
  }

  it(`stamps one reply sent`, () => {
    const patched = patchReplies(replies, `a`)
    expect(patched.ok && patched.item).toEqual({ id: `a`, text: `Raise the version numbers`, sent: true })
  })

  it(`refuses a sent, unknown or wrong-kind reply`, () => {
    expect(patchReplies(replies, `b`)).toMatchObject({ ok: false, code: `CONFLICT` })
    expect(patchReplies(replies, `z`)).toMatchObject({ ok: false, code: `NOT_FOUND` })
    expect(patchReplies(card, `a`)).toMatchObject({ ok: false, code: `BAD_REQUEST` })
  })

  it(`dismiss stamps every reply`, () => {
    const patched = patchReplies(replies, null)
    expect(patched.ok && patched.card).toEqual({
      kind: `replies`,
      items: [
        { id: `a`, text: `Raise the version numbers`, sent: true },
        { id: `b`, text: `Open the PR`, sent: true },
      ],
    })
  })
})
