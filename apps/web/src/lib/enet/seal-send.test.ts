// @vitest-environment node
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import { b64urlEncode, openBody, openCard, pushPublicKey, unwrapTreeKey } from "@exp/domain-contract/sealed-content"
import { KeyHolder, type SessionKeysApi } from "./key-holder"
import { sealAnswerInput, sealSendInput, sendSealed, type InstallRow, type SealDeps, type SealSessionRow } from "./seal-send"
import { memoryTreeKeyStore } from "./tree-key-store"
import { answeredCard } from "@/lib/sessions/card-answer"

// EXP-1459: what the web sends (contract addenda 1+2 of #EXP-1458):
// sealed text, `seal` inline for a root the send creates, `sealedName`,
// `mentionUserIds`, `sealedCard` for an answer to a sealed card.

const hex = (text: string) => Uint8Array.from(text.match(/../g)!.map((b) => parseInt(b, 16)))
const ME = `u-me`
const INSTALL = `00000000-0000-4000-8000-0000000000aa`
const PHONE = `00000000-0000-4000-8000-0000000000bb`
const DEVICE_INSTALL = `00000000-0000-4000-8000-0000000000cc`
const OTHER_INSTALL = `00000000-0000-4000-8000-0000000000dd`
const phoneSecret = hex(`33`.repeat(32))
const deviceSecret = hex(`44`.repeat(32))
const pub = (secret: Uint8Array) => b64urlEncode(pushPublicKey(secret))

function holderWith(api: Partial<SessionKeysApi> = {}) {
  const full: SessionKeysApi = {
    mine: async () => [],
    pending: async () => [],
    wrap: async () => ({}),
    create: vi.fn(async () => ({})),
    ...api,
  }
  return { holder: new KeyHolder(ME, INSTALL, hex(fixture.install.secret), { api: full, store: memoryTreeKeyStore() }), api: full }
}

function deps(holder: KeyHolder | undefined, rows: SealSessionRow[] = []): SealDeps {
  const mine: InstallRow[] = [
    { installId: INSTALL, x25519Public: fixture.install.x25519Public },
    { installId: PHONE, x25519Public: pub(phoneSecret) },
    { installId: `revoked`, x25519Public: pub(phoneSecret), revokedAt: `2026-10-01` },
  ]
  const team: InstallRow[] = [
    ...mine,
    { installId: DEVICE_INSTALL, x25519Public: pub(deviceSecret), deviceId: `dev-row-1`, userId: `u-host` },
    { installId: OTHER_INSTALL, x25519Public: pub(hex(`55`.repeat(32))), userId: `u-other` },
  ]
  return {
    holder,
    me: ME,
    session: (id) => rows.find((row) => row.id === id),
    sessions: () => rows,
    deviceRowIds: (machine) => (machine === `machine-1` ? [`dev-row-1`] : []),
    myInstalls: async () => mine,
    teamInstalls: async () => team,
    userIdsByEmail: (emails) => emails.flatMap((email) => (email === `dana@acme.dev` ? [`u-dana`] : [])),
    issueIdsByIdentifier: (_team, identifiers) => identifiers.flatMap((id) => (id === `EXP-12` ? [`i-12`] : [])),
    sendTarget: async () => ({ rootId: null, epoch: null, publicKey: null }),
    publicKey: async () => undefined,
  }
}

const keyedRoot: SealSessionRow = {
  id: `r1`,
  rootId: `r1`,
  parentId: null,
  teamId: `t1`,
  userId: ME,
  deviceId: `machine-1`,
  shared: false,
  sealPublicKey: fixture.tree.publicKey,
  sealEpoch: 1,
}

