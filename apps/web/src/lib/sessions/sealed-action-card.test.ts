// A press in a keyed tree (#EXP-2067): `sessions.send` takes the action card
// as the envelope every client builds (`sealCard` / its twins, replayed from
// sealed-content.json `card.splits`), clear fields included.
import { describe, expect, it } from "vitest"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { sealCard } from "@exp/domain-contract/sealed-content"
import { actionCardInputSchema } from "@exp/db-schema/domain"
import { validSealedCard } from "./sealed"

type Card = Record<string, unknown>
const splits = (fixture.card.splits as { card: Card; clear: Card; envelope: Card }[]).filter((split) => split.card.kind === `action`)

describe(`a sealed action card as sessions.send input`, () => {
  it(`the fixture holds a press on a synced row (a clear messageId)`, () => {
    expect(splits.some((split) => typeof split.clear.messageId === `string`)).toBe(true)
  })

  it.each(splits.map((split) => [JSON.stringify(split.clear), split] as const))(`takes the clients' envelope %s`, (_name, split) => {
    expect(actionCardInputSchema.safeParse(split.envelope).success).toBe(true)
    expect(validSealedCard(split.envelope)).toBe(true)
  })

  it(`takes what sealCard builds, with and without a messageId`, () => {
    const card = {
      kind: `action`,
      name: `reply`,
      context: null,
      surfaceId: `show:00000000-0000-4000-8000-000000000035`,
      block: null,
      messageId: `00000000-0000-4000-8000-000000000035`,
    }
    for (const press of [card, { ...card, surfaceId: `n:s1:m1#1`, block: `n:s1:m1#1`, messageId: null }]) {
      const envelope = sealCard(fixture.tree.publicKey, 1, press)
      expect(Object.keys(envelope).sort()).toEqual(press.messageId ? [`kind`, `messageId`, `sealed`] : [`kind`, `sealed`])
      expect(actionCardInputSchema.safeParse(envelope).success).toBe(true)
    }
  })

  it(`still refuses a card field in clear beside sealed`, () => {
    const envelope = sealCard(fixture.tree.publicKey, 1, { kind: `action`, name: `reply`, context: null, surfaceId: `x`, block: null, messageId: null })
    expect(actionCardInputSchema.safeParse({ ...envelope, name: `reply` }).success).toBe(false)
    expect(actionCardInputSchema.safeParse({ ...envelope, messageId: `not-a-uuid` }).success).toBe(false)
  })
})
