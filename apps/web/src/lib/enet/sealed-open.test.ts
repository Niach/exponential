// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import type { TreeKeys } from "@exp/domain-contract/sealed-content"
import { openCardCached, openDigestMessages, openMessageWith, openTitleWith, resolveSealedMirrors, type KeySource } from "./sealed-open"

// EXP-1459: the web's opening layer replays fixtures/sealed-content.json
// (`placeholder.messageCases`, the body vector, every card split).

const hex = (text: string) => Uint8Array.from(text.match(/../g)!.map((b) => parseInt(b, 16)))
const keysOf = (keys: Record<string, string>): TreeKeys =>
  new Map(Object.entries(keys).map(([epoch, secret]) => [Number(epoch), hex(secret)]))
const source = (keys: TreeKeys) => {
  const request = vi.fn<(rootId: string, epoch: number | null) => void>()
  return { keysFor: () => keys, request } satisfies KeySource
}
const ROOT = `r1`

describe(`openMessageWith`, () => {
  for (const c of fixture.placeholder.messageCases) {
    it(`placeholder: ${c.name}`, () => {
      const keys = source(keysOf(c.keys as Record<string, string>))
      const opened = openMessageWith(c.message as { body: string; card: Record<string, unknown> | null }, ROOT, keys)
      expect({ locked: opened.locked, body: opened.body, card: opened.card }).toEqual(c.expected)
      if (c.expected.locked === `noKey`) expect(keys.request).toHaveBeenCalledWith(ROOT, 1)
      else expect(keys.request).not.toHaveBeenCalled()
    })
  }

  it(`a tab without a key holder locks every envelope and leaves plaintext alone`, () => {
    expect(openMessageWith({ body: fixture.body.envelope, card: null }, ROOT, undefined).locked).toBe(`noKey`)
    expect(openMessageWith({ body: `hi`, card: { kind: `picture` } }, ROOT, undefined)).toEqual({
      body: `hi`,
      card: { kind: `picture` },
      locked: null,
    })
  })

  it(`opens every card split and keeps the clear fields on top`, () => {
    const keys = keysOf({ 1: fixture.tree.secret })
    for (const split of fixture.card.splits) {
      const opened = openCardCached(split.envelope as Record<string, unknown>, keys)
      expect(opened.status).toBe(`opened`)
      // The clear `pictures` is a projection, never laid over the sealed ones.
      const { pictures: _projected, ...clear } = split.clear as Record<string, unknown>
      expect(opened.card).toEqual({ ...split.card, ...clear })
    }
    // The server flips a clear flag without re-sealing: the clear value wins.
    const superseded = { ...fixture.card.splits[2]!.envelope, decision: `superseded` }
    expect(openCardCached(superseded, keys).card.decision).toBe(`superseded`)
  })

  it(`a key arriving (a new keys map) re-opens what was locked`, () => {
    const before = openMessageWith({ body: fixture.body.envelope, card: null }, ROOT, source(new Map()))
    expect(before.locked).toBe(`noKey`)
    const after = openMessageWith({ body: fixture.body.envelope, card: null }, ROOT, source(keysOf({ 1: fixture.tree.secret })))
    expect(after).toEqual({ body: fixture.body.text, card: null, locked: null })
  })
})

describe(`openTitleWith / openDigestMessages`, () => {
  it(`opens a sealed title, else null (the neutral title stays)`, () => {
    const keys = source(keysOf({ 1: fixture.tree.secret }))
    expect(openTitleWith(fixture.body.envelope, ROOT, keys)).toBe(fixture.body.text)
    const none = source(new Map())
    expect(openTitleWith(fixture.body.envelope, ROOT, none)).toBeNull()
    expect(none.request).toHaveBeenCalledWith(ROOT, 1)
    expect(openTitleWith(null, ROOT, none)).toBeNull()
  })

  it(`opens digest messages with the session's ROOT keys`, () => {
    const keysFor = vi.fn((rootId: string) => (rootId === ROOT ? keysOf({ 1: fixture.tree.secret }) : new Map()))
    const opened = openDigestMessages(
      `child`,
      [{ id: `m`, body: fixture.body.envelope, card: null }],
      (sessionId) => (sessionId === `child` ? ROOT : sessionId),
      { keysFor, request: vi.fn() }
    )
    expect(keysFor).toHaveBeenCalledWith(ROOT)
    expect(opened[0]).toMatchObject({ body: fixture.body.text, locked: null })
  })
})

describe(`resolveSealedMirrors`, () => {
  it(`a locked escalated origin is drawn from its mirror's copy sealed to this tree`, () => {
    const split = fixture.card.splits[0]!
    // The origin is sealed to a tree this install does not hold (rotatedTree).
    const keys = (rootId: string) => (rootId === `mine` ? keysOf({ 1: fixture.tree.secret }) : new Map())
    const src = { keysFor: keys, request: vi.fn() }
    const origin = { ...openMessageWith({ body: ``, card: split.envelope as Record<string, unknown> }, `theirs`, src), id: `o1` }
    expect(origin.locked).toBe(`noKey`)
    const mirror = {
      id: `m1`,
      locked: null,
      card: { kind: `ask`, sessionId: `s-origin`, messageId: `o1`, sealedCard: split.envelope },
    }
    const out = resolveSealedMirrors([origin, mirror], (m) => (m.id === `m1` ? `mine` : `theirs`), src)
    expect(out[0]).toMatchObject({ id: `o1`, locked: null, card: split.card })
    // Without the mirror tree's key the origin stays locked.
    expect(resolveSealedMirrors([origin, mirror], () => `none`, src)[0]!.locked).toBe(`noKey`)
  })
})
