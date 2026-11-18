// #EXP-1941: the call seal's vectors (voice-call.json `seal`), replayed on the
// TS reference: every vector is recomputed from its inputs and every entry of
// `refused` is refused with its reason. The Rust core and, through its
// binding, the phones replay the same section.
import { createHash } from "node:crypto"
import { describe, expect, test } from "bun:test"

import {
  CALL_SEAL,
  CallHandshake,
  CallSealError,
  callHelloFrame,
  callHelloOfFrame,
  callHelloOfText,
  callHelloText,
  parseCallHello,
  type CallRole,
  type CallSeal,
} from "@exp/domain-contract/call-seal"
import fixture from "../fixtures/voice-call.json" with { type: "json" }

const seal = fixture.seal
type Inputs = typeof seal.inputs
type Direction = `up` | `down`

const hex = (bytes: Uint8Array) => Buffer.from(bytes).toString(`hex`)
const unhex = (text: string) => Uint8Array.from(Buffer.from(text, `hex`))
const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString(`base64url`)
const unb64 = (text: string) => Uint8Array.from(Buffer.from(text, `base64url`))
const sha256 = (bytes: Uint8Array) => createHash(`sha256`).update(bytes).digest(`hex`)

/** The fixed-eph constructor is private to the class: only a test reaches it. */
const fixed = (role: CallRole, i: Inputs) =>
  CallHandshake[`withEphSecret`](role, i.rootId, i.epoch, unhex(role === `caller` ? i.callerEphSecret : i.hostEphSecret))

/** A fresh pair of ends from `seal.inputs`; `seen` swaps the hello an end is shown. */
function ends(o: { with?: Partial<Inputs>; step?: [Direction, number]; seen?: { caller?: Uint8Array; host?: Uint8Array } } = {}) {
  const i = { ...seal.inputs, ...o.with }
  const pad = { ...seal, audioPadStep: { ...seal.audioPadStep, ...(o.step ? { [o.step[0]]: o.step[1] } : {}) } }
  const caller = fixed(`caller`, i)
  const host = fixed(`host`, i)
  const input = { secret: unhex(i.secret), callId: i.callId, agentId: i.agentId, deviceId: i.deviceId, pad }
  const hellos = { caller: caller.hello, host: host.hello }
  return {
    hellos,
    handshakes: { caller, host },
    input,
    caller: () => caller.finish({ ...input, peerHello: o.seen?.caller ?? hellos.host }),
    host: () => host.finish({ ...input, peerHello: o.seen?.host ?? hellos.caller }),
  }
}

const keysOf = (end: CallSeal) => {
  const keys = end[`keys`]
  return { upAudio: hex(keys.upAudio), downAudio: hex(keys.downAudio), downControl: hex(keys.downControl) }
}

/** The sealing and the opening end of a direction. */
function pairFor(direction: Direction, o: Parameters<typeof ends>[0] = {}) {
  const pair = ends(o)
  const [caller, host] = [pair.caller(), pair.host()]
  return direction === `up` ? { sealer: caller, opener: host } : { sealer: host, opener: caller }
}

const opened = (end: CallSeal, frame: string) => {
  const out = end.openAudio(unhex(frame))
  return { seq: out.seq, flags: out.flags, packet: hex(out.packet), missing: out.missing }
}

function refusal(run: () => unknown): string {
  try {
    run()
  } catch (error) {
    if (error instanceof CallSealError) return error.reason
    throw error
  }
  return `accepted`
}

describe(`EXP-1941: the call seal's constants`, () => {
  test(`the reference's constants are the fixture's`, () => {
    expect<unknown>(CALL_SEAL).toEqual({
      version: seal.version,
      domain: seal.domain,
      labels: seal.labels,
      roles: seal.roles,
      frameTypes: seal.frameTypes,
      kinds: fixture.wire.kinds,
      helloBytes: seal.helloBytes,
    })
  })

  test(`a sealed frame fits the relay's frame cap, a padded text its event cap`, () => {
    for (const step of Object.values(seal.audioPadStep)) expect(27 + step).toBeLessThanOrEqual(fixture.wire.maxFrameBytes)
    expect(seal.maxPlainBytes % seal.controlPadBytes).toBe(0)
    // b64url(ctr + plaintext + tag) inside `{"t":"sealed","data":"…"}` stays under the relay's 8192 bytes of event JSON.
    expect(Math.ceil(((4 + seal.maxPlainBytes + 16) * 4) / 3) + `{"t":"sealed","data":""}`.length).toBeLessThanOrEqual(8192)
  })
})

