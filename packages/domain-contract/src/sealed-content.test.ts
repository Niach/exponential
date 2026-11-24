// The fixture is the contract; every mirror replays it.
import { describe, expect, test } from "bun:test"

import fixture from "../fixtures/sealed-content.json" with { type: "json" }
import pushFixture from "../fixtures/push-seal.json" with { type: "json" }
import { messageCardKindValues } from "../../db-schema/src/domain"
import {
  b64urlDecode,
  b64urlEncode,
  CARD_CLEAR_FIELDS,
  CARD_STAMP_FIELDS,
  cardItemWire,
  type CardItemAction,
  generateTreeKey,
  openBody,
  openCard,
  parseBodyEnvelope,
  pushOpen,
  pushPublicKey,
  pushSealWith,
  sealBody,
  sealBodyWith,
  sealCard,
  sealCardWith,
  splitCard,
  treeKeyFromSecret,
  unwrapTreeKey,
  wrapTreeKey,
  wrapTreeKeyWith,
  WRAPPED_TREE_KEY_BYTES,
  withAttachmentEmbeds,
  type AttachmentUpload,
  NO_KEY_REFUSAL,
  SEALING_DEVICE_MIN_VERSION,
  sendWire,
  CLEAR_TREE_REFUSAL,
  createFailure,
  keylessStep,
  rotationAfter,
  rotationEpoch,
  type CreateFailure,
  type RequestKeyAnswer,
  type SendTree,
} from "./sealed-content"

const hex = (text: string) => new Uint8Array(Buffer.from(text, `hex`))
const keys = new Map(Object.entries(fixture.keys).map(([epoch, secret]) => [Number(epoch), hex(secret)]))
type Card = Record<string, unknown>

describe(`push_seal primitive (fixtures/push-seal.json, pure JS)`, () => {
  test(`replays the shared vector byte-exact`, () => {
    const secret = hex(pushFixture.recipient.secret)
    expect(Buffer.from(pushPublicKey(secret)).toString(`hex`)).toBe(pushFixture.recipient.public)
    const v = pushFixture.vector
    const sealed = pushSealWith(hex(v.ephemeralSecret), hex(pushFixture.recipient.public), new TextEncoder().encode(v.plaintext))
    expect(Buffer.from(sealed).toString(`hex`)).toBe(v.sealed)
    expect(new TextDecoder().decode(pushOpen(secret, sealed))).toBe(v.plaintext)
    expect(new TextDecoder().decode(pushOpen(secret, hex(pushFixture.rustSealed.sealed)))).toBe(pushFixture.rustSealed.plaintext)
    for (const c of pushFixture.refused) expect(() => pushOpen(hex(c.secret), hex(c.sealed))).toThrow()
    expect(() => pushSealWith(hex(v.ephemeralSecret), hex(pushFixture.lowOrderPublic), new Uint8Array([1]))).toThrow()
  })
})

describe(`base64url`, () => {
  test(`round-trips every length and refuses non-canonical input`, () => {
    for (let n = 0; n < 40; n++) {
      const bytes = Uint8Array.from({ length: n }, (_, i) => (i * 37 + n) & 0xff)
      const text = b64urlEncode(bytes)
      expect(text).toBe(Buffer.from(bytes).toString(`base64url`))
      expect(b64urlDecode(text)).toEqual(bytes)
    }
    expect(b64urlDecode(`AQ==`)).toBeNull()
    expect(b64urlDecode(`AR`)).toBeNull()
    expect(b64urlDecode(`A`)).toBeNull()
    expect(b64urlDecode(`a+b/`)).toBeNull()
  })
})