describe(`sealSendInput`, () => {
  it(`a new chat carries its first key inline, wrapped for my installs and the target device`, async () => {
    const { holder } = holderWith()
    const sealed = await sealSendInput(
      { teamId: `t1`, deviceId: `machine-1`, text: `Ship it @dana@acme.dev #EXP-12 #NOPE-1`, name: `Launch` },
      deps(holder)
    )
    const { seal, text, sealedName, mentionUserIds, refIssueIds } = sealed.input
    expect(sealed.input.name).toBeUndefined()
    expect(mentionUserIds).toEqual([`u-dana`])
    expect(refIssueIds).toEqual([`i-12`])
    expect(seal?.installId).toBe(INSTALL)
    expect(seal?.epoch).toBe(1)
    expect(seal?.wraps.map((w) => w.installId).sort()).toEqual([INSTALL, PHONE, DEVICE_INSTALL].sort())
    const deviceWrap = seal!.wraps.find((w) => w.installId === DEVICE_INSTALL)!
    const secret = unwrapTreeKey(deviceSecret, deviceWrap.wrapped)
    const keys = new Map([[1, secret]])
    expect(openBody(text!, keys)).toEqual({ status: `opened`, epoch: 1, text: `Ship it @dana@acme.dev #EXP-12 #NOPE-1` })
    expect(openBody(sealedName!, keys).text).toBe(`Launch`)
    // The send's result names the new root: this install keeps the key.
    await sealed.after({ sessionId: `new-root` })
    expect(holder.current(`new-root`)?.publicKey).toBe(seal!.publicKey)
  })

  it(`a message into a keyed tree seals to the root's synced public key`, async () => {
    const child = { ...keyedRoot, id: `c1`, parentId: `r1`, sealPublicKey: null, sealEpoch: null }
    const sealed = await sealSendInput({ sessionId: `c1`, text: `next` }, deps(holderWith().holder, [keyedRoot, child]))
    expect(sealed.input.seal).toBeUndefined()
    expect(openBody(sealed.input.text!, new Map([[1, hex(fixture.tree.secret)]])).text).toBe(`next`)
  })

  it(`an existing keyless root gets its key first (sessionKeys.create)`, async () => {
    const { holder, api } = holderWith()
    const root = { ...keyedRoot, sealPublicKey: null, sealEpoch: null }
    const sealed = await sealSendInput({ sessionId: `r1`, text: `hi` }, deps(holder, [root]))
    expect(api.create).toHaveBeenCalledTimes(1)
    const made = (api.create as ReturnType<typeof vi.fn>).mock.calls[0]![0]
    expect(made.rootId).toBe(`r1`)
    expect(made.wraps.map((w: { installId: string }) => w.installId).sort()).toEqual([INSTALL, PHONE, DEVICE_INSTALL].sort())
    expect(openBody(sealed.input.text!, holder.keysFor(`r1`)).text).toBe(`hi`)
  })

  it(`a keyless root this install cannot key stays plaintext (legal), mentions still clear`, async () => {
    const { holder } = holderWith({ create: vi.fn(async () => Promise.reject(new Error(`NOT_FOUND`))) })
    const root = { ...keyedRoot, userId: `u-other`, sealPublicKey: null, sealEpoch: null }
    const sealed = await sealSendInput({ sessionId: `r1`, text: `hey @dana@acme.dev` }, deps(holder, [root]))
    expect(sealed.input.text).toBe(`hey @dana@acme.dev`)
    expect(sealed.input.mentionUserIds).toEqual([`u-dana`])
  })

  it(`the first message to an agent creates its conversation's key inline`, async () => {
    const sealed = await sealSendInput({ agentId: `a1`, text: `hello` }, deps(holderWith().holder))
    expect(sealed.input.seal?.wraps.length).toBeGreaterThan(0)
    // An existing conversation with that agent is a keyed root: no inline key.
    const conversation = { ...keyedRoot, id: `conv`, rootId: `conv`, agentId: `a1` }
    const again = await sealSendInput({ agentId: `a1`, text: `hello` }, deps(holderWith().holder, [conversation]))
    expect(again.input.seal).toBeUndefined()
  })

  it(`without a key holder nothing changes`, async () => {
    const input = { sessionId: `r1`, text: `plain` }
    expect((await sealSendInput(input, deps(undefined, [keyedRoot]))).input).toBe(input)
  })
})