describe(`EXP-1941: the handshake of seal.inputs gives seal.derived`, () => {
  const pair = ends()

  test(`the hellos`, () => {
    expect(b64(pair.hellos.caller)).toBe(seal.derived.callerHello)
    expect(b64(pair.hellos.host)).toBe(seal.derived.hostHello)
    expect(pair.hellos.caller.length).toBe(seal.helloBytes)
    const caller = parseCallHello(pair.hellos.caller)
    const host = parseCallHello(pair.hellos.host)
    expect({ ...caller, eph: hex(caller.eph) }).toEqual({ version: 1, role: `caller`, epoch: seal.inputs.epoch, rootId: seal.inputs.rootId, eph: seal.derived.callerEph })
    expect({ ...host, eph: hex(host.eph) }).toEqual({ version: 1, role: `host`, epoch: seal.inputs.epoch, rootId: seal.inputs.rootId, eph: seal.derived.hostEph })
  })

  test(`the hello on the wire: a binary frame up, base64url text down`, () => {
    expect(hex(callHelloFrame(pair.hellos.caller))).toBe(seal.derived.callerHelloFrame)
    expect(callHelloOfFrame(unhex(seal.derived.callerHelloFrame))).toEqual(pair.hellos.caller)
    expect(callHelloText(pair.hellos.host)).toBe(seal.derived.hostHello)
    expect(callHelloOfText(seal.derived.hostHello)).toEqual(pair.hellos.host)
    expect(seal.derived.hostHello.length).toBe(72)
    // A hello is type 1 on the up kind only; anything else is no hello.
    const frame = unhex(seal.derived.callerHelloFrame)
    expect(callHelloOfFrame(frame.slice(0, 55))).toBeNull()
    expect(callHelloOfFrame(Uint8Array.of(2, ...frame.slice(1)))).toBeNull()
    expect(callHelloOfFrame(Uint8Array.of(1, 0, ...frame.slice(2)))).toBeNull()
    expect(callHelloOfText(`${seal.derived.hostHello}A`)).toBeNull()
    expect(callHelloOfText(`not base64url!`)).toBeNull()
  })

  test(`dh, the transcript and the three keys, the same on both ends`, () => {
    const i = seal.inputs
    const derive = (ephSecret: string, peerEph: string) =>
      CallHandshake[`derive`](unhex(ephSecret), unhex(peerEph), unhex(i.secret), i.callId, i.agentId, i.deviceId, pair.hellos.caller, pair.hellos.host)
    const caller = derive(i.callerEphSecret, seal.derived.hostEph)
    const host = derive(i.hostEphSecret, seal.derived.callerEph)
    for (const end of [caller, host]) {
      expect(hex(end.dh)).toBe(seal.derived.dh)
      expect(end.transcript.length).toBe(188)
      expect(sha256(end.transcript)).toBe(seal.derived.transcriptSha256)
    }
    expect(keysOf(pair.caller())).toEqual(seal.derived.keys)
    expect(keysOf(pair.host())).toEqual(seal.derived.keys)
  })
})

