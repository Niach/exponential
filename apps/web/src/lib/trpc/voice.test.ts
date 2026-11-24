import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import fixture from "@exp/etalk/fixtures/voice.json" with { type: "json" }

const h = vi.hoisted(() => {
  const state = {
    // One entry per select, in call order: the stored row, then the device
    // probe, then the agent probe (as the mutation reaches them).
    selectQueue: [] as unknown[][],
    upserts: [] as { values: Record<string, unknown>; set: Record<string, unknown> }[],
    // S-3: the member's Assistants (oldest first) and the voice device each
    // one yields (null = not one of their own desktops).
    assistants: [] as { id: string; voiceDeviceId: string | null }[],
    // FEED-163: one entry per update…returning (the conditional meeting clear).
    updateQueue: [] as unknown[][],
    updates: [] as Record<string, unknown>[],
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
    update: () => ({
      set: (set: Record<string, unknown>) => ({
        where: () => ({
          returning: () => {
            state.updates.push(set)
            return Promise.resolve(state.updateQueue.shift() ?? [])
          },
        }),
      }),
    }),
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
    // S-3: lib/agents/assistant.ts `voiceAssistantOf`, over `state.assistants`
    // (the stored agent while it is one of them, else the oldest).
    voiceAssistantOf: vi.fn(async (_exec: unknown, _userId: string, storedAgentId: string | null) => {
      const assistant =
        state.assistants.find((row) => row.id === storedAgentId) ?? state.assistants[0]
      return assistant
        ? { agentId: assistant.id, voiceDeviceId: assistant.voiceDeviceId }
        : { agentId: null, voiceDeviceId: null }
    }),
    postMeetingStopNote: vi.fn(async (..._args: unknown[]) => `note-1` as string | null),
  }
})

vi.mock(`@/db/connection`, () => ({ db: {} }))
vi.mock(`@/lib/auth`, () => ({ auth: {} }))
vi.mock(`@/lib/sessions/meeting-access`, async () => ({
  MEETING_SHARED_REFUSED: (await import(`@exp/etalk/meeting`)).MEETING_COPY.sharedRefused,
  meetingAgentIsPrivate: h.meetingAgentIsPrivate,
  postMeetingStopNote: h.postMeetingStopNote,
}))
vi.mock(`@/lib/agents/assistant`, () => ({ voiceAssistantOf: h.voiceAssistantOf }))
vi.mock(`@/lib/steer`, async () => ({
  ...(await vi.importActual<typeof import("@/lib/steer")>(`@/lib/steer`)),
  getSteerRelayConfig: () => ({ url: `https://relay.example`, secret: `relay-secret` }),
  relayPostNudge: h.relayPostNudge,
}))

import callFixture from "@exp/etalk/fixtures/voice-call.json" with { type: "json" }
import meetingFixture from "@exp/etalk/fixtures/meeting.json" with { type: "json" }
import { SESSIONS_SEND_TEXT_MAX } from "@exp/db-schema/domain"
import { verifySteerTicket } from "@exp/steer-ticket"
import { callSpeechOf, voiceRouter, withClearOnNone, withMeetingScope } from "@/lib/trpc/voice"
import { mintCallId, verifyCallId } from "@/lib/voice-call-id"

// #EXP-1941: a call id is a MAC under this secret; without it no call starts.
process.env.BETTER_AUTH_SECRET = `unit-test-secret`

const caller = voiceRouter.createCaller({
  session: { user: { id: `me`, name: `Me`, email: `me@example.com` } },
  db: h.db,
  request: new Request(`http://localhost/`),
} as never)

const AGENT = `3f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d`
const stored = {
  voiceDeviceId: `d-mint`,
  agentId: AGENT,
  pttShortcut: `ctrl+space`,
  wakeWord: false,
  wakePhrase: ``,
  language: `en`,
  voiceFile: ``,
  voiceTranscript: ``,
  desktopAudio: `off`,
  desktopAudioApps: [] as { id: string; name: string }[],
  meetingStartedAt: null as string | null,
  meetingAutoStopHours: 4,
}
const MEETING_DEFAULTS = { desktopAudio: `off`, desktopAudioApps: [], meetingStartedAt: null, meetingAutoStopHours: 4 }

const OTHER_AGENT = `4f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d`
const FOREIGN_AGENT = `5f2b1c4d-5e6f-4a7b-8c9d-0e1f2a3b4c5d`

beforeEach(() => {
  h.state.selectQueue = []
  h.state.upserts = []
  // The member's one Assistant, on their desktop `d-mint`.
  h.state.assistants = [{ id: AGENT, voiceDeviceId: `d-mint` }]
  h.state.updateQueue = []
  h.state.updates = []
  h.relayPostNudge.mockClear()
  h.postMeetingStopNote.mockClear()
  h.meetingAgentIsPrivate.mockReset()
  h.meetingAgentIsPrivate.mockResolvedValue(true)
})

