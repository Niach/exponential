import { beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "@exp/domain-contract/fixtures/voice.json" with { type: "json" }

const h = vi.hoisted(() => {
  const state = {
    // One entry per select, in call order: the stored row, then the device
    // probe, then the agent probe (as the mutation reaches them).
    selectQueue: [] as unknown[][],
    upserts: [] as { values: Record<string, unknown>; set: Record<string, unknown> }[],
  }
  const chain = () => {
    const rows = () => Promise.resolve(state.selectQueue.shift() ?? [])
    const b = {
      from: () => b,
      where: () => ({ limit: rows, then: (f: (v: unknown) => unknown) => rows().then(f) }),
    }
    return b
  }
  const db = {
    select: () => chain(),
    insert: () => ({
      values: (values: Record<string, unknown>) => ({
        onConflictDoUpdate: ({ set }: { set: Record<string, unknown> }) => {
          state.upserts.push({ values, set })
          return Promise.resolve()
        },
      }),
    }),
  }
  return { state, db, relayPostNudge: vi.fn(async () => ({ delivered: true })) }
})

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/steer`, () => ({
  getSteerRelayConfig: () => ({ url: `ws://relay` }),
  relayPostNudge: h.relayPostNudge,
}))

import { voiceRouter, withClearOnNone } from "@/lib/trpc/voice"

const caller = voiceRouter.createCaller({
  session: { user: { id: `me`, name: `Me`, email: `me@example.com` } },
  db: h.db,
  request: new Request(`http://localhost/`),
} as never)

const AGENT = `3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d`
const stored = {
  voiceDeviceId: `d-mint`,
  agentId: AGENT,
  speakReplies: true,
  pushToTalk: true,
  pttShortcut: `ctrl+space`,
  wakeWord: false,
  wakePhrase: ``,
}

beforeEach(() => {
  h.state.selectQueue = []
  h.state.upserts = []
  h.relayPostNudge.mockClear()
})

describe(`voice.get`, () => {
  it(`a member without a row gets the fixture's defaults`, async () => {
    expect(await caller.get()).toEqual(fixture.defaults)
  })

  it(`returns the stored row`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, createdAt: new Date(), updatedAt: new Date() }]]
    expect(await caller.get()).toEqual(stored)
  })
})

describe(`voice.update`, () => {
  it(`stores a desktop and an agent of the member's teams, and nudges the device`, async () => {
    h.state.selectQueue = [[], [{ kind: `desktop` }], [{ id: AGENT }]]
    const next = await caller.update({ voiceDeviceId: `d-mint`, agentId: AGENT, speakReplies: true })
    expect(next).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT, speakReplies: true })
    expect(h.state.upserts[0]?.set).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT, speakReplies: true })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
  })

  it(`refuses a phone or a foreign device as the voice device`, async () => {
    h.state.selectQueue = [[], [{ kind: `phone` }]]
    await expect(caller.update({ voiceDeviceId: `d-phone` })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    h.state.selectQueue = [[], []]
    await expect(caller.update({ voiceDeviceId: `d-other` })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(h.state.upserts).toHaveLength(0)
  })

  it(`refuses an agent outside the member's teams`, async () => {
    h.state.selectQueue = [[], []]
    await expect(caller.update({ agentId: AGENT })).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`choosing None for the agent clears every switch and nudges the old device`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    const next = await caller.update({ agentId: null })
    expect(next).toMatchObject({ agentId: null, speakReplies: false, pushToTalk: false, wakeWord: false })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
  })

  it(`trims the wake phrase and refuses an unknown shortcut`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    expect(await caller.update({ wakePhrase: `  Hey Ops  ` })).toMatchObject({ wakePhrase: `Hey Ops` })
    await expect(caller.update({ pttShortcut: `ctrl+q` as never })).rejects.toThrow()
  })
})

describe(`#NEW-71`, () => {
  it(`a deleted agent (agent_id nulled by the FK) reads with every switch off`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, agentId: null }]]
    expect(await caller.get()).toMatchObject({ agentId: null, speakReplies: false, pushToTalk: false, wakeWord: false })
  })

  it(`picking a new agent after that never turns speaking back on`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, agentId: null }], [{ id: AGENT }]]
    const next = await caller.update({ agentId: AGENT })
    expect(next).toMatchObject({ agentId: AGENT, speakReplies: false, pushToTalk: false, wakeWord: false })
  })

  it(`refuses a wake phrase with a newline or a control character`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }], [{ userId: `me`, ...stored }]]
    await expect(caller.update({ wakePhrase: `Hey\nOps` })).rejects.toThrow()
    await expect(caller.update({ wakePhrase: `Hey\u0007Ops` })).rejects.toThrow()
    expect(h.state.upserts).toHaveLength(0)
  })
})

describe(`withClearOnNone matches the fixture`, () => {
  for (const c of fixture.clearOnNoneCases) {
    it(c.name, () => {
      const before = { ...fixture.defaults, ...c.before }
      expect(withClearOnNone({ ...before, ...c.change } as never)).toMatchObject(c.after)
    })
  }
})