describe(`tree keys`, () => {
  test(`fixed secrets give the fixture's public keys`, () => {
    expect(treeKeyFromSecret(hex(fixture.tree.secret)).publicKey).toBe(fixture.tree.publicKey)
    expect(treeKeyFromSecret(hex(fixture.rotatedTree.secret)).publicKey).toBe(fixture.rotatedTree.publicKey)
    expect(b64urlEncode(pushPublicKey(hex(fixture.install.secret)))).toBe(fixture.install.x25519Public)
  })
  test(`the wrap vector is byte-exact and unwraps with the install secret`, () => {
    const w = fixture.wrap
    expect(wrapTreeKeyWith(hex(w.ephemeralSecret), w.installPublic, hex(w.treeSecret))).toBe(w.wrapped)
    expect(b64urlDecode(w.wrapped)!.length).toBe(WRAPPED_TREE_KEY_BYTES)
    expect(w.bytes).toBe(WRAPPED_TREE_KEY_BYTES)
    expect(Buffer.from(unwrapTreeKey(hex(fixture.install.secret), w.wrapped)).toString(`hex`)).toBe(w.treeSecret)
    expect(() => unwrapTreeKey(hex(fixture.tree.secret), w.wrapped)).toThrow()
    expect(() => unwrapTreeKey(hex(fixture.install.secret), w.wrapped.slice(0, -4))).toThrow()
  })
  test(`a generated key wraps and unwraps; seals open`, () => {
    const tree = generateTreeKey()
    expect(tree.publicKey).toHaveLength(43)
    const wrapped = wrapTreeKey(fixture.install.x25519Public, tree.secret)
    expect(unwrapTreeKey(hex(fixture.install.secret), wrapped)).toEqual(tree.secret)
    const own = new Map([[3, tree.secret]])
    expect(openBody(sealBody(tree.publicKey, 3, `hi`), own)).toEqual({ status: `opened`, epoch: 3, text: `hi` })
    const card = sealCard(tree.publicKey, 3, { kind: `report`, sessionId: `s`, summary: `done` })
    expect(card).toEqual({ kind: `report`, sessionId: `s`, sealed: expect.stringMatching(/^v1:3:/) })
    expect(openCard(card, own).card).toEqual({ kind: `report`, sessionId: `s`, summary: `done` })
  })
})

describe(`body envelope`, () => {
  test(`the vector is byte-exact`, () => {
    const b = fixture.body
    expect(sealBodyWith(hex(b.ephemeralSecret), b.treePublicKey, b.epoch, b.text)).toBe(b.envelope)
    const parsed = parseBodyEnvelope(b.envelope)
    expect(parsed.kind).toBe(`sealed`)
  })
  for (const c of fixture.attachmentEmbedCases) {
    test(`attachment lines: ${c.name}`, () => {
      const uploads = c.uploads as Record<string, AttachmentUpload>
      expect(withAttachmentEmbeds(c.text, c.ids, (id) => uploads[id])).toBe(c.expected)
    })
  }

  for (const c of fixture.bodyCases) {
    test(c.name, () => {
      expect(openBody(c.body, keys)).toEqual({ status: c.status as never, epoch: c.epoch, text: c.text as never })
      const parsed = parseBodyEnvelope(c.body)
      if (c.status === `plain`) expect(parsed).toEqual({ kind: `plain`, text: c.body })
      if (c.status === `unsupported`) expect(parsed).toEqual({ kind: `unsupported`, version: (c as { version?: string }).version! })
      if (c.status === `malformed`) expect(parsed).toEqual({ kind: `malformed` })
    })
  }
})

describe(`card envelope`, () => {
  test(`the allowlist is the contract's`, () => {
    expect([...CARD_CLEAR_FIELDS]).toEqual(fixture.cardClearFields)
  })
  test(`the splits cover every MessageCard kind`, () => {
    expect(new Set(fixture.card.splits.map((s) => s.card.kind))).toEqual(new Set(messageCardKindValues))
  })
  for (const s of fixture.card.splits) {
    test(`split + seal: ${s.name}`, () => {
      const split = splitCard(s.card as Card)
      expect(split.clear).toEqual(s.clear)
      expect(split.plaintext).toBe(s.plaintext)
      expect(sealCardWith(hex(fixture.card.ephemeralSecret), fixture.card.treePublicKey, fixture.card.epoch, s.card as Card)).toEqual(s.envelope)
      for (const key of Object.keys(s.clear)) expect(fixture.cardClearFields).toContain(key)
      const opened = openCard(s.envelope as Card, keys)
      expect(opened.status).toBe(`opened`)
      // The clear `pictures` is a projection, never laid over the card.
      const { pictures: _projected, ...over } = s.clear as Card
      expect(opened.card).toEqual({ ...(s.card as Card), ...over })
    })
  }
  for (const c of fixture.cardCases) {
    test(c.name, () => {
      const opened = openCard(c.card as Card, keys)
      expect(opened.status).toBe(c.status as never)
      expect(opened.epoch).toBe(c.epoch)
      expect(opened.card).toEqual(c.opened as Card)
    })
  }
})

