// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import type { TreeKeys } from "@exp/domain-contract/sealed-content"
import { openCardCached, openDigestMessages, openMessageWith, openTitleWith, type KeySource } from "./sealed-open"

// EXP-1459: the web's opening layer replays fixtures/sealed-content.json
// (`placeholder.messageCases`, the body vector, every card split).

const hex = (text: string) => Uint8Array.from(text.match(/../g)!.map((b) => parseInt(b, 16)))
const keysOf = (keys: Record<string, string>): TreeKeys =>
  new Map(Object.entries(keys).map(([epoch, secret]) => [Number(epoch), hex(secret)]))
const source = (keys: TreeKeys): KeySource & { request: ReturnType<typeof vi.fn> } => ({
  keysFor: () => keys,
  request: vi.fn(),
})
const ROOT = `r1`

describe(`openMessageWith`, () => {
  for (const c of fixture.placeholder.messageCases) {
    it(`placeholder: ${c.name}`, () => {
      const keys = source(keysOf(c.keys))
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
      expect(opened.card).toEqual({ ...split.card, ...split.clear })
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
