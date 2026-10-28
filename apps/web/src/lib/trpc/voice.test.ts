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
  return {
    state,
    db,
    relayPostNudge: vi.fn(async () => ({ delivered: true })),
    meetingAgentIsPrivate: vi.fn(async (..._args: unknown[]) => true),
  }
})

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/sessions/meeting-access`, async () => ({
  MEETING_SHARED_REFUSED: (await import(`@exp/domain-contract/meeting`)).MEETING_COPY.sharedRefused,
  meetingAgentIsPrivate: h.meetingAgentIsPrivate,
}))
vi.mock(`@/lib/steer`, async () => ({
  ...(await vi.importActual<typeof import("@/lib/steer")>(`@/lib/steer`)),
  getSteerRelayConfig: () => ({ url: `https://relay.example`, secret: `relay-secret` }),
  relayPostNudge: h.relayPostNudge,
}))

import callFixture from "@exp/domain-contract/fixtures/voice-call.json" with { type: "json" }
import meetingFixture from "@exp/domain-contract/fixtures/meeting.json" with { type: "json" }
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import { verifySteerTicket } from "@exp/steer-ticket"
import { callSpeechOf, voiceRouter, withClearOnNone, withMeetingScope } from "@/lib/trpc/voice"

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
  desktopAudio: `off`,
  desktopAudioApps: [] as { id: string; name: string }[],
  meetingStartedAt: null as string | null,
}
const MEETING_DEFAULTS = { desktopAudio: `off`, desktopAudioApps: [], meetingStartedAt: null }
// S-3: the legacy shims old clients read (speak-back always on, push to
// talk = a key bound).
const LEGACY_DEFAULTS = { speakReplies: true, pushToTalk: false }

beforeEach(() => {
  h.state.selectQueue = []
  h.state.upserts = []
  h.relayPostNudge.mockClear()
  h.meetingAgentIsPrivate.mockReset()
  h.meetingAgentIsPrivate.mockResolvedValue(true)
})

describe(`voice.get`, () => {
  it(`a member without a row gets the fixture's defaults`, async () => {
    expect(await caller.get()).toEqual({ ...fixture.defaults, ...LEGACY_DEFAULTS, ...MEETING_DEFAULTS })
  })

  it(`S-3: speakReplies always reads true and pushToTalk reads a bound key`, async () => {
    const at = { createdAt: new Date(), updatedAt: new Date() }
    h.state.selectQueue = [
      [{ userId: `me`, ...stored, speakReplies: false, pushToTalk: false, ...at }],
      [{ userId: `me`, ...stored, pttShortcut: ``, pushToTalk: true, ...at }],
    ]
    expect(await caller.get()).toMatchObject({ speakReplies: true, pushToTalk: true, pttShortcut: `ctrl+space` })
    expect(await caller.get()).toMatchObject({ speakReplies: true, pushToTalk: false, pttShortcut: `` })
  })

  it(`returns the stored row`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, createdAt: new Date(), updatedAt: new Date() }]]
    expect(await caller.get()).toEqual(stored)
  })
})

describe(`voice.update`, () => {
  it(`stores a desktop and an agent of the member's teams, and nudges the device`, async () => {
    h.state.selectQueue = [[], [{ kind: `desktop` }], [{ id: AGENT }]]
    const next = await caller.update({ voiceDeviceId: `d-mint`, agentId: AGENT })
    expect(next).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT })
    expect(h.state.upserts[0]?.set).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT })
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

  it(`choosing None for the agent clears the wake word (S-3: the key stays) and nudges the old device`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, wakeWord: true }]]
    const next = await caller.update({ agentId: null })
    expect(next).toMatchObject({ agentId: null, wakeWord: false, pttShortcut: `ctrl+space` })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
  })

  it(`trims the wake phrase`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    expect(await caller.update({ wakePhrase: `  Hey Ops  ` })).toMatchObject({ wakePhrase: `Hey Ops` })
  })
})

