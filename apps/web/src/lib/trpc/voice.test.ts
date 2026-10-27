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
vi.mock(`@/lib/steer`, async () => ({
  ...(await vi.importActual<typeof import("@/lib/steer")>(`@/lib/steer`)),
  getSteerRelayConfig: () => ({ url: `https://relay.example`, secret: `relay-secret` }),
  relayPostNudge: h.relayPostNudge,
}))

import callFixture from "@exp/domain-contract/fixtures/voice-call.json" with { type: "json" }
import { verifySteerTicket } from "@exp/steer-ticket"
import { callSpeechOf, voiceRouter, withClearOnNone } from "@/lib/trpc/voice"

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
  language: `en`,
  voiceFile: ``,
  voiceTranscript: ``,
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

describe(`#NEW-73 language and custom voice`, () => {
  it(`stores German and a trimmed voice file + transcript`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    const next = await caller.update({ language: `de`, voiceFile: `  /home/me/ref.wav `, voiceTranscript: ` Hallo zusammen. ` })
    expect(next).toMatchObject({ language: `de`, voiceFile: `/home/me/ref.wav`, voiceTranscript: `Hallo zusammen.` })
  })

  it(`refuses an unknown language and a multi-line transcript`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }], [{ userId: `me`, ...stored }]]
    await expect(caller.update({ language: `fr` as never })).rejects.toThrow()
    await expect(caller.update({ voiceTranscript: `one\ntwo` })).rejects.toThrow()
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

// NEW-55: the call's availability + the caller/host relay tickets.
describe(`voice.startCall`, () => {
  const doctor = (state: string | null) => ({
    checkedAt: new Date().toISOString(),
    items: state === null ? [] : [{ key: `speech`, group: `voice`, state }],
  })
  const deviceRow = (patch: Record<string, unknown> = {}) => ({
    label: `mint`,
    lastSeenAt: new Date(),
    doctor: doctor(`ok`),
    ...patch,
  })
  const reasons = callFixture.copy.reasons
  const refused = (message: string) => ({ code: `PRECONDITION_FAILED`, message })

  it(`mints a caller ticket ringing the member's voice device`, async () => {
    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored }], [deviceRow()]]
    const out = await caller.startCall({ agentId: AGENT, platform: `phone` })
    expect(out.relayUrl).toBe(`https://relay.example`)
    expect(out.url).toBe(`wss://relay.example/ws?ticket=${encodeURIComponent(out.ticket)}`)
    const verdict = verifySteerTicket(out.ticket, `relay-secret`)
    expect(verdict.ok).toBe(true)
    if (verdict.ok) {
      expect(verdict.claims).toMatchObject({
        role: `caller`,
        sub: `me`,
        callId: out.callId,
        agentId: AGENT,
        deviceId: `d-mint`,
        deviceOwnerId: `me`,
      })
    }
  })

  it(`refuses an agent outside the member's teams`, async () => {
    h.state.selectQueue = [[]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`refuses with the fixture's reason, first match wins`, async () => {
    h.state.selectQueue = [[{ id: AGENT }], []]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(refused(reasons.noVoiceDevice))

    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored, speakReplies: false }], [deviceRow()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject(refused(reasons.speakOff))

    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored }], [deviceRow({ lastSeenAt: new Date(0) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(refused(`mint is offline`))

    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored }], [deviceRow()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `desktop`, deviceId: `d-other` })).rejects.toMatchObject(
      refused(`Calls run on mint`)
    )

    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored }], [deviceRow({ doctor: null })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(
      refused(`mint is getting its voice ready`)
    )

    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored }], [deviceRow({ doctor: doctor(`missing`) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject(
      refused(`mint can't speak: no usable GPU`)
    )
  })

  it(`reads the speech doctor row`, () => {
    expect(callSpeechOf(null)).toBeNull()
    expect(callSpeechOf(doctor(null))).toBeNull()
    expect(callSpeechOf(doctor(`ok`))).toBe(`ok`)
    expect(callSpeechOf(doctor(`missing`))).toBe(`noGpu`)
    expect(callSpeechOf(doctor(`action`))).toBe(`preparing`)
    // A `speech` key in another group is not the voice's.
    expect(callSpeechOf({ checkedAt: ``, items: [{ key: `speech`, group: `required`, state: `ok` }] })).toBeNull()
  })
})

describe(`voice.callHostTicket`, () => {
  it(`mints a call_host ticket for the member`, async () => {
    const callId = `7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d`
    const out = await caller.callHostTicket({ callId })
    expect(out.url).toBe(`wss://relay.example/ws?ticket=${encodeURIComponent(out.ticket)}`)
    const verdict = verifySteerTicket(out.ticket, `relay-secret`)
    expect(verdict.ok && verdict.claims).toMatchObject({ role: `call_host`, sub: `me`, callId })
  })
})