describe(`voice.get`, () => {
  it(`a member without a row or an Assistant gets the fixture's defaults`, async () => {
    h.state.assistants = []
    expect(await caller.get()).toEqual({ ...fixture.defaults, ...MEETING_DEFAULTS })
  })

  // S-3: the voice device and agent are DERIVED from the member's Assistant.
  it(`a member without a row reads their Assistant and its desktop, wake word off`, async () => {
    expect(await caller.get()).toEqual({
      ...fixture.defaults,
      ...MEETING_DEFAULTS,
      voiceDeviceId: `d-mint`,
      agentId: AGENT,
    })
  })

  it(`the derived Assistant wins over the stored mirror`, async () => {
    h.state.assistants = [{ id: OTHER_AGENT, voiceDeviceId: `d-other` }]
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    expect(await caller.get()).toMatchObject({ voiceDeviceId: `d-other`, agentId: OTHER_AGENT })
    expect(h.voiceAssistantOf).toHaveBeenLastCalledWith(h.db, `me`, AGENT)
  })

  it(`an Assistant off the member's desktops has no voice device: wake word off, the key stays`, async () => {
    h.state.assistants = [{ id: AGENT, voiceDeviceId: null }]
    h.state.selectQueue = [[{ userId: `me`, ...stored, wakeWord: true }]]
    expect(await caller.get()).toMatchObject({
      voiceDeviceId: null,
      agentId: AGENT,
      wakeWord: false,
    })
  })

  it(`returns the stored row`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, createdAt: new Date(), updatedAt: new Date() }]]
    expect(await caller.get()).toEqual(stored)
  })
})

describe(`voice.update`, () => {
  it(`stores the binding with the Assistant's mirror, and nudges its device`, async () => {
    h.state.selectQueue = [[]]
    const next = await caller.update({ agentId: AGENT, pttShortcut: `ctrl+space` })
    expect(next).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT, pttShortcut: `ctrl+space` })
    expect(h.state.upserts[0]?.set).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT, pttShortcut: `ctrl+space` })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
  })

  // S-3: the device is the Assistant's (agents.update moves it).
  it(`strips an old client's voiceDeviceId (the input stays non-strict)`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    const next = await caller.update({ voiceDeviceId: `d-other`, wakeWord: true } as never)
    expect(next).toMatchObject({ voiceDeviceId: `d-mint`, agentId: AGENT, wakeWord: true })
    expect(h.state.upserts[0]?.set).toMatchObject({ voiceDeviceId: `d-mint` })
    expect(h.relayPostNudge).not.toHaveBeenCalledWith(expect.anything(), `me`, `d-other`)
  })

  it(`ignores None for the device or the agent: the Assistant keeps the voice`, async () => {
    for (const change of [{ voiceDeviceId: null } as never, { agentId: null }]) {
      h.state.selectQueue = [[{ userId: `me`, ...stored }]]
      expect(await caller.update(change)).toMatchObject({
        voiceDeviceId: `d-mint`,
        agentId: AGENT,
      })
    }
  })

  it(`moves the voice onto another of the member's Assistants`, async () => {
    h.state.assistants = [
      { id: AGENT, voiceDeviceId: `d-mint` },
      { id: OTHER_AGENT, voiceDeviceId: `d-other` },
    ]
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    const next = await caller.update({ agentId: OTHER_AGENT })
    expect(next).toMatchObject({ voiceDeviceId: `d-other`, agentId: OTHER_AGENT })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-other`)
  })

  it(`refuses an agentId that is not one of the member's Assistants`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }]]
    await expect(caller.update({ agentId: FOREIGN_AGENT })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    h.state.assistants = []
    h.state.selectQueue = [[]]
    await expect(caller.update({ agentId: AGENT })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    expect(h.state.upserts).toHaveLength(0)
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
        expect(await write, c.in).toMatchObject({ pttShortcut: c.in })
        expect(h.state.upserts[0]?.set, c.in).toMatchObject({ pttShortcut: c.in })
      } else {
        await expect(write, c.in).rejects.toMatchObject({ code: `BAD_REQUEST` })
        expect(h.state.upserts, c.in).toHaveLength(0)
      }
    }
  })

  it(`an empty binding = push to talk off`, async () => {
    h.state.selectQueue = [row()]
    expect(await caller.update({ pttShortcut: `` })).toMatchObject({ pttShortcut: `` })
  })
})