// S-3: push to talk = a key binding in `shortcutGrammar`'s canonical text.
describe(`S-3 push-to-talk binding`, () => {
  const row = (patch: Record<string, unknown> = {}) => [{ userId: `me`, ...stored, ...patch }]

  it(`stores every canonical value of shortcutParseCases and refuses every other text`, async () => {
    for (const c of fixture.shortcutParseCases) {
      h.state.selectQueue = [row()]
      h.state.upserts = []
      const write = caller.update({ pttShortcut: c.in })
      if (c.value === c.in) {
        expect(await write, c.in).toMatchObject({ pttShortcut: c.in, pushToTalk: c.in !== `` })
        expect(h.state.upserts[0]?.set, c.in).toMatchObject({ pttShortcut: c.in, pushToTalk: c.in !== `` })
      } else {
        await expect(write, c.in).rejects.toMatchObject({ code: `BAD_REQUEST` })
        expect(h.state.upserts, c.in).toHaveLength(0)
      }
    }
  })

  it(`an empty binding = push to talk off`, async () => {
    h.state.selectQueue = [row()]
    expect(await caller.update({ pttShortcut: `` })).toMatchObject({ pttShortcut: ``, pushToTalk: false })
  })

  it(`an old client's pushToTalk false clears the key; true without one binds ctrl+space`, async () => {
    h.state.selectQueue = [row()]
    expect(await caller.update({ pushToTalk: false })).toMatchObject({ pttShortcut: ``, pushToTalk: false })
    h.state.selectQueue = [row({ pttShortcut: `` })]
    expect(await caller.update({ pushToTalk: true })).toMatchObject({ pttShortcut: `ctrl+space`, pushToTalk: true })
    h.state.selectQueue = [row({ pttShortcut: `f13` })]
    expect(await caller.update({ pushToTalk: true })).toMatchObject({ pttShortcut: `f13`, pushToTalk: true })
  })

  it(`an old client's speakReplies is ignored (speak-back is always on)`, async () => {
    h.state.selectQueue = [row()]
    expect(await caller.update({ speakReplies: false })).toMatchObject({ speakReplies: true })
    expect(h.state.upserts[0]?.set).toMatchObject({ speakReplies: true })
  })
})

describe(`#NEW-71`, () => {
  it(`a deleted agent (agent_id nulled by the FK) reads with the wake word off`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, agentId: null, wakeWord: true }]]
    expect(await caller.get()).toMatchObject({ agentId: null, wakeWord: false })
  })

  it(`picking a new agent after that never turns the wake word back on`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, agentId: null, wakeWord: true }], [{ id: AGENT }]]
    const next = await caller.update({ agentId: AGENT })
    expect(next).toMatchObject({ agentId: AGENT, wakeWord: false })
  })

  it(`refuses a wake phrase with a newline or a control character`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }], [{ userId: `me`, ...stored }]]
    await expect(caller.update({ wakePhrase: `Hey\nOps` })).rejects.toThrow()
    await expect(caller.update({ wakePhrase: `Hey\u0007Ops` })).rejects.toThrow()
    expect(h.state.upserts).toHaveLength(0)
  })
})

describe(`#NEW-56 wake phrase length`, () => {
  it(`refuses a phrase under the fixture's letters with its text; empty stays valid`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }], [{ userId: `me`, ...stored }]]
    for (const c of fixture.wakePhraseErrorCases.filter((c) => c.error !== null)) {
      await expect(caller.update({ wakePhrase: c.in })).rejects.toMatchObject({ code: `BAD_REQUEST`, message: c.error })
    }
    expect(h.state.upserts).toHaveLength(0)
    expect(await caller.update({ wakePhrase: `` })).toMatchObject({ wakePhrase: `` })
    expect(await caller.update({ wakePhrase: `Jürgen` })).toMatchObject({ wakePhrase: `Jürgen` })
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

