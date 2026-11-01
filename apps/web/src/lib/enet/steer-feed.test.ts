import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { generateTreeKey } from "@exp/domain-contract/sealed-content"
import {
  STEER_ENET_LOST_GRACE_MS,
  STEER_FALLBACK_STALL_MS,
  STEER_TUNNEL_MAX_DATA_BYTES,
  type SteerFrame,
} from "@exp/domain-contract/steer-enet"
import { createSteerSessionStore, REPLAY_QUIET_MS } from "@/lib/steer-session-store"
import { CLOSE_ENET_LOST } from "./steer-enet-viewer"
import { openSteerFeed, type EnetLeg, type FeedStats, type SteerFeed } from "./steer-feed"
import { openTunnelData, sealTunnelData, type TunnelDeps, type TunnelSocket, type TunnelTicket } from "./steer-tunnel"

vi.mock(`@/lib/trpc-client`, () => ({ trpc: {} }))

// EXP-1527 (steer-enet.json `fallback`): eNet first, the sealed relay tunnel
// when eNet stalls, has no feed or drops, back to eNet on its reset.

const SID = `session-1`
const tree = generateTreeKey()
const KEY = { epoch: 1, publicKey: tree.publicKey, keys: new Map([[1, tree.secret]]) }

const narration = (text: string, seq: number): SteerFrame => ({ t: `activity`, seq, event: { kind: `narration`, text } })
const replayOf = (...texts: string[]): SteerFrame[] => [
  { t: `activity_reset` },
  ...texts.map((text, i) => narration(text, i)),
  { t: `activity_synced`, firstSeq: 0, lastSeq: Math.max(0, texts.length - 1) },
]

/** A joined eNet feed: its replay goes out on the first `join`. */
class FakeEnet implements EnetLeg {
  readyState = 1
  onmessage: ((event: { data: string }) => void) | null = null
  onclose: ((event: { code: number }) => void) | null = null
  sent: SteerFrame[] = []
  closed = false
  constructor(private replay: SteerFrame[] | null) {}
  send(data: string) {
    const frame = JSON.parse(data) as SteerFrame
    if (frame.t === `join` && this.replay) {
      const replay = this.replay
      this.replay = null
      void Promise.resolve().then(() => replay.forEach((f) => this.frame(f)))
      return
    }
    this.sent.push(frame)
  }
  frame(frame: SteerFrame) {
    this.onmessage?.({ data: JSON.stringify(frame) })
  }
  /** eNet killed mid-stream. */
  kill() {
    this.readyState = 3
    this.onclose?.({ code: CLOSE_ENET_LOST })
  }
  close() {
    this.closed = true
    this.readyState = 3
  }
}

/** The relay + the device at its other end: opens what the viewer sealed, answers sealed. */
class FakeTunnel implements TunnelSocket {
  readyState = 0
  onopen: (() => void) | null = null
  onmessage: ((event: { data: unknown }) => void) | null = null
  onclose: ((event: { code: number }) => void) | null = null
  received: SteerFrame[][] = []
  closed = false
  constructor(
    readonly url: string,
    private readonly device: (frames: SteerFrame[]) => SteerFrame[] | null
  ) {
    setTimeout(() => {
      this.readyState = 1
      this.onopen?.()
    }, 1)
  }
  send(raw: string) {
    const message = JSON.parse(raw) as { t: string; data: string }
    expect(message.t).toBe(`sealed`)
    // Ciphertext only: the plaintext never rides the relay.
    expect(raw).not.toContain(`join`)
    const frames = openTunnelData(SID, message.data, KEY.keys)!
    this.received.push(frames)
    const reply = this.device(frames)
    if (reply) setTimeout(() => this.push(reply), 1)
  }
  push(frames: SteerFrame[]) {
    this.onmessage?.({ data: JSON.stringify({ t: `sealed`, data: sealTunnelData(SID, frames, KEY) }) })
  }
  error(code: string) {
    this.onmessage?.({ data: JSON.stringify({ t: `tunnel_error`, code }) })
  }
  drop() {
    this.readyState = 3
    this.onclose?.({ code: 1006 })
  }
  close() {
    this.closed = true
    this.readyState = 3
  }
}

type Plan = {
  /** Each eNet attempt; undefined = it never answers. */
  enet: () => Promise<FakeEnet | null> | undefined
  keyed?: boolean
  ticket?: () => TunnelTicket
  device?: (frames: SteerFrame[]) => SteerFrame[] | null
}