describe(`sealAnswerInput`, () => {
  it(`re-seals the FULL card with the answer, clear answered`, async () => {
    const { holder } = holderWith({ mine: async () => [{ rootId: `r1`, epoch: 1, wrapped: fixture.wrap.wrapped }] })
    await holder.start()
    const open = fixture.card.splits[0]!
    const out = sealAnswerInput({ card: open.envelope, rootId: `r1` }, keyedRoot, { answers: [`Postgres`] }, holder)
    expect(`sealedCard` in out).toBe(true)
    const sealedCard = (out as { sealedCard: Record<string, unknown> }).sealedCard
    expect(sealedCard.answered).toBe(true)
    expect(sealedCard.question).toBeUndefined()
    const opened = openCard(sealedCard, new Map([[1, hex(fixture.tree.secret)]]))
    expect(opened.card).toMatchObject({ kind: `question`, question: `Which database?`, answer: `Postgres`, answers: [`Postgres`] })
  })

  it(`a plan decision keeps decision + fresh in clear`, async () => {
    const { holder } = holderWith({ mine: async () => [{ rootId: `r1`, epoch: 1, wrapped: fixture.wrap.wrapped }] })
    await holder.start()
    const plan = fixture.card.splits[2]!
    const out = sealAnswerInput({ card: plan.envelope, rootId: `r1` }, keyedRoot, { decision: `approved`, fresh: true, answers: { scope: [`All`] } }, holder) as {
      decision: string
      fresh: boolean
      sealedCard: Record<string, unknown>
    }
    expect(out.decision).toBe(`approved`)
    expect(out.fresh).toBe(true)
    expect(out.sealedCard).toMatchObject({ kind: `plan`, decision: `approved`, fresh: true })
  })

  it(`a plaintext card answers as before; a sealed one without its key refuses`, () => {
    const input = { answer: `Postgres` }
    expect(sealAnswerInput({ card: { kind: `question`, question: `?` }, rootId: `r1` }, keyedRoot, input, undefined)).toBe(input)
    expect(() => sealAnswerInput({ card: fixture.card.splits[0]!.envelope, rootId: `r1` }, keyedRoot, input, undefined)).toThrow()
  })
})

describe(`sealAnswerInput (escalated stand-in)`, () => {
  it(`answers an origin this install cannot open from the mirror's copy, sealed to the origin tree`, () => {
    const open = fixture.card.splits[0]!
    const out = sealAnswerInput(
      { card: open.envelope, rootId: `origin-root` },
      undefined,
      { answers: [`SQLite`] },
      { keysFor: () => new Map(), current: () => undefined },
      { card: open.card, key: { epoch: 2, publicKey: fixture.rotatedTree.publicKey } }
    ) as { sealedCard: Record<string, unknown> }
    const opened = openCard(out.sealedCard, new Map([[2, hex(fixture.rotatedTree.secret)]]))
    expect(opened.card).toMatchObject({ kind: `question`, answer: `SQLite` })
  })
})

// EXP-1486: the fixture's answer cases end to end: the stored card alone
// picks the wire, and the server's `answeredCard` accepts every one.
describe(`sealAnswerInput (fixture answerCases)`, () => {
  type Card = Record<string, unknown>
  const storedCard = (stored: Card): Card =>
    typeof stored.split === `string` ? (fixture.card.splits.find((s) => s.name === stored.split)!.envelope as Card) : stored
  for (const c of fixture.answerCases as Array<Record<string, unknown> & { name: string; input: Record<string, unknown> }>) {
    it(c.name, async () => {
      const card = storedCard(c.stored as Card)
      let holder: KeyHolder | undefined
      if (c.keys === `tree`) {
        holder = holderWith({ mine: async () => [{ rootId: `r1`, epoch: 1, wrapped: fixture.wrap.wrapped }] }).holder
        await holder.start()
      }
      const root = c.keys === `tree` ? keyedRoot : { ...keyedRoot, sealPublicKey: null, sealEpoch: null }
      const run = () => sealAnswerInput({ card: card as never, rootId: `r1` }, root, c.input as never, holder)
      if (c.wire === `refused`) {
        expect(run).toThrow()
        return
      }
      const out = run() as Record<string, unknown>
      if (c.wire === `clear`) {
        expect(out).toBe(c.input)
      } else {
        const sealedCard = out.sealedCard as Card
        expect(Object.keys(out).sort()).toEqual([`sealedCard`, ...Object.keys(c.input).filter((k) => k === `decision` || k === `fresh`)].sort())
        expect(sealedCard).toMatchObject(c.clear as Card)
        expect(openCard(sealedCard, new Map([[1, hex(fixture.tree.secret)]])).card).toMatchObject(c.opened as Card)
      }
      const stamped = answeredCard(card as never, out as never)
      expect(stamped.ok).toBe(true)
      expect(stamped.ok && stamped.card).toMatchObject(c.server as Card)
    })
  }
})