describe(`#NEW-71`, () => {
  it(`a member whose Assistant is gone reads with the wake word off (S-3: the key stays bound)`, async () => {
    h.state.assistants = []
    h.state.selectQueue = [[{ userId: `me`, ...stored, agentId: null, wakeWord: true }]]
    expect(await caller.get()).toMatchObject({
      agentId: null,
      pttShortcut: `ctrl+space`,
      wakeWord: false,
    })
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
    expect(h.state.upserts[0]?.set).toMatchObject({ desktopAudio: `include` })
    expect(h.state.upserts[0]?.set).not.toHaveProperty(`meetingStartedAt`)
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
    h.state.assistants = []
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

  it(`FEED-163: startMeeting over an EXPIRED stored stamp clears it with the autoStop note first`, async () => {
    const old = new Date(Date.now() - meetingFixture.autoStop.afterMs - 1)
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: old }], [{ meetingStartedAt: old }]]
    h.state.updateQueue = [[{ agentId: AGENT, voiceDeviceId: `d-mint`, hours: 4 }]]
    const next = await caller.startMeeting({ consent: true })
    expect(h.state.updates[0]).toMatchObject({ meetingStartedAt: null })
    expect(h.postMeetingStopNote).toHaveBeenCalledTimes(1)
    expect(h.postMeetingStopNote).toHaveBeenCalledWith(h.db, `me`, AGENT, `autoStop`, 4)
    // The new meeting is stamped now, after the old one's note.
    expect(next.meetingStartedAt).not.toBeNull()
    const set = h.state.upserts[0]?.set as { meetingStartedAt: Date }
    expect(Math.abs(set.meetingStartedAt.getTime() - Date.now())).toBeLessThan(5000)
  })

  it(`startMeeting without a stored stamp posts no note`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored }], []]
    await caller.startMeeting({ consent: true })
    expect(h.state.updates).toHaveLength(0)
    expect(h.postMeetingStopNote).not.toHaveBeenCalled()
  })

  it(`stopMeeting takes a null input (the desktop's body for no reason)`, async () => {
    h.state.selectQueue = [[{ meetingStartedAt: STARTED }], [{ userId: `me`, ...stored }]]
    h.state.updateQueue = [[{ agentId: AGENT, voiceDeviceId: `d-mint`, hours: 4 }]]
    expect((await caller.stopMeeting(null)).meetingStartedAt).toBeNull()
    expect(h.state.updates[0]).toMatchObject({ meetingStartedAt: null })
    expect(h.postMeetingStopNote).not.toHaveBeenCalled()
  })

  it(`stopMeeting clears it and nudges the voice device; a manual stop posts no note`, async () => {
    h.state.selectQueue = [[{ meetingStartedAt: STARTED }], [{ userId: `me`, ...stored }]]
    h.state.updateQueue = [[{ agentId: AGENT, voiceDeviceId: `d-mint`, hours: 4 }]]
    expect((await caller.stopMeeting()).meetingStartedAt).toBeNull()
    expect(h.state.updates[0]).toMatchObject({ meetingStartedAt: null })
    expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
    expect(h.postMeetingStopNote).not.toHaveBeenCalled()
  })

  it(`FEED-163: stopMeeting {reason} posts ONE note at the member's hours, only when it cleared the stamp`, async () => {
    for (const reason of [`autoStop`, `restart`] as const) {
      h.postMeetingStopNote.mockClear()
      h.state.selectQueue = [[{ meetingStartedAt: STARTED }], [{ userId: `me`, ...stored }]]
      h.state.updateQueue = [[{ agentId: AGENT, voiceDeviceId: `d-mint`, hours: 2 }]]
      await caller.stopMeeting({ reason })
      expect(h.postMeetingStopNote).toHaveBeenCalledTimes(1)
      expect(h.postMeetingStopNote).toHaveBeenCalledWith(h.db, `me`, AGENT, reason, 2)
    }
    // The watchdog (or another call) cleared it first: nothing to post.
    h.postMeetingStopNote.mockClear()
    h.state.selectQueue = [[{ meetingStartedAt: STARTED }], [{ userId: `me`, ...stored }]]
    h.state.updateQueue = [[]]
    await caller.stopMeeting({ reason: `autoStop` })
    expect(h.postMeetingStopNote).not.toHaveBeenCalled()
    // No stored meeting at all: no clear, no note.
    h.state.selectQueue = [[], [{ userId: `me`, ...stored }]]
    await caller.stopMeeting({ reason: `autoStop` })
    expect(h.state.updates).toHaveLength(3)
    expect(h.postMeetingStopNote).not.toHaveBeenCalled()
    await expect(caller.stopMeeting({ reason: `sharing` } as never)).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`FEED-163: the member's hours decide the read guard; a value outside hoursValues = the default`, async () => {
    const hourOld = new Date(Date.now() - 60 * 60 * 1000 - 1)
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingAutoStopHours: 1, meetingStartedAt: hourOld }]]
    expect(await caller.get()).toMatchObject({ meetingStartedAt: null, meetingAutoStopHours: 1 })
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingAutoStopHours: 8, meetingStartedAt: hourOld }]]
    expect((await caller.get()).meetingStartedAt).toBe(hourOld.toISOString())
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingAutoStopHours: 5 }]]
    expect((await caller.get()).meetingAutoStopHours).toBe(meetingFixture.autoStop.defaultHours)
  })

  it(`FEED-163: update stores meetingAutoStopHours (one of hoursValues), the meeting keeps running`, async () => {
    h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
    const next = await caller.update({ meetingAutoStopHours: 8 })
    expect(next).toMatchObject({ meetingAutoStopHours: 8, meetingStartedAt: STARTED.toISOString() })
    expect(h.state.upserts[0]?.set).toMatchObject({ meetingAutoStopHours: 8 })
    // Never rewrites the stamp it did not change (an expired one is the clear's).
    expect(h.state.upserts[0]?.set).not.toHaveProperty(`meetingStartedAt`)
    for (const bad of [0, 5, 2.5, 24]) {
      h.state.selectQueue = [[{ userId: `me`, ...stored }]]
      await expect(caller.update({ meetingAutoStopHours: bad })).rejects.toMatchObject({ code: `BAD_REQUEST` })
    }
  })

  it(`no Assistant (no device, no agent) reads with the meeting stopped (clearOnNone grows meeting)`, async () => {
    for (const assistants of [[], [{ id: AGENT, voiceDeviceId: null }]]) {
      h.state.assistants = assistants
      h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
      expect((await caller.get()).meetingStartedAt).toBeNull()
    }
  })

  it(`widening the scope mid-meeting (the voice Assistant, desktop audio, apps) stops it`, async () => {
    h.state.assistants = [
      { id: AGENT, voiceDeviceId: `d-mint` },
      { id: OTHER_AGENT, voiceDeviceId: `d-other` },
    ]
    const changes: Record<string, unknown>[] = [
      { agentId: OTHER_AGENT },
      { desktopAudio: `all` },
      { desktopAudioApps: [{ id: `z`, name: `Zoom` }] },
    ]
    for (const change of changes) {
      h.state.upserts = []
      h.relayPostNudge.mockClear()
      h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
      const next = await caller.update(change as never)
      expect(next.meetingStartedAt, JSON.stringify(change)).toBeNull()
      expect(h.state.upserts[0]?.set).toMatchObject({ meetingStartedAt: null })
      expect(h.relayPostNudge).toHaveBeenCalledWith(expect.anything(), `me`, `d-mint`)
    }
    // The device side follows: the old and the new device are both nudged.
    expect(withMeetingScope({ ...stored, meetingStartedAt: `x` } as never, { ...stored, meetingStartedAt: `x`, voiceDeviceId: `d-other` } as never).meetingStartedAt).toBeNull()
  })

  it(`a same-value update, an unrelated switch or an ignored device keeps the meeting`, async () => {
    for (const change of [{ desktopAudio: `off` }, { wakeWord: true }, { language: `de` }, { voiceDeviceId: `d-other` }, { agentId: AGENT }]) {
      h.state.upserts = []
      h.state.selectQueue = [[{ userId: `me`, ...stored, meetingStartedAt: STARTED }]]
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
// EXP-1857: the call rings the agent's device, else falls back (`callTarget`).
describe(`voice.startCall`, () => {
  const doctor = (state: string | null) => ({
    checkedAt: new Date().toISOString(),
    items: state === null ? [] : [{ key: `speech`, group: `voice`, state }],
  })
  const desktop = (deviceId: string, label: string, patch: Record<string, unknown> = {}) => ({
    deviceId,
    kind: `desktop`,
    label,
    lastSeenAt: new Date(),
    doctor: doctor(`ok`),
    ...patch,
  })
  const MINT = (patch: Record<string, unknown> = {}) => desktop(`d-mint`, `mint`, patch)
  // EXP-1858: one of the member's own servers (a CLI daemon).
  const SERVER = (patch: Record<string, unknown> = {}) => desktop(`d-server`, `server`, { kind: `server`, ...patch })
  const MAC = (patch: Record<string, unknown> = {}) =>
    desktop(`d-mac`, `macbook`, { lastSeenAt: new Date(Date.now() - 1000), ...patch })
  const agentOn = (deviceId: string) => [{ id: AGENT, deviceId }]
  const reasons = callFixture.copy.reasons
  const refused = (message: string) => ({ code: `PRECONDITION_FAILED`, message })
  const claimsOf = (ticket: string) => {
    const verdict = verifySteerTicket(ticket, `relay-secret`)
    expect(verdict.ok).toBe(true)
    return verdict.ok ? verdict.claims : null
  }

  it(`mints a caller ticket ringing the agent's own desktop`, async () => {
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MAC(), MINT()]]
    const out = await caller.startCall({ agentId: AGENT, platform: `phone` })
    expect(out.relayUrl).toBe(`https://relay.example`)
    expect(out.url).toBe(`wss://relay.example/ws?ticket=${encodeURIComponent(out.ticket)}`)
    expect(out).toMatchObject({ deviceId: `d-mint`, device: `mint`, fallback: false })
    expect(claimsOf(out.ticket)).toMatchObject({
      role: `caller`,
      sub: `me`,
      callId: out.callId,
      agentId: AGENT,
      deviceId: `d-mint`,
      deviceOwnerId: `me`,
    })
  })

  it(`an agent on a server falls back to the newest desktop that speaks`, async () => {
    // Danny's Bro on the N95 `server`: no voice device, still callable.
    h.state.assistants = [{ id: AGENT, voiceDeviceId: null }]
    h.state.selectQueue = [agentOn(`d-server`), [{ userId: `me`, ...stored }], [MAC(), MINT()]]
    const out = await caller.startCall({ agentId: AGENT, platform: `web` })
    expect(out).toMatchObject({ deviceId: `d-mint`, device: `mint`, fallback: true })
    expect(claimsOf(out.ticket)).toMatchObject({ role: `caller`, deviceId: `d-mint` })
  })

  it(`EXP-1858: an agent on the member's own server rings that server`, async () => {
    h.state.selectQueue = [agentOn(`d-server`), [{ userId: `me`, ...stored }], [MAC(), MINT(), SERVER()]]
    const out = await caller.startCall({ agentId: AGENT, platform: `web` })
    expect(out).toMatchObject({ deviceId: `d-server`, device: `server`, fallback: false })
    expect(claimsOf(out.ticket)).toMatchObject({ role: `caller`, deviceId: `d-server`, deviceOwnerId: `me` })
  })

  it(`EXP-1858: a server without a GPU falls back; alone it names its reason; never another agent's fallback`, async () => {
    h.state.selectQueue = [agentOn(`d-server`), [{ userId: `me`, ...stored }], [SERVER({ doctor: doctor(`missing`) }), MAC()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).resolves.toMatchObject({
      deviceId: `d-mac`,
      fallback: true,
    })
    h.state.selectQueue = [agentOn(`d-server`), [{ userId: `me`, ...stored }], [SERVER({ doctor: doctor(`missing`) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject(
      refused(`server can't speak: no usable GPU`)
    )
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ lastSeenAt: new Date(0) }), SERVER()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject(refused(`mint is offline`))
  })

  it(`the agent's desktop without a GPU falls back`, async () => {
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ doctor: doctor(`missing`) }), MAC()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).resolves.toMatchObject({
      deviceId: `d-mac`,
      fallback: true,
    })
  })

  it(`the desktop IDE calls another desktop; its own target is refused`, async () => {
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MAC(), MINT()]]
    await expect(
      caller.startCall({ agentId: AGENT, platform: `desktop`, deviceId: `d-mac` })
    ).resolves.toMatchObject({ deviceId: `d-mint`, fallback: false })
    h.state.selectQueue = [agentOn(`d-server`), [{ userId: `me`, ...stored }], [MAC(), MINT()]]
    await expect(
      caller.startCall({ agentId: AGENT, platform: `desktop`, deviceId: `d-mint` })
    ).rejects.toMatchObject({ code: `BAD_REQUEST` })
  })

  it(`refuses an agent outside the member's teams`, async () => {
    h.state.selectQueue = [[]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject({ code: `NOT_FOUND` })
  })

  it(`S-3: refuses another member's Assistant, takes the caller's own`, async () => {
    h.state.selectQueue = [[{ role: `assistant`, ownerId: `someone-else`, deviceId: `d-mint` }]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject({ code: `NOT_FOUND` })
    h.state.selectQueue = [
      [{ role: `assistant`, ownerId: `me`, deviceId: `d-mint` }],
      [{ userId: `me`, ...stored }],
      [MINT()],
    ]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).resolves.toMatchObject({
      relayUrl: `https://relay.example`,
    })
  })

  it(`refuses with the first candidate's reason when nothing speaks`, async () => {
    h.state.selectQueue = [agentOn(`d-server`), [{ userId: `me`, ...stored }], []]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(refused(reasons.noDesktop))

    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ lastSeenAt: new Date(0) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(refused(`mint is offline`))

    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ doctor: null })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(
      refused(`mint is getting its voice ready`)
    )

    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ doctor: doctor(`missing`) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject(
      refused(`mint can't speak: no usable GPU`)
    )

    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ doctor: doctor(`error`) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `web` })).rejects.toMatchObject(
      refused(`mint couldn't load its voice: restart Exponential there`)
    )
  })

  it(`#NEW-57: a meeting on the voice device falls back, alone it refuses`, async () => {
    const meeting = { ...stored, meetingStartedAt: new Date().toISOString() }
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...meeting }], [MINT(), MAC()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).resolves.toMatchObject({
      deviceId: `d-mac`,
      fallback: true,
    })
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...meeting }], [MINT()]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).rejects.toMatchObject(
      refused(`mint is transcribing a meeting`)
    )
  })

  it(`S-3: a downloading voice rings`, async () => {
    h.state.selectQueue = [agentOn(`d-mint`), [{ userId: `me`, ...stored }], [MINT({ doctor: doctor(`action`) })]]
    await expect(caller.startCall({ agentId: AGENT, platform: `phone` })).resolves.toMatchObject({ deviceId: `d-mint` })
  })

  it(`reads the speech doctor row`, () => {
    expect(callSpeechOf(null)).toBeNull()
    expect(callSpeechOf(doctor(null))).toBeNull()
    expect(callSpeechOf(doctor(`ok`))).toBe(`ok`)
    expect(callSpeechOf(doctor(`missing`))).toBe(`noGpu`)
    expect(callSpeechOf(doctor(`action`))).toBe(`preparing`)
    expect(callSpeechOf(doctor(`error`))).toBe(`failed`)
    // A `speech` key in another group is not the voice's.
    expect(callSpeechOf({ checkedAt: ``, items: [{ key: `speech`, group: `required`, state: `ok` }] })).toBeNull()
  })
})