describe(`EXP-1941: seal.vectors`, () => {
  for (const vector of seal.vectors) {
    test(vector.name, () => {
      if (vector.op === `audio`) {
        const direction = vector.direction as Direction
        const { sealer, opener } = pairFor(direction, { step: [direction, vector.step!] })
        // The sealer owns its counter: `ctr` is reached by sealing, never set.
        for (let n = 0; n < vector.ctr!; n++) sealer.sealAudio(0, 0, new Uint8Array(0))
        const frame = sealer.sealAudio(vector.seq!, vector.flags!, unhex(vector.packet!))
        expect(hex(frame)).toBe(vector.frame!)
        expect(hex(frame.slice(0, 6))).toBe(vector.aad!)
        expect(frame.length).toBe(27 + vector.step!)
        if (`frameSha256` in vector) expect(sha256(frame)).toBe(vector.frameSha256!)
        expect(opened(opener, vector.frame!)).toEqual(vector.opened!)
      } else if (vector.op === `control`) {
        const { sealer, opener } = pairFor(`down`)
        const data = sealer.sealControl(vector.text!)
        expect(data).toBe(vector.data!)
        expect(hex(unb64(data).slice(0, 4))).toBe(vector.aad!)
        expect(unb64(data).length).toBe(4 + vector.plainBytes! + 16)
        const plain = opener.openControl(vector.data!)
        expect(plain).toBe(vector.text!.padEnd(vector.plainBytes!, ` `))
        // Every control parser in use takes the trailing spaces.
        expect(JSON.parse(plain)).toEqual(JSON.parse(vector.text!))
      } else if (vector.op === `open`) {
        const { opener } = pairFor(vector.direction as Direction)
        for (const frame of vector.after!) opener.openAudio(unhex(frame))
        expect(opened(opener, vector.frame!)).toEqual(vector.opened!)
      } else if (vector.op === `keys`) {
        const expected = { callerHello: vector.callerHello!, hostHello: vector.hostHello!, keys: vector.keys! }
        const lower = Object.fromEntries(Object.entries(vector.with!).map(([key, value]) => [key, value.toLowerCase()]))
        for (const inputs of [vector.with!, lower]) {
          const pair = ends({ with: inputs })
          expect({ callerHello: b64(pair.hellos.caller), hostHello: b64(pair.hellos.host), keys: keysOf(pair.caller()) }).toEqual(expected)
          expect(keysOf(pair.host())).toEqual(expected.keys)
        }
        if (`secret` in vector.with!) expect(expected.keys).toEqual(seal.derived.keys)
      } else {
        throw new Error(`unknown vector op ${vector.op}`)
      }
    })
  }
})

describe(`EXP-1941: seal.refused`, () => {
  for (const entry of seal.refused) {
    test(entry.name, () => {
      if (entry.op === `openAudio`) {
        const direction = entry.direction as Direction
        const seen = `peerHello` in entry ? { caller: unb64(entry.peerHello!) } : undefined
        const { opener } = pairFor(direction, { seen })
        for (const frame of entry.after ?? []) opener.openAudio(unhex(frame))
        const before = opener[`audioSeen`]
        expect(refusal(() => opener.openAudio(unhex(entry.frame!)))).toBe(entry.reason)
        // A refused frame moves no mark: the next good one still opens.
        expect(opener[`audioSeen`]).toBe(before)
      } else if (entry.op === `openControl`) {
        const { opener } = pairFor(`down`)
        for (const data of entry.after ?? []) opener.openControl(data)
        expect(refusal(() => opener.openControl(entry.data!))).toBe(entry.reason)
      } else if (entry.op === `sealAudio`) {
        const direction = entry.direction as Direction
        const { sealer } = pairFor(direction, { step: [direction, entry.step!] })
        if (`packetBytes` in entry) {
          expect(refusal(() => sealer.sealAudio(0, 0, new Uint8Array(entry.packetBytes!)))).toBe(entry.reason)
          // The refused packet took no counter: the empty frame in its place is ctr 0.
          expect(hex(sealer.sealAudio(0, 0, new Uint8Array(0)).slice(2, 6))).toBe(`00000000`)
        } else {
          // No end can set a counter; only this test puts one at the last value.
          sealer[`audioCtr`] = entry.ctr!
          expect(hex(sealer.sealAudio(entry.seq!, entry.flags!, unhex(entry.packet!)))).toBe(entry.frame!)
          expect(refusal(() => sealer.sealAudio(entry.seq!, entry.flags!, unhex(entry.packet!)))).toBe(entry.reason)
          expect(refusal(() => sealer.sealAudio(0, 0, new Uint8Array(0)))).toBe(entry.reason)
        }
      } else if (entry.op === `sealControl`) {
        const { sealer } = pairFor(`down`)
        expect(refusal(() => sealer.sealControl(`x`.repeat(entry.textBytes!)))).toBe(entry.reason)
        expect(unb64(sealer.sealControl(`x`.repeat(seal.maxPlainBytes))).length).toBe(4 + seal.maxPlainBytes + 16)
      } else if (entry.op === `finish`) {
        const role = entry.role as CallRole
        const pair = ends()
        const peerHello = unb64(entry.peerHello!)
        const finish = () => pair.handshakes[role].finish({ ...pair.input, peerHello })
        if (`second` in entry) finish()
        expect(refusal(finish)).toBe(entry.reason)
        // `finish` works once whatever it answered: the eph secret is gone.
        expect(pair.handshakes[role][`ephSecret`]).toBeNull()
        expect(refusal(finish)).toBe(`finished`)
      } else {
        throw new Error(`unknown refused op ${entry.op}`)
      }
    })
  }
})