// #FEED-248 (sealed-content.json `staleTree`): the synced store is a cache.
describe(`sealSendInput (fixture staleTree)`, () => {
  const stale = fixture.staleTree
  const treeKey = { epoch: 1, publicKey: fixture.tree.publicKey }
  for (const c of stale.cases) {
    it(c.name, async () => {
      const asks: string[] = []
      const serverKeyed = c.server === `keyed`
      const { holder } = holderWith({
        create: vi.fn(async () => {
          asks.push(`create`)
          if (serverKeyed) throw new Error(stale.refusal)
          return {}
        }),
      })
      const local =
        c.local === `missing`
          ? []
          : [{ ...keyedRoot, agentId: `a1`, ...(c.local === `keyless` ? { sealPublicKey: null, sealEpoch: null } : {}) }]
      const base = deps(holder, local)
      const d: SealDeps = {
        ...base,
        sendTarget: async () => {
          asks.push(`sendTarget`)
          return c.server === `none`
            ? { rootId: null, epoch: null, publicKey: null }
            : { rootId: `r1`, ...(serverKeyed ? treeKey : { epoch: null, publicKey: null }) }
        },
        publicKey: async () => {
          asks.push(`publicKeys`)
          return serverKeyed ? treeKey : undefined
        },
      }
      const input = c.send === `agent` ? { agentId: `a1`, teamId: `t1`, text: `hi` } : { sessionId: `r1`, text: `hi` }
      const sealed = await sealSendInput(input, d)
      expect(asks).toEqual(c.asks)
      if (c.wire === `inline`) {
        expect(sealed.input.seal?.epoch).toBe(1)
        return
      }
      expect(sealed.input.seal).toBeUndefined()
      const keys = c.wire === `create` ? holder.keysFor(`r1`) : new Map([[1, hex(fixture.tree.secret)]])
      expect(openBody(sealed.input.text!, keys).text).toBe(`hi`)
    })
  }

  for (const c of stale.retryCases) {
    it(c.name, async () => {
      // The tab missed the keyed conversation; the server knows it.
      const d: SealDeps = { ...deps(holderWith().holder), sendTarget: async () => ({ rootId: `r1`, ...treeKey }) }
      const sent: Array<Record<string, unknown>> = []
      let refusals = c.refusals
      const run = sendSealed({ agentId: `a1`, teamId: `t1`, text: `hi` }, d, async (input) => {
        sent.push(input)
        if (refusals-- > 0) throw new Error(stale.refusal)
        return { sessionId: `r1` }
      })
      if (c.outcome === `sent`) await expect(run).resolves.toEqual({ sessionId: `r1` })
      else await expect(run).rejects.toThrow(stale.refusal)
      expect(sent).toHaveLength(c.sends)
      // The re-send is sealed to the server's key, never a new inline one.
      expect(sent.at(-1)!.seal).toBeUndefined()
      expect(openBody(sent.at(-1)!.text as string, new Map([[1, hex(fixture.tree.secret)]])).text).toBe(`hi`)
    })
  }

  it(`an older server's raw epoch refusal is re-sealed too; anything else is not`, async () => {
    const d = deps(holderWith().holder, [keyedRoot])
    let calls = 0
    await sendSealed({ sessionId: `r1`, text: `hi` }, d, async () => {
      if (calls++ === 0) throw new Error(`The tree is at epoch 3`)
      return { sessionId: `r1` }
    })
    expect(calls).toBe(2)
    calls = 0
    await expect(
      sendSealed({ sessionId: `r1`, text: `hi` }, d, async () => {
        calls++
        throw new Error(`Session not found`)
      })
    ).rejects.toThrow(`Session not found`)
    expect(calls).toBe(1)
  })
})