describe(`voice.callHostTicket`, () => {
  const storedRow = [{ userId: `me`, ...stored, createdAt: new Date(), updatedAt: new Date() }]
  const mint = (patch: { userId?: string; deviceId?: string; agentId?: string; sealed?: boolean } = {}, now = new Date()) =>
    mintCallId({ userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: false, ...patch }, now)!
  const claimsOf = (ticket: string) => {
    const verdict = verifySteerTicket(ticket, `relay-secret`)
    return verdict.ok ? verdict.claims : null
  }
  const refusedCall = { code: `NOT_FOUND`, message: `No such call` }
  /** The member's callable agents, as `memberAgentIds` reads them. */
  const agentRows = [
    { id: OTHER_AGENT, role: `agent`, ownerId: null },
    { id: AGENT, role: `assistant`, ownerId: `me` },
    { id: FOREIGN_AGENT, role: `assistant`, ownerId: `mate` },
  ]

  it(`an old IDE (no device, no agent): the voice device, the clear id of any agent the member may call`, async () => {
    const callId = mint()
    h.state.selectQueue = [storedRow, agentRows]
    const out = await caller.callHostTicket({ callId })
    expect(out.url).toBe(`wss://relay.example/ws?ticket=${encodeURIComponent(out.ticket)}`)
    expect(out.seal).toBe(false)
    expect(claimsOf(out.ticket)).toMatchObject({ role: `call_host`, sub: `me`, callId, deviceId: `d-mint` })
    // Another member's Assistant is not one of them, whatever id names it.
    h.state.selectQueue = [storedRow, agentRows]
    await expect(caller.callHostTicket({ callId: mint({ agentId: FOREIGN_AGENT }) })).rejects.toMatchObject(refusedCall)
  })

  it(`EXP-1857: any of the member's own desktops answers for itself`, async () => {
    h.state.assistants = [{ id: AGENT, voiceDeviceId: null }]
    const callId = mint({ deviceId: `d-mac` })
    h.state.selectQueue = [[{ deviceId: `d-mac` }]]
    const out = await caller.callHostTicket({ callId, deviceId: `d-mac`, agentId: AGENT })
    expect(out.seal).toBe(false)
    expect(claimsOf(out.ticket)).toMatchObject({ role: `call_host`, sub: `me`, callId, deviceId: `d-mac` })
    // Someone else's device is not the member's to answer for.
    h.state.selectQueue = [[]]
    await expect(caller.callHostTicket({ callId, deviceId: `d-theirs`, agentId: AGENT })).rejects.toMatchObject({ code: `NOT_FOUND`, message: `That device isn't yours` })
  })

  it(`refuses a member without a voice device`, async () => {
    h.state.assistants = [{ id: AGENT, voiceDeviceId: null }]
    await expect(caller.callHostTicket({ callId: mint() })).rejects.toThrow(`No voice device`)
  })

  it(`#EXP-1941: refuses a ring the server did not mint, and logs who and where, never the id`, async () => {
    const warn = vi.spyOn(console, `warn`).mockImplementation(() => {})
    const own = [{ deviceId: `d-mint` }]
    const forged = `7a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d`
    const cases: [string, Parameters<typeof caller.callHostTicket>[0]][] = [
      [`an id of its own`, { callId: forged, deviceId: `d-mint`, agentId: AGENT }],
      [`minted for another device`, { callId: mint({ deviceId: `d-mac` }), deviceId: `d-mint`, agentId: AGENT }],
      [`minted for another member`, { callId: mint({ userId: `mate` }), deviceId: `d-mint`, agentId: AGENT }],
      [`older than its window`, { callId: mint({}, new Date(Date.now() - 4 * 60_000)), deviceId: `d-mint`, agentId: AGENT }],
      [`a clear id with another agent than the host names`, { callId: mint(), deviceId: `d-mint`, agentId: OTHER_AGENT }],
      [`a sealed id with another agent than the host names`, { callId: mint({ sealed: true }), deviceId: `d-mint`, agentId: OTHER_AGENT }],
    ]
    for (const [name, input] of cases) {
      h.state.selectQueue = [own]
      await expect(caller.callHostTicket(input), name).rejects.toMatchObject(refusedCall)
    }
    // A sealed id from a host that names no agent, or no device (the voice device).
    h.state.selectQueue = [own, agentRows]
    await expect(caller.callHostTicket({ callId: mint({ sealed: true }), deviceId: `d-mint` })).rejects.toMatchObject(refusedCall)
    h.state.selectQueue = [storedRow]
    await expect(caller.callHostTicket({ callId: mint({ sealed: true }), agentId: AGENT })).rejects.toMatchObject(refusedCall)
    const lines = warn.mock.calls.map((call) => String(call[0]))
    expect(lines).toHaveLength(8)
    expect(lines[0]).toBe(`[voice] callHostTicket refused: member=me device=d-mint tried sealed and clear for the named agent`)
    expect(lines[6]).toBe(`[voice] callHostTicket refused: member=me device=d-mint tried clear for 2 agents (the host named none)`)
    expect(lines.join(`\n`)).not.toContain(forged)
    warn.mockRestore()
  })

  it(`#EXP-1941: accepts an id minted one minute ahead, and answers the mode the id was minted with`, async () => {
    h.state.selectQueue = [[{ deviceId: `d-mint` }]]
    const ahead = mint({}, new Date(Date.now() + 60_000))
    await expect(caller.callHostTicket({ callId: ahead, deviceId: `d-mint`, agentId: AGENT })).resolves.toMatchObject({ seal: false })
    h.state.selectQueue = [[{ deviceId: `d-mint` }]]
    const sealed = mint({ sealed: true })
    await expect(caller.callHostTicket({ callId: sealed, deviceId: `d-mint`, agentId: AGENT })).resolves.toMatchObject({ seal: true, callId: sealed })
  })
})