function harness(plan: Plan) {
  const enets: FakeEnet[] = []
  const tunnels: FakeTunnel[] = []
  const tickets = vi.fn(async () => (plan.ticket ?? (() => ({ ticket: `t`, url: `wss://relay/tunnel?ticket=t` })))())
  const log = { applied: [] as string[], fallbacks: 0, failures: [] as string[] }
  const stats: FeedStats = {
    applied: (path, reset) => reset && log.applied.push(path),
    fallback: () => void log.fallbacks++,
    failure: (cls) => void log.failures.push(cls),
  }
  const tunnel: TunnelDeps = {
    keys: () => (plan.keyed === false ? null : KEY),
    ticket: tickets,
    socket: (url) => {
      const socket = new FakeTunnel(url, plan.device ?? ((frames) => (frames[0]?.t === `join` ? replayOf(`over relay`) : null)))
      tunnels.push(socket)
      return socket
    },
  }
  const deps = {
    enet: async () => {
      const attempt = plan.enet()
      if (!attempt) return new Promise<null>(() => {})
      const leg = await attempt
      if (leg) enets.push(leg)
      return leg
    },
    tunnel,
    stats,
  }
  return { deps, enets, tunnels, tickets, log }
}

const texts = (feed: readonly unknown[]) => feed.map((row) => (row as { text?: string }).text)

beforeEach(() => {
  vi.useFakeTimers()
  vi.stubGlobal(`URL`, { ...URL, createObjectURL: vi.fn(() => `blob:x`), revokeObjectURL: vi.fn() })
})

afterEach(() => {
  vi.useRealTimers()
  vi.unstubAllGlobals()
})

describe(`the sealed tunnel wire`, () => {
  it(`opens what it sealed; another session, garbage and oversize are dropped`, () => {
    const frames: SteerFrame[] = [{ t: `input`, data: `hi` }]
    const data = sealTunnelData(SID, frames, KEY)
    expect(data.startsWith(`v1:1:`)).toBe(true)
    expect(openTunnelData(SID, data, KEY.keys)).toEqual(frames)
    expect(openTunnelData(`other`, data, KEY.keys)).toBeNull()
    expect(openTunnelData(SID, data, new Map())).toBeNull()
    expect(openTunnelData(SID, `v1:1:garbage`, KEY.keys)).toBeNull()
    expect(openTunnelData(SID, `v2:1:${data.slice(5)}`, KEY.keys)).toBeNull()
    expect(openTunnelData(SID, `x`.repeat(STEER_TUNNEL_MAX_DATA_BYTES + 1), KEY.keys)).toBeNull()
  })
})

