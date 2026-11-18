// #EXP-1941: the relay's canary. A host and a caller built on the call seal's
// TS reference hold a sealed call through the REAL hub, with a canary in the
// member's words, in the reply and in the Opus payload. Everything the relay
// touches is captured: every frame it receives, what `parseClientFrame` makes
// of each, every frame it sends, every log line, its counters. The canary is
// in none of it. A control case holds the same call UNSEALED and does find
// it: the probe works, and that is today's call until the ends seal.
import { describe, expect, spyOn, test } from "bun:test"
import type { SteerTicketClaims } from "@exp/steer-ticket"
import { CallHandshake, callHelloFrame, callHelloOfFrame, callHelloOfText, callHelloText } from "@exp/domain-contract/call-seal"
import voiceCall from "@exp/etalk/fixtures/voice-call.json" with { type: "json" }
import { Hub, type RelaySocket } from "./hub"
import { CALL_KIND_DOWN, CALL_KIND_UP, parseClientFrame } from "./protocol"

const CANARY = `canary-7f3a`
const seal = voiceCall.seal
const utf8 = new TextEncoder()

/** The canary as the relay could meet it: raw, base64, base64url, hex. */
const SHAPES = [CANARY, Buffer.from(CANARY).toString(`base64`).replace(/=+$/, ``), Buffer.from(CANARY).toString(`base64url`), Buffer.from(CANARY).toString(`hex`)]
const holds = (text: string) => SHAPES.some((shape) => text.includes(shape))

class Tap implements RelaySocket {
  sent: string[] = []
  binary: Uint8Array[] = []
  send(data: string) {
    this.sent.push(data)
  }
  sendBinary(data: Uint8Array) {
    this.binary.push(data)
  }
  close() {}
  bufferedAmount() {
    return 0
  }
  events() {
    return this.sent.map((d) => JSON.parse(d) as { t: string; event?: { t: string; data?: string } }).filter((f) => f.t === `call_event`)
  }
}

function claims(overrides: Partial<SteerTicketClaims>): SteerTicketClaims {
  return { sub: `owner`, team: ``, role: `control`, exp: Math.floor(Date.now() / 1000) + 60, ...overrides } as SteerTicketClaims
}

/** A paired call on a real hub, with a record of all the relay saw and did. */
function call() {
  const hub = new Hub()
  const logs: string[] = []
  const spies = ([`log`, `info`, `warn`, `error`, `debug`] as const).map((level) =>
    spyOn(console, level).mockImplementation((...args: unknown[]) => void logs.push(args.map(String).join(` `)))
  )
  const [device, caller, host] = [new Tap(), new Tap(), new Tap()]
  const ids = { callId: seal.inputs.callId, agentId: seal.inputs.agentId, deviceId: seal.inputs.deviceId }
  hub.onOpen(device, claims({ role: `control` }))
  hub.onMessage(device, JSON.stringify({ t: `online`, deviceId: ids.deviceId }))
  hub.onOpen(caller, claims({ role: `caller`, deviceOwnerId: `owner`, ...ids }))
  hub.onOpen(host, claims({ role: `call_host`, callId: ids.callId, deviceId: ids.deviceId }))
  const received: string[] = []
  const parsed: string[] = []
  const up: Uint8Array[] = []
  const down: Uint8Array[] = []
  return {
    hub,
    caller,
    host,
    up,
    down,
    text(from: Tap, frame: object) {
      const raw = JSON.stringify(frame)
      received.push(raw)
      parsed.push(JSON.stringify(parseClientFrame(raw)))
      hub.onMessage(from, raw)
    },
    binary(from: Tap, frame: Uint8Array) {
      ;(from === caller ? up : down).push(frame)
      hub.onBinary(from, frame)
    },
    /** Hangs up and gives back every byte the relay saw, wrote or logged, as text. */
    end() {
      hub.onMessage(caller, JSON.stringify({ t: `call_end` }))
      const counters = hub.counters()
      hub.destroy()
      for (const spy of spies) spy.mockRestore()
      const sockets = [device, caller, host]
      const latin = (frames: Uint8Array[]) => frames.map((f) => Buffer.from(f).toString(`latin1`))
      const record = [
        ...received,
        ...parsed,
        ...sockets.flatMap((s) => s.sent),
        ...latin([...up, ...down]),
        ...sockets.flatMap((s) => latin(s.binary)),
        ...logs,
        JSON.stringify(counters),
      ]
      return { counters, record, logs }
    },
  }
}

const said = { t: `call_event`, event: { t: `said`, text: `please read ${CANARY} back to me` } }
const reply = { t: `call_event`, event: { t: `replyStarted`, text: `It is ${CANARY}.` } }
const opus = (n: number) => Uint8Array.of(0xf8, n, ...utf8.encode(CANARY))