describe(`#EXP-1941 voice.startCall mints the id callHostTicket vouches for`, () => {
  const doctor = { checkedAt: new Date().toISOString(), items: [{ key: `speech`, group: `voice`, state: `ok` }] }
  const MINT = { deviceId: `d-mint`, kind: `desktop`, label: `mint`, lastSeenAt: new Date(), doctor }
  const start = (input: { seal?: boolean } = {}) => {
    h.state.selectQueue = [[{ id: AGENT, deviceId: `d-mint` }], [{ userId: `me`, ...stored }], [MINT]]
    return caller.startCall({ agentId: AGENT, platform: `phone`, ...input })
  }

  // The claiming caller's extra read: the target device's caps. An Android
  // phone unless the test says otherwise; the switch names web, desktop, android.
  const from = (client: string | null) =>
    voiceRouter.createCaller({
      session: { user: { id: `me`, name: `Me`, email: `me@example.com` } },
      db: h.db,
      request: new Request(`http://localhost/`, client ? { headers: { "x-client-version": client } } : {}),
    } as never)
  const queue = (caps: unknown) => {
    h.state.selectQueue = [[{ id: AGENT, deviceId: `d-mint` }], [{ userId: `me`, ...stored }], [MINT], caps === undefined ? [] : [{ caps }]]
  }
  const startClaiming = (caps: unknown) => {
    queue(caps)
    return from(`android/0.14.155`).startCall({ agentId: AGENT, platform: `phone`, seal: true })
  }
  beforeEach(() => {
    process.env.VOICE_CALL_SEAL = `web,desktop,android`
  })
  afterEach(() => {
    delete process.env.VOICE_CALL_SEAL
  })

  it(`the switch names the platforms that seal: an iPhone, a phone that names no platform and every platform while it is unset stay clear, and the caps are not read`, async () => {
    const cases: Array<[string | undefined, string | null, `phone` | `web` | `desktop`, boolean]> = [
      [`web,desktop,android`, `ios/0.14.143`, `phone`, false],
      [`web,desktop,android`, null, `phone`, false],
      [`web,desktop,android`, `desktop/0.14.176`, `phone`, false],
      [`web,desktop,android`, `android/0.14.155`, `phone`, true],
      [`web,desktop,android`, null, `web`, true],
      [`web,desktop,android`, `desktop/0.14.176`, `desktop`, true],
      [` Web , IOS,nonsense`, `ios/0.14.143`, `phone`, true],
      [` Web , IOS,nonsense`, `android/0.14.155`, `phone`, false],
      [undefined, `android/0.14.155`, `phone`, false],
      [undefined, null, `web`, false],
      [``, `desktop/0.14.176`, `desktop`, false],
      [`on`, null, `web`, false],
    ]
    for (const [env, client, platform, sealed] of cases) {
      if (env === undefined) delete process.env.VOICE_CALL_SEAL
      else process.env.VOICE_CALL_SEAL = env
      queue([`call-seal`])
      // A desktop caller names the desktop it is at; the agent's device is another.
      const out = await from(client).startCall({ agentId: AGENT, platform, seal: true, ...(platform === `desktop` ? { deviceId: `d-here` } : {}) })
      expect([env, client, platform, out.seal]).toEqual([env, client, platform, sealed])
      // Switched off = decided before the caps: the device row is not read.
      expect(h.state.selectQueue.length).toBe(sealed ? 0 : 1)
      expect(verifyCallId(out.callId, { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed })).toBe(true)
    }
  })

  const hostIsTold = async (out: { callId: string; seal: boolean }) => {
    h.state.selectQueue = [[{ deviceId: `d-mint` }]]
    await expect(caller.callHostTicket({ callId: out.callId, deviceId: `d-mint`, agentId: AGENT })).resolves.toMatchObject({ seal: out.seal })
  }

  it(`a claim AND the target's call-seal cap seal the call: the id vouches for the mode and the host is told the same`, async () => {
    const out = await startClaiming([`voice`, `call-seal`])
    expect(out.seal).toBe(true)
    expect(verifyCallId(out.callId, { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: true })).toBe(true)
    // The mode is inside the id's MAC: the same id never vouches for a clear call.
    expect(verifyCallId(out.callId, { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: false })).toBe(false)
    await hostIsTold(out)
  })

  it(`no claim (an older caller) is a clear call, whatever the host can do: the caps are not even read`, async () => {
    for (const input of [{}, { seal: false }]) {
      const out = await start(input)
      expect(out.seal).toBe(false)
      expect(h.state.selectQueue).toEqual([])
      expect(verifyCallId(out.callId, { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: false })).toBe(true)
      expect(verifyCallId(out.callId, { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: true })).toBe(false)
      await hostIsTold(out)
    }
  })

  it(`a claim to a host without the cap (an older host, no key holder, a caps column that is no list) is a clear call`, async () => {
    for (const caps of [[], [`voice`], null, `call-seal`, { "call-seal": true }, undefined]) {
      const out = await startClaiming(caps)
      expect(out.seal).toBe(false)
      expect(verifyCallId(out.callId, { userId: `me`, deviceId: `d-mint`, agentId: AGENT, sealed: false })).toBe(true)
      await hostIsTold(out)
    }
  })

  it(`a sealed id from startCall opens callConversation for its agent and device only`, async () => {
    const out = await startClaiming([`call-seal`])
    const seal = { installId: AGENT, publicKey: `A`.repeat(43), epoch: 1 as const, wraps: [{ installId: AGENT, wrapped: `A`.repeat(108) }] }
    // A relay that moves the id to another agent or device, and a clear id: refused before any read.
    const clear = await start()
    for (const input of [
      { agentId: OTHER_AGENT, callId: out.callId, deviceId: `d-mint` },
      { agentId: AGENT, callId: out.callId, deviceId: `d-other` },
      { agentId: AGENT, callId: clear.callId, deviceId: `d-mint` },
    ]) {
      h.state.selectQueue = []
      await expect(caller.callConversation({ ...input, seal })).rejects.toMatchObject({ code: `NOT_FOUND`, message: `No such call` })
    }
  })

  it(`counts every answer by claim, cap and mode, never an id`, async () => {
    const info = vi.spyOn(console, `info`).mockImplementation(() => {})
    try {
      const sealed = await startClaiming([`call-seal`])
      const clear = await startClaiming([])
      await start()
      const lines = info.mock.calls.map((call) => String(call[0])).filter((line) => line.startsWith(`[voice] startCall`))
      expect(lines).toEqual([
        `[voice] startCall: platform=android claim=true on=true cap=true sealed=true`,
        `[voice] startCall: platform=android claim=true on=true cap=false sealed=false`,
        `[voice] startCall: platform=phone claim=false on=false cap=false sealed=false`,
      ])
      expect(lines.join(`\n`)).not.toContain(sealed.callId)
      expect(lines.join(`\n`)).not.toContain(clear.callId)
    } finally {
      info.mockRestore()
    }
  })

  it(`without BETTER_AUTH_SECRET no call starts: an id never vouches for nothing`, async () => {
    const secret = process.env.BETTER_AUTH_SECRET
    delete process.env.BETTER_AUTH_SECRET
    try {
      await expect(start()).rejects.toMatchObject({ code: `PRECONDITION_FAILED`, message: `Calls need the steer relay` })
    } finally {
      process.env.BETTER_AUTH_SECRET = secret
    }
  })
})