describe(`the carrier rule (openSteerFeed)`, () => {
  async function joined(feed: SteerFeed | null) {
    const frames: SteerFrame[] = []
    feed!.onmessage = ({ data }) => frames.push(JSON.parse(data))
    feed!.send(JSON.stringify({ t: `join`, channel: `activity` }))
    await vi.advanceTimersByTimeAsync(0)
    return frames
  }

  it(`eNet within the stall: no relay at all`, async () => {
    const h = harness({ enet: () => Promise.resolve(new FakeEnet(replayOf(`over enet`))) })
    const feed = await openSteerFeed(SID, h.deps)
    expect(feed?.carrier).toBe(`enet`)
    expect(texts((await joined(feed)).map((f) => f.event ?? {}))).toContain(`over enet`)
    await vi.advanceTimersByTimeAsync(STEER_FALLBACK_STALL_MS * 4)
    expect(h.tickets).not.toHaveBeenCalled()
    expect(h.log.fallbacks).toBe(0)
    feed!.close()
  })

  it(`eNet stalls: the tunnel opens after stallMs and carries the feed`, async () => {
    const h = harness({ enet: () => undefined })
    const opening = openSteerFeed(SID, h.deps)
    await vi.advanceTimersByTimeAsync(STEER_FALLBACK_STALL_MS - 10)
    expect(h.tickets).not.toHaveBeenCalled()
    await vi.advanceTimersByTimeAsync(20)
    const feed = await opening
    expect(h.tickets).toHaveBeenCalledWith(SID)
    expect(feed?.carrier).toBe(`relay`)
    const frames = await joined(feed)
    expect(frames.map((f) => f.t)).toEqual([`activity_reset`, `activity`, `activity_synced`])
    expect(h.log.applied).toEqual([`server_fallback`])
    // Steering goes out on the active carrier, sealed.
    feed!.send(JSON.stringify({ t: `input`, data: `hello` }))
    feed!.send(JSON.stringify({ t: `input`, data: `\r` }))
    await vi.advanceTimersByTimeAsync(0)
    expect(h.tunnels[0].received.at(-1)).toEqual([
      { t: `input`, data: `hello` },
      { t: `input`, data: `\r` },
    ])
    feed!.close()
    expect(h.tunnels[0].closed).toBe(true)
  })

  it(`eNet has no feed: the tunnel at once`, async () => {
    const h = harness({ enet: () => Promise.resolve(null) })
    const opening = openSteerFeed(SID, h.deps)
    await vi.advanceTimersByTimeAsync(10)
    const feed = await opening
    expect(feed?.carrier).toBe(`relay`)
    expect(h.log.fallbacks).toBe(1)
    feed!.close()
  })

  it(`a keyless root is eNet only (relayUnavailable): no ticket is asked for`, async () => {
    const h = harness({ enet: () => Promise.resolve(null), keyed: false })
    expect(await openSteerFeed(SID, h.deps)).toBeNull()
    expect(h.tickets).not.toHaveBeenCalled()
    expect(h.log.fallbacks).toBe(0)
  })

  it(`a host device offline on the relay (tunnel_error) and no eNet = no feed`, async () => {
    const h = harness({ enet: () => Promise.resolve(null), device: () => null })
    const opening = openSteerFeed(SID, h.deps)
    await vi.advanceTimersByTimeAsync(5)
    h.tunnels[0].error(`device_offline`)
    expect(await opening).toBeNull()
    expect(h.log.failures).toContain(`device_offline`)
  })

  it(`no ticket (disabled, unavailable) and no eNet = no feed`, async () => {
    for (const ticket of [{ disabled: true as const }, { unavailable: true as const }]) {
      const h = harness({ enet: () => Promise.resolve(null), ticket: () => ticket })
      expect(await openSteerFeed(SID, h.deps)).toBeNull()
      expect(h.tunnels).toHaveLength(0)
    }
  })

  it(`a late eNet takes over from the relay and the relay closes`, async () => {
    let release: (leg: FakeEnet) => void = () => {}
    const late = new Promise<FakeEnet>((resolve) => (release = resolve))
    const h = harness({ enet: () => late })
    const opening = openSteerFeed(SID, h.deps)
    await vi.advanceTimersByTimeAsync(STEER_FALLBACK_STALL_MS + 10)
    const feed = await opening
    const frames = await joined(feed)
    expect(feed?.carrier).toBe(`relay`)
    release(new FakeEnet(replayOf(`over enet`)))
    await vi.advanceTimersByTimeAsync(0)
    expect(feed?.carrier).toBe(`enet`)
    expect(h.tunnels[0].closed).toBe(true)
    expect(frames.filter((f) => f.t === `activity_reset`)).toHaveLength(2)
    expect(h.log.applied).toEqual([`server_fallback`, `iroh_relay`])
    feed!.close()
  })
})

