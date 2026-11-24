// @vitest-environment node
import { beforeEach, describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"

// #EXP-1958 (sealed-content.json `cardStamps.tapCases`): which call a web tap
// makes, by the stored card. The sealing itself is `sendSessionMessage`'s
// (seal-send.test.ts); here it is the seam.

const hex = (text: string) => Uint8Array.from(text.match(/../g)!.map((b) => parseInt(b, 16)))
const rows = new Map<string, Record<string, unknown>>()
const send = vi.fn(async (..._args: unknown[]) => ({ sessionId: `s` }))
const calls: Array<[string, unknown]> = []
const mutation = (name: string) => ({ mutate: vi.fn(async (input: unknown) => (calls.push([name, input]), {})) })
let keyed = true

vi.mock(`@/lib/collections`, () => ({
  messageCollection: { get: (id: string) => rows.get(id) },
  issueCollection: {
    values: () =>
      [
        // Real-shaped: the issues shape carries no team_id (#EXP-2082).
        { id: `00000000-0000-4000-8000-000000000077`, identifier: `EXP-7`, boardId: `b-t` },
        { id: `00000000-0000-4000-8000-000000000088`, identifier: `EXP-7`, boardId: `b-u` },
      ][Symbol.iterator](),
  },
  boardCollection: { get: (id: string) => ({ "b-t": { id, teamId: `t` }, "b-u": { id, teamId: `u` } })[id] },
  // #EXP-2079: the card's session sits in a team that is not synced (the
  // Assistant's conversation in its member's personal team): its refs
  // resolve over the chat teams, the first team's on a clash.
  sessionCollection: { get: (id: string) => (id === `card-s` ? { id, rootId: `root`, teamId: `personal` } : undefined) },
  teamCollection: {
    toArray: [
      { id: `u`, name: `Zeta` },
      { id: `t`, name: `Acme` },
    ],
    get: () => undefined,
  },
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    sessions: Object.fromEntries(
      [`sendReply`, `startSuggestion`, `startAllSuggestions`, `dismissSuggestion`, `dismissReplies`, `fileSuggestion`].map((n) => [n, mutation(n)])
    ),
  },
}))
vi.mock(`./current-key-holder`, () => ({
  currentKeyHolder: () => ({ keysFor: () => (keyed ? new Map([[1, hex(fixture.keys[`1`])]]) : new Map()) }),
}))
vi.mock(`./send-sealed`, () => ({ sendSessionMessage: (...args: unknown[]) => send(...args) }))

const { tapCardItem } = await import(`./card-taps`)
const { sealCard } = await import(`@exp/domain-contract/sealed-content`)
const split = (name: string) => fixture.card.splits.find((s) => s.name === name)!.envelope as Record<string, unknown>

beforeEach(() => {
  rows.clear()
  calls.length = 0
  send.mockClear()
  keyed = true
  rows.set(`plain-r`, { id: `plain-r`, sessionId: `card-s`, rootId: `root`, card: { kind: `replies`, items: [{ id: `r1`, text: `x` }] } })
  rows.set(`plain-s`, { id: `plain-s`, sessionId: `card-s`, rootId: `root`, card: { kind: `suggestions`, items: [{ id: `s1`, title: `T`, text: `x` }] } })
  rows.set(`sealed-r`, { id: `sealed-r`, sessionId: `card-s`, rootId: `root`, card: split(`replies`) })
  // The split's item carries an `issueId` (filed): an open one here.
  const open = sealCard(fixture.tree.publicKey, 1, { kind: `suggestions`, items: [{ id: `s1`, title: `Add tests`, text: `Cover the readers rule` }] })
  rows.set(`sealed-s`, { id: `sealed-s`, sessionId: `card-s`, rootId: `root`, card: open })
})

describe(`a tap takes its stored card's wire`, () => {
  for (const c of fixture.cardStamps.tapCases) {
    it(c.name, async () => {
      keyed = c.opened
      const kind = [`reply`, `dismissReplies`].includes(c.action) ? `r` : `s`
      const messageId = `${c.stored}-${kind}`
      const itemId = kind === `r` ? `r1` : `s1`
      const tap = tapCardItem({ action: c.action as never, messageId, itemId })
      if (c.wire === `refused`) {
        await expect(tap).rejects.toThrow()
        expect(send).not.toHaveBeenCalled()
        expect(calls).toEqual([])
        return
      }
      await tap
      if (c.wire === `sealed`) {
        expect(calls).toEqual([])
        expect(send).toHaveBeenCalledTimes(1)
        const [input, , extra] = send.mock.calls[0] as [Record<string, unknown>, unknown, unknown]
        expect(extra).toEqual({ cardItem: { messageId, itemId } })
        if (c.action === `reply`) expect(input).toEqual({ sessionId: `card-s`, text: `Yes, ship it` })
        else if (c.action === `here`) expect(input).toEqual({ sessionId: `card-s`, text: `Add tests\n\nCover the readers rule` })
        else expect(input).toEqual({ parentId: `card-s`, text: `Add tests\n\nCover the readers rule`, name: `Add tests` })
      } else {
        expect(send).not.toHaveBeenCalled()
        expect(calls.map(([name]) => `sessions.${name}`)).toEqual([c.call])
        if (c.wire === `stamp` && c.action === `dismissReplies`) expect(calls[0]![1]).toEqual({ messageId, itemIds: [`r1`] })
        if (c.wire === `stamp` && c.action === `file`) {
          expect(calls[0]![1]).toEqual({ messageId, itemId, title: `Add tests`, text: `Cover the readers rule` })
        }
      }
    })
  }

  it(`Start resolves the item's issue refs to ids (unknown dropped)`, async () => {
    const card = sealCard(fixture.tree.publicKey, 1, {
      kind: `suggestions`,
      items: [{ id: `s1`, title: `T`, text: `x`, issueIds: [`EXP-7`, `00000000-0000-4000-8000-000000000012`, `NOPE-1`] }],
    })
    rows.set(`m`, { id: `m`, sessionId: `card-s`, rootId: `root`, card })
    await tapCardItem({ action: `start`, messageId: `m`, itemId: `s1`, model: `opus` })
    expect(send.mock.calls[0]![0]).toMatchObject({
      issueIds: [`00000000-0000-4000-8000-000000000077`, `00000000-0000-4000-8000-000000000012`],
      model: `opus`,
    })
  })
})