// NEW-57: meeting mode (fixtures/meeting.json).
describe(`#NEW-57 meeting mode`, () => {
  // A minute ago (whole seconds): never old enough for autoStop.
  const STARTED = new Date(Math.floor(Date.now() / 1000) * 1000 - 60_000)

  it(`AUTO-STOP: a meeting autoStop.afterMs old reads as none`, async () => {
    const old = new Date(Date.now() - meetingFixture.autoStop.afterMs - 1)
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: old }]]
    expect((await caller.get()).meetingStartedAt).toBeNull()
  })

  it(`the apps scope is a set: reordering the same apps keeps the meeting`, () => {
    const zoom = { id: `z`, name: `Zoom` }
    const slack = { id: `s`, name: `Slack` }
    const before = { ...stored, meetingStartedAt: `x`, desktopAudio: `include`, desktopAudioApps: [zoom, slack] } as never
    const next = { ...stored, meetingStartedAt: `x`, desktopAudio: `include`, desktopAudioApps: [slack, zoom] } as never
    expect(withMeetingScope(before, next).meetingStartedAt).toBe(`x`)
  })

  it(`reads the stored desktop audio, apps and the running meeting`, async () => {
    const apps = [{ id: `us.zoom.xos`, name: `zoom.us` }]
    h.state.selectQueue = [[{ userId: `me`, ...stored, desktopAudio: `include`, desktopAudioApps: apps, meetingStartedAt: STARTED }]]
    expect(await caller.get()).toMatchObject({ desktopAudio: `include`, desktopAudioApps: apps, meetingStartedAt: STARTED.toISOString() })
  })

  it(`update stores desktop audio and de-duplicated, trimmed apps, never the meeting`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    const next = await caller.update({
      desktopAudio: `include`,
      desktopAudioApps: [
        { id: `z`, name: ` Zoom ` },
        { id: `z`, name: `Zoom again` },
        { id: `s`, name: `Slack` },
      ],
      meetingStartedAt: STARTED.toISOString(),
    } as never)
    expect(next).toMatchObject({
      desktopAudio: `include`,
      desktopAudioApps: [{ id: `z`, name: `Zoom` }, { id: `s`, name: `Slack` }],
      meetingStartedAt: null,
    })
    expect(h.state.upserts[0]?.set).toMatchObject({ desktopAudio: `include`, meetingStartedAt: null })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
  })

  it(`update refuses an unknown value, too many apps and a long name`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }], [{ userId: `me`, ...stored }], [{ userId: `me`, ...stored }]]
    await expect(caller.update({ desktopAudio: `mix` as never })).rejects.toThrow()
    const many = Array.from({ length: 33 }, (_, i) => ({ id: `a${i}`, name: `App ${i}` }))
    await expect(caller.update({ desktopAudioApps: many })).rejects.toThrow()
    await expect(caller.update({ desktopAudioApps: [{ id: `x`, name: `x`.repeat(65) }] })).rejects.toThrow()
    expect(h.state.upserts).toHaveLength(0)
  })

  it(`startMeeting needs consent: true`, async () => {
    await expect(caller.startMeeting({} as never)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    await expect(caller.startMeeting({ consent: false } as never)).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(h.state.upserts).toHaveLength(0)
  })

  it(`startMeeting needs a voice device and an agent`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, agentId: null }]]
    await expect(caller.startMeeting({ consent: true })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    h.state.selectQueue = [[]]
    await expect(caller.startMeeting({ consent: true })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(h.state.upserts).toHaveLength(0)
  })

  it(`startMeeting stamps now and nudges the voice device; a running one keeps its start`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    const next = await caller.startMeeting({ consent: true })
    expect(next.meetingStartedAt).not.toBeNull()
    const set = h.state.upserts[0]?.set as { meetingStartedAt: Date }
    expect(set.meetingStartedAt).toBeInstanceOf(Date)
    expect(Math.abs(set.meetingStartedAt.getTime() - Date.now())).toBeLessThan(5000)
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)

    h.state.upserts = []
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
    expect((await caller.startMeeting({ consent: true })).meetingStartedAt).toBe(STARTED.toISOString())
    expect(h.state.upserts).toHaveLength(0)
  })

  it(`stopMeeting clears it and nudges the voice device`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
    expect((await caller.stopMeeting()).meetingStartedAt).toBeNull()
    expect(h.state.upserts[0]?.set).toMatchObject({ meetingStartedAt: null })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
  })

  it(`choosing None for the device or the agent stops the meeting (clearOnNone grows meeting)`, async () => {
    for (const change of [{ voiceDeviceId: null }, { agentId: null }]) {
      h.state.upserts = []
      h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
      const next = await caller.update(change)
      expect(next.meetingStartedAt).toBeNull()
      expect(h.state.upserts[0]?.set).toMatchObject({ meetingStartedAt: null })
    }
  })

  it(`widening the scope mid-meeting (device, agent, desktop audio, apps) stops it`, async () => {
    const OTHER_AGENT = `4f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d`
    const changes: [Record<string, unknown>, unknown[][]][] = [
      [{ voiceDeviceId: `d-other` }, [[{ kind: `desktop` }]]],
      [{ agentId: OTHER_AGENT }, [[{ id: OTHER_AGENT }]]],
      [{ desktopAudio: `all` }, []],
      [{ desktopAudioApps: [{ id: `z`, name: `Zoom` }] }, []],
    ]
    for (const [change, probes] of changes) {
      h.state.upserts = []
      h.relayPostNudge.mockClear()
      h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }], ...probes]
      const next = await caller.update(change as never)
      expect(next.meetingStartedAt, JSON.stringify(change)).toBeNull()
      expect(h.state.upserts[0]?.set).toMatchObject({ meetingStartedAt: null })
      expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
    }
    // The device side follows: the old and the new device are both nudged.
    expect(withMeetingScope({ ...stored, meetingStartedAt: `x` } as never, { ...stored, meetingStartedAt: `x`, voiceDeviceId: `d-other` } as never).meetingStartedAt).toBeNull()
  })

  it(`a same-value update or an unrelated switch keeps the meeting`, async () => {
    for (const change of [{ desktopAudio: `off` }, { wakeWord: true }, { language: `de` }, { voiceDeviceId: `d-mint` }]) {
      h.state.upserts = []
      const probes = `voiceDeviceId` in change ? [[{ kind: `desktop` }]] : []
      h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }], ...probes]
      const next = await caller.update(change as never)
      expect(next.meetingStartedAt, JSON.stringify(change)).toBe(STARTED.toISOString())
    }
  })

  it(`startMeeting refuses an agent on a shared or foreign device (copy.sharedRefused)`, async () => {
    h.meetingAgentIsPrivate.mockResolvedValue(false)
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    await expect(caller.startMeeting({ consent: true })).rejects.toMatchObject({
      code: `BAD_REQUEST`,
      message: meetingFixture.copy.sharedRefused,
    })
    expect(h.meetingAgentIsPrivate).toHaveBeenCalledWith(h.db, `me`, AGENT)
    expect(h.state.upserts).toHaveLength(0)
    expect(h.relayPostNudge).not.toHaveBeenCalled()
  })

  it(`a flush message fits the server's sessions.send text limit`, () => {
    expect(meetingFixture.flush.maxMessageChars).toBe(SESSIONS_SEND_TEXT_MAX)
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

  it(`S-3: Speak replies off never refuses, and a downloading voice rings`, async () => {
    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored, speakReplies: false }], [deviceRow()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).resolves.toMatchObject({ relayUrl: `https://relay.example` })
    h.state.selectQueue = [[{ id: AGENT }], [{ userId: `me`, ...stored }], [deviceRow({ doctor: doctor(`action`) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).resolves.toMatchObject({ relayUrl: `https://relay.example` })
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
  it(`mints a call_host ticket for the member's voice device`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, createdAt: new Date(), updatedAt: new Date() }]]
    const callId = `7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d`
    const out = await caller.callHostTicket({ callId })
    expect(out.url).toBe(`wss://relay.example/ws?ticket=${encodeURIComponent(out.ticket)}`)
    const verdict = verifySteerTicket(out.ticket, `relay-secret`)
    expect(verdict.ok && verdict.claims).toMatchObject({ role: `call_host`, sub: `me`, callId, deviceId: `d-mint` })
  })

  it(`refuses a member without a voice device`, async () => {
    await expect(caller.callHostTicket({ callId: `7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d` })).rejects.toThrow(`No voice device`)
  })
})
