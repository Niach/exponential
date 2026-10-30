// @vitest-environment node
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { encodeSteerEnvelope, parseSteerEnvelope } from "@exp/domain-contract/steer-enet"
import { serveSteerBridge, SteerBridgeClient, type BridgeChannel, type BridgeMessage, type SteerNode } from "./steer-bridge"
import { openEnetSteer } from "./steer-enet-viewer"

// VAPP-33: tabs without the node reach it through the node tab.

/** BroadcastChannel semantics: every OTHER member hears a post, async. */
function bus() {
  const members = new Set<BridgeChannel>()
  return () => {
    const channel: BridgeChannel = {
      onmessage: null,
      postMessage: (data: BridgeMessage) => {
        for (const other of members) {
          if (other !== channel) setTimeout(() => other.onmessage?.({ data: structuredClone(data) }), 0)
        }
      },
      close: () => void members.delete(channel),
    }
    members.add(channel)
    return channel
  }
}

function fakeNode() {
  const listeners = new Set<(from: string, body: Uint8Array) => void>()
  const sent: Array<{ to: string; frames: string[] }> = []
  const pins = new Map<string, number>()
  const node: SteerNode & { hear: (from: string, body: Uint8Array) => void } = {
    onChat: (listener) => {
      listeners.add(listener)
      return () => void listeners.delete(listener)
    },
    sendChat: async (to, body) => {
      if (to === `down`) throw new Error(`unreachable`)
      const envelope = parseSteerEnvelope(body)
      sent.push({ to, frames: envelope ? envelope.frames.map((f) => f.t) : [new TextDecoder().decode(body)] })
    },
    pin: (id) => {
      pins.set(id, (pins.get(id) ?? 0) + 1)
      return () => pins.set(id, pins.get(id)! - 1)
    },
    hear: (from, body) => {
      for (const listener of listeners) listener(from, body)
    },
  }
  return { node, sent, pins }
}

const env = (session: string, ...types: string[]) => encodeSteerEnvelope(session, types.map((t) => ({ t })))

beforeEach(() => {
  vi.useFakeTimers()
})
afterEach(() => {
  vi.useRealTimers()
})

describe(`the bridge`, () => {
  it(`no node tab answering = no carrier (no feed)`, async () => {
    const open = bus()
    const client = new SteerBridgeClient(open())
    const carrier = client.carrier()
    await vi.advanceTimersByTimeAsync(500)
    expect(await carrier).toBeUndefined()
  })

  it(`sends, pins and hears through the node tab`, async () => {
    const open = bus()
    const { node, sent, pins } = fakeNode()
    serveSteerBridge(node, open())
    const client = new SteerBridgeClient(open())
    const found = client.carrier()
    await vi.advanceTimersByTimeAsync(1)
    const carrier = (await found)!
    expect(carrier).toBeDefined()

    const release = carrier.allow(`d1`)
    const sending = carrier.send(`d1`, env(`s1`, `join`))
    await vi.advanceTimersByTimeAsync(1)
    await sending
    expect(sent).toEqual([{ to: `d1`, frames: [`join`] }])
    expect(pins.get(`d1`)).toBe(1)
    release()
    await vi.advanceTimersByTimeAsync(1)
    expect(pins.get(`d1`)).toBe(0)

    const failing = carrier.send(`down`, env(`s1`, `join`))
    failing.catch(() => {})
    await vi.advanceTimersByTimeAsync(1)
    await expect(failing).rejects.toThrow(/unreachable/)

    const heard: string[] = []
    carrier.subscribe((from, body) => heard.push(`${from}:${parseSteerEnvelope(body)?.frames[0]?.t}`))
    node.hear(`d1`, env(`s1`, `keepalive`))
    node.hear(`d1`, new TextEncoder().encode(`a plain chat`))
    await vi.advanceTimersByTimeAsync(1)
    expect(heard).toEqual([`d1:keepalive`])
  })

  it(`one tab's leave never ends another tab's feed; the last one leaves`, async () => {
    const open = bus()
    const { node, sent } = fakeNode()
    const served = serveSteerBridge(node, open())
    const other = new SteerBridgeClient(open())
    const found = other.carrier()
    await vi.advanceTimersByTimeAsync(1)
    const remote = (await found)!

    await served.carrier.send(`d1`, env(`s1`, `join`))
    const joining = remote.send(`d1`, env(`s1`, `join`))
    await vi.advanceTimersByTimeAsync(1)
    await joining
    const leaving = remote.send(`d1`, env(`s1`, `input`, `leave`))
    await vi.advanceTimersByTimeAsync(1)
    await leaving
    await served.carrier.send(`d1`, env(`s1`, `leave`))
    expect(sent.map((s) => s.frames)).toEqual([[`join`], [`join`], [`input`], [`leave`]])

    // A tab that closes without leaving counts down too.
    const rejoining = remote.send(`d1`, env(`s1`, `join`))
    await vi.advanceTimersByTimeAsync(1)
    await rejoining
    other.gone()
    await vi.advanceTimersByTimeAsync(1)
    expect(sent.at(-1)).toEqual({ to: `d1`, frames: [`leave`] })
  })

  it(`a bridged tab joins a device end to end`, async () => {
    const open = bus()
    const { node } = fakeNode()
    const replay = encodeSteerEnvelope(`s1`, [{ t: `activity_reset` }, { t: `activity_synced`, firstSeq: 0, lastSeq: 0 }])
    node.sendChat = async (to, body) => {
      if (parseSteerEnvelope(body)?.frames[0]?.t === `join`) setTimeout(() => node.hear(to, replay), 1)
    }
    serveSteerBridge(node, open())
    const client = new SteerBridgeClient(open())
    const opening = openEnetSteer(`s1`, {
      carrier: () => client.carrier(),
      forSession: async () => [{ endpointId: `d1`, kind: `desktop` }],
    })
    await vi.advanceTimersByTimeAsync(10)
    expect((await opening)?.endpointId).toBe(`d1`)
  })
})