describe(`the store over both carriers`, () => {
  function store(plan: Plan) {
    const h = harness(plan)
    const s = createSteerSessionStore(SID, {
      openFeed: async (id) => (await openSteerFeed(id, h.deps)) as unknown as WebSocket | null,
    })
    s.noteSessionStatus(`running`)
    return { ...h, store: s }
  }

  it(`eNet killed mid-stream: the view keeps streaming over the tunnel and moves back to eNet`, async () => {
    let attempts = 0
    let enetUp = true
    const { store: s, enets, tunnels } = store({
      enet: () => {
        attempts++
        return Promise.resolve(enetUp ? new FakeEnet(attempts === 1 ? replayOf(`one`) : replayOf(`one`, `two`)) : null)
      },
      device: (frames) => (frames[0]?.t === `join` ? replayOf(`one`) : null),
    })
    s.connect()
    await vi.advanceTimersByTimeAsync(10 + REPLAY_QUIET_MS)
    expect(s.getSnapshot().phase.kind).toBe(`live`)
    expect(texts(s.getSnapshot().feed)).toEqual([`one`])

    // eNet dies; its redials fail for now.
    enetUp = false
    enets[0].kill()
    await vi.advanceTimersByTimeAsync(10 + REPLAY_QUIET_MS)
    expect(tunnels).toHaveLength(1)
    expect(s.getSnapshot().phase.kind).toBe(`live`)
    expect(s.getSnapshot().connected).toBe(true)
    expect(texts(s.getSnapshot().feed)).toEqual([`one`])

    // Live frames now arrive over the tunnel, and steering goes out on it.
    tunnels[0].push([narration(`relayed`, 1)])
    await vi.advanceTimersByTimeAsync(REPLAY_QUIET_MS)
    expect(texts(s.getSnapshot().feed)).toEqual([`one`, `relayed`])
    expect(s.sendMessage(`hi`)).toBe(true)
    await vi.advanceTimersByTimeAsync(0)
    expect(tunnels[0].received.at(-1)).toEqual([
      { t: `input`, data: `hi` },
      { t: `input`, data: `\r` },
    ])

    // eNet comes back on its background redial: it takes over (its reset),
    // the tunnel closes, and steering is on eNet again.
    enetUp = true
    await vi.advanceTimersByTimeAsync(30_000 + REPLAY_QUIET_MS)
    expect(enets).toHaveLength(2)
    expect(tunnels[0].closed).toBe(true)
    expect(s.getSnapshot().phase.kind).toBe(`live`)
    expect(texts(s.getSnapshot().feed)).toContain(`two`)
    expect(s.interrupt()).toBe(true)
    expect(enets[1].sent).toEqual([{ t: `interrupt` }])
    s.dispose()
    expect(enets[1].closed).toBe(true)
  })

  it(`eNet stalls on open: the tunnel carries the first frame`, async () => {
    const { store: s, tunnels } = store({ enet: () => undefined })
    s.connect()
    await vi.advanceTimersByTimeAsync(STEER_FALLBACK_STALL_MS + 10 + REPLAY_QUIET_MS)
    expect(tunnels).toHaveLength(1)
    expect(s.getSnapshot().phase.kind).toBe(`live`)
    expect(texts(s.getSnapshot().feed)).toEqual([`over relay`])
    s.dispose()
  })

  it(`a keyless root: an eNet drop redials eNet only, silently`, async () => {
    let up = true
    const { store: s, enets, tickets } = store({
      enet: () => Promise.resolve(up ? new FakeEnet(replayOf(`one`)) : null),
      keyed: false,
    })
    s.connect()
    await vi.advanceTimersByTimeAsync(10 + REPLAY_QUIET_MS)
    up = false
    enets[0].kill()
    await vi.advanceTimersByTimeAsync(10)
    expect(s.getSnapshot().phase.kind).toBe(`live`)
    expect(s.getSnapshot().connected).toBe(false)
    up = true
    await vi.advanceTimersByTimeAsync(30_000 + REPLAY_QUIET_MS)
    expect(s.getSnapshot().connected).toBe(true)
    expect(tickets).not.toHaveBeenCalled()
    s.dispose()
  })

  it(`both carriers down past the grace = Connection lost`, async () => {
    let up = true
    const { store: s, enets, tunnels } = store({
      enet: () => Promise.resolve(up ? new FakeEnet(replayOf(`one`)) : null),
      device: () => null,
    })
    s.connect()
    await vi.advanceTimersByTimeAsync(10 + REPLAY_QUIET_MS)
    up = false
    enets[0].kill()
    await vi.advanceTimersByTimeAsync(5)
    // The relay is down too (the host is not on it).
    tunnels[0].error(`device_offline`)
    await vi.advanceTimersByTimeAsync(STEER_ENET_LOST_GRACE_MS - 100)
    expect(s.getSnapshot().phase.kind).toBe(`live`)
    // Every redial: eNet no feed, the relay offline again.
    for (let i = 0; i < 6 && s.getSnapshot().phase.kind === `live`; i++) {
      await vi.advanceTimersByTimeAsync(30_100)
      tunnels.at(-1)?.error(`device_offline`)
      await vi.advanceTimersByTimeAsync(10)
    }
    expect(s.getSnapshot().phase).toEqual({ kind: `closed`, terminal: false })
    expect(texts(s.getSnapshot().feed)).toEqual([`one`])
    s.dispose()
  })
})