describe(`card stamps (cardStamps, #EXP-1958)`, () => {
  const splits = new Map(fixture.card.splits.map((s) => [s.name, s]))
  test(`the stamp fields are the contract's`, () => {
    expect([...CARD_STAMP_FIELDS]).toEqual(fixture.cardStamps.fields as never)
  })
  for (const c of fixture.cardStamps.openCases) {
    test(c.name, () => {
      const stored = { ...(splits.get(c.stored)!.envelope as Card), stamps: c.stamps }
      const opened = openCard(stored, c.keys === `tree` ? keys : new Map())
      expect(opened.status).toBe(c.status as never)
      if (c.items) {
        expect(opened.card.items).toEqual(c.items)
        expect(`stamps` in opened.card).toBe(false)
      } else {
        expect(opened.card).toEqual(c.card as Card)
      }
    })
  }
  for (const c of fixture.cardStamps.tapCases) {
    test(c.name, () => {
      expect(cardItemWire({ sealed: c.stored === `sealed`, opened: c.opened, action: c.action as CardItemAction })).toEqual({
        wire: c.wire as never,
        call: c.call,
      })
    })
  }
})

describe(`sealed by default (clearTree, #EDE-97)`, () => {
  for (const c of fixture.clearTree.writerCases) {
    test(c.name, () => {
      expect(
        sendWire({
          tree: c.tree as SendTree,
          holder: c.holder as `ready` | `late` | `none`,
          reader: (c as { reader?: boolean }).reader,
          keyArrives: (c as { keyArrives?: boolean }).keyArrives,
        })
      ).toBe(c.wire as never)
    })
  }
  for (const c of fixture.clearTree.keylessCases) {
    test(`keyless (#EXP-1960): ${c.name}`, () => {
      const row = c as { failure?: CreateFailure; attempt?: number; ownOnly?: boolean; answer?: RequestKeyAnswer; step: string }
      const step = row.answer
        ? keylessStep({ answer: row.answer })
        : keylessStep({ failure: row.failure!, attempt: row.attempt, ownOnly: row.ownOnly })
      expect(step).toBe(row.step as never)
    })
  }
  test(`a create failure is read from its tRPC code and message`, () => {
    expect(createFailure({ message: CLEAR_TREE_REFUSAL, code: `BAD_REQUEST` })).toBe(`clear`)
    expect(createFailure({ code: `CONFLICT`, message: `The tree is at epoch 3` })).toBe(`stale`)
    expect(createFailure({ code: `BAD_REQUEST`, message: fixture.clearTree.wrapRefusal })).toBe(`wrapRefused`)
    expect(createFailure({ code: `NOT_FOUND`, message: `Session not found` })).toBe(`notReader`)
    expect(createFailure({ code: `FORBIDDEN`, message: `Not your install` })).toBe(`refused`)
    expect(createFailure({ code: `INTERNAL_SERVER_ERROR` })).toBe(`transient`)
    expect(createFailure({ code: `TOO_MANY_REQUESTS` })).toBe(`transient`)
    expect(createFailure({ message: `Failed to fetch` })).toBe(`transient`)
  })
  test(`the copy and the floor come from the fixture`, () => {
    expect(NO_KEY_REFUSAL).toBe(fixture.clearTree.noKeyRefusal)
    expect(SEALING_DEVICE_MIN_VERSION).toBe(`0.14.96`)
  })
})

describe(`rotation (#EXP-1951)`, () => {
  const hex = (text: string) => Uint8Array.from(text.match(/../g)!.map((byte) => parseInt(byte, 16)))
  for (const c of fixture.rotation.stepCases) {
    test(`step: ${c.name}`, () => {
      expect(rotationEpoch({ flagged: c.flagged, current: c.current, held: c.held, left: c.left })).toBe(c.epoch)
    })
  }
  for (const c of fixture.rotation.answerCases) {
    test(`answer: ${c.name}`, () => {
      expect(rotationAfter(c.failure as CreateFailure | null)).toBe(c.then as never)
    })
  }
  test(`the vectors: the other holder's key opens for the install, a foreign row does not`, () => {
    const secret = hex(fixture.install.secret)
    expect(unwrapTreeKey(secret, fixture.rotation.rotatedWrap)).toEqual(hex(fixture.rotatedTree.secret))
    expect(() => unwrapTreeKey(secret, fixture.rotation.foreignWrap)).toThrow()
  })
  test(`every holder case names known answers and only wraps listed readers`, () => {
    for (const c of fixture.rotation.holderCases) {
      for (const answer of c.create) expect([`ok`, `blip`, `CONFLICT`, `FORBIDDEN`, `NOT_FOUND`]).toContain(answer)
      for (const create of c.creates) expect(create.wraps).toEqual([`self`])
      for (const wrap of c.wraps) expect(c.readers).toContain(wrap.to)
    }
  })
})