describe(`EXP-1941: a live call on fresh keys`, () => {
  const i = seal.inputs
  const input = { secret: unhex(i.secret), callId: i.callId, agentId: i.agentId, deviceId: i.deviceId, pad: seal }

  test(`hello for hello, an empty frame each way, a packet, a caption`, () => {
    const callerSide = CallHandshake.create(`caller`, null, 0)
    const hostSide = CallHandshake.create(`host`, i.rootId, i.epoch)
    expect(parseCallHello(callerSide.hello).rootId).toBe(`00000000-0000-0000-0000-000000000000`)
    expect(hex(CallHandshake.create(`caller`, null, 0).hello)).not.toBe(hex(callerSide.hello))
    const caller = callerSide.finish({ ...input, peerHello: hostSide.hello })
    const host = hostSide.finish({ ...input, peerHello: callerSide.hello })
    expect(callerSide[`ephSecret`]).toBeNull()

    expect(host.openAudio(caller.sealAudio(0, 0, new Uint8Array(0)))).toEqual({ seq: 0, flags: 0, packet: new Uint8Array(0), missing: 0 })
    expect(caller.openAudio(host.sealAudio(0, 0, new Uint8Array(0))).packet.length).toBe(0)
    const packet = Uint8Array.of(0xf8, 0xff, 0xfe)
    const up = caller.sealAudio(9, 1, packet)
    expect(up.length).toBe(27 + seal.audioPadStep.up)
    expect(host.openAudio(up)).toEqual({ seq: 9, flags: 1, packet, missing: 0 })
    const down = host.sealAudio(65535, 0, new Uint8Array(seal.audioPadStep.down).fill(7))
    expect(down.length).toBe(27 + seal.audioPadStep.down)
    expect(caller.openAudio(down).packet.length).toBe(seal.audioPadStep.down)
    const caption = `{"t":"call_event","event":{"t":"partial","text":"grüß dich"}}`
    expect(caller.openControl(host.sealControl(caption)).trimEnd()).toBe(caption)
    expect(caller.openControl(host.sealControl(`x`.repeat(257))).length).toBe(512)
  })

  test(`each end seals its own direction only, and only the host seals text`, () => {
    const pair = ends()
    const [caller, host] = [pair.caller(), pair.host()]
    expect(refusal(() => caller.openAudio(caller.sealAudio(0, 0, new Uint8Array(0))))).toBe(`frame`)
    expect(refusal(() => caller.sealControl(`{}`))).toBe(`input`)
    expect(refusal(() => host.openControl(host.sealControl(`{}`)))).toBe(`input`)
    expect(refusal(() => caller.openControl(`not base64url!`))).toBe(`frame`)
  })

  test(`ids and secrets of the wrong shape are refused before anything is derived`, () => {
    expect(refusal(() => CallHandshake.create(`caller`, `not-a-uuid`, 0))).toBe(`input`)
    expect(refusal(() => CallHandshake.create(`caller`, null, -1))).toBe(`input`)
    const pair = ends()
    expect(refusal(() => pair.handshakes.caller.finish({ ...pair.input, secret: new Uint8Array(31), peerHello: pair.hellos.host }))).toBe(`input`)
    expect(refusal(() => pair.handshakes.host.finish({ ...pair.input, callId: `22222222`, peerHello: pair.hellos.caller }))).toBe(`input`)
  })

  test(`another device, agent, call, root or epoch gives keys under which nothing opens`, () => {
    const frame = ends().caller().sealAudio(0, 0, new Uint8Array(0))
    const others: Partial<Inputs>[] = [
      { deviceId: `desk-7f3b` },
      { agentId: `33333333-3333-4333-8333-333333333334` },
      { callId: `22222222-2222-4222-8222-222222222223` },
      { rootId: `11111111-1111-4111-8111-111111111112` },
      { epoch: 2 },
      { secret: `ff`.repeat(32) },
    ]
    for (const other of others) expect(refusal(() => ends({ with: other }).host().openAudio(frame))).toBe(`tag`)
    expect(ends().host().openAudio(frame).missing).toBe(0)
  })
})