describe(`EXP-1941: the relay reads nothing of a sealed call`, () => {
  test(`a sealed call: the canary is in nothing the relay receives, parses, sends, logs or counts`, () => {
    const line = call()
    const i = seal.inputs
    const input = { secret: Uint8Array.from(Buffer.from(i.secret, `hex`)), callId: i.callId, agentId: i.agentId, deviceId: i.deviceId, pad: seal }
    // -> e: the caller's hello, a binary frame on the up kind.
    const callerSide = CallHandshake.create(`caller`, i.rootId, i.epoch)
    line.binary(line.caller, callHelloFrame(callerSide.hello))
    const hosted = CallHandshake.create(`host`, i.rootId, i.epoch).finish({ ...input, peerHello: callHelloOfFrame(line.host.binary[0]!)! })
    // <- e, ee, psk: the host's hello with its tag, inside today's call_event.
    line.text(line.host, { t: `call_event`, event: { t: `hello`, data: callHelloText(hosted.hostHello) } })
    const hello = callHelloOfText(line.caller.events()[0]!.event!.data!)!
    const callerSeal = callerSide.finish({ ...input, rootId: i.rootId, epoch: i.epoch, peerHello: hello }).seal
    const hostSeal = hosted.seal

    for (let n = 0; n < 5; n++) line.binary(line.caller, callerSeal.sealAudio(n, 0, opus(n)))
    const heard = line.host.binary.slice(1).map((frame) => hostSeal.openAudio(frame))
    line.text(line.host, { t: `call_event`, event: { t: `sealed`, data: hostSeal.sealControl(JSON.stringify(said)) } })
    line.text(line.host, { t: `call_event`, event: { t: `sealed`, data: hostSeal.sealControl(JSON.stringify(reply)) } })
    for (let n = 0; n < 5; n++) line.binary(line.host, hostSeal.sealAudio(n, 0, opus(n)))

    // The ends DID exchange it: the call works through the relay.
    expect(heard.map((frame) => Buffer.from(frame.packet).toString(`latin1`)).every(holds)).toBe(true)
    const events = line.caller.events().slice(1).map((frame) => JSON.parse(callerSeal.openControl(frame.event!.data!)))
    expect(events).toEqual([said, reply])
    expect(line.caller.binary.map((frame) => holds(Buffer.from(callerSeal.openAudio(frame).packet).toString(`latin1`)))).toEqual(Array(5).fill(true))

    // Forwarded binary equals sent binary, and each direction has ONE frame length.
    expect(line.host.binary).toEqual(line.up)
    expect(line.caller.binary).toEqual(line.down)
    expect([...new Set(line.up.slice(1).map((frame) => frame.length))]).toEqual([27 + seal.audioPadStep.up])
    expect([...new Set(line.down.map((frame) => frame.length))]).toEqual([27 + seal.audioPadStep.down])
    expect(line.up[0]![0]).toBe(CALL_KIND_UP)
    expect(line.down[0]![0]).toBe(CALL_KIND_DOWN)

    const { counters, record } = line.end()
    expect(record.length).toBeGreaterThan(30)
    expect(record.filter(holds)).toEqual([])
    expect({ sealed: counters.callsSealed, legacy: counters.callsLegacy, forwarded: counters.callFramesForwarded }).toEqual({ sealed: 1, legacy: 0, forwarded: 11 })
  })

  test(`the control: the same call unsealed, and the probe finds the canary everywhere`, () => {
    const line = call()
    const frame = (kind: number, n: number) => Uint8Array.of(kind, 0, n, ...opus(n))
    for (let n = 0; n < 5; n++) line.binary(line.caller, frame(CALL_KIND_UP, n))
    line.text(line.host, said)
    line.text(line.host, reply)
    for (let n = 0; n < 5; n++) line.binary(line.host, frame(CALL_KIND_DOWN, n))
    const { counters, record } = line.end()
    // What it received, what it parsed, what it sent on, both ways, text and audio.
    expect(record.filter(holds).length).toBe(2 * 3 + 10 * 2)
    expect({ sealed: counters.callsSealed, legacy: counters.callsLegacy }).toEqual({ sealed: 0, legacy: 1 })
  })

  test(`a call nobody answered is neither sealed nor legacy`, () => {
    const hub = new Hub()
    const [device, caller] = [new Tap(), new Tap()]
    hub.onOpen(device, claims({ role: `control` }))
    hub.onMessage(device, JSON.stringify({ t: `online`, deviceId: `mint` }))
    hub.onOpen(caller, claims({ role: `caller`, deviceOwnerId: `owner`, callId: `call-1`, agentId: `agent-1`, deviceId: `mint` }))
    hub.onMessage(caller, JSON.stringify({ t: `call_end` }))
    expect({ sealed: hub.counters().callsSealed, legacy: hub.counters().callsLegacy }).toEqual({ sealed: 0, legacy: 0 })
    hub.destroy()
  })
})
