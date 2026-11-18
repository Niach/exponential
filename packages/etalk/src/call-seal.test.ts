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

/**
 * A fresh pair of ends from `seal.inputs`. The host finishes first (its hello
 * carries its tag), the caller takes that hello with its own root and epoch.
 */
function ends(o: { with?: Partial<Inputs>; step?: [Direction, number]; after?: string[] } = {}) {
  const i = { ...seal.inputs, ...o.with }
  const pad = { ...seal, audioPadStep: { ...seal.audioPadStep, ...(o.step ? { [o.step[0]]: o.step[1] } : {}) } }
  const handshakes = { caller: fixed(`caller`, i), host: fixed(`host`, i) }
  const input = { secret: unhex(i.secret), callId: i.callId, agentId: i.agentId, deviceId: i.deviceId, pad }
  const target = { rootId: i.rootId, epoch: i.epoch }
  const callerHello = handshakes.caller.hello
  const hosted = handshakes.host.finish({ ...input, peerHello: callerHello })
  const caller = handshakes.caller.finish({ ...input, ...target, peerHello: hosted.hostHello }).seal
  // `after`: the up frames the HOST's object opens first (the caller's proof).
  for (const frame of o.after ?? []) hosted.seal.openAudio(unhex(frame))
  return { hellos: { caller: callerHello, host: hosted.hostHello }, input, target, caller, host: hosted.seal }
}

/** One end that has not finished, with what the other end would send it. */
function unfinished(role: CallRole) {
  const pair = ends()
  const handshake = fixed(role, seal.inputs)
  return { handshake, finish: (peerHello: Uint8Array, target: Partial<typeof pair.target> = {}) => handshake.finish({ ...pair.input, ...pair.target, ...target, peerHello }) }
}

const keysOf = (end: CallSeal) => {
  const keys = end[`keys`]
  return { upAudio: hex(keys.upAudio), downAudio: hex(keys.downAudio), downControl: hex(keys.downControl) }
}

/** The sealing and the opening end of a direction. */
function pairFor(direction: Direction, o: Parameters<typeof ends>[0] = {}) {
  const pair = ends(o)
  return direction === `up` ? { sealer: pair.caller, opener: pair.host } : { sealer: pair.host, opener: pair.caller }
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
    expect(Object.values(seal.labels).length).toBe(4)
    // b64url(ctr + plaintext + tag) inside `{"t":"sealed","data":"…"}` stays under the relay's 8192 bytes of event JSON.
    expect(Math.ceil(((4 + seal.maxPlainBytes + 16) * 4) / 3) + `{"t":"sealed","data":""}`.length).toBeLessThanOrEqual(8192)
  })
})

describe(`EXP-1941: the handshake of seal.inputs gives seal.derived`, () => {
  const pair = ends()
  const body = pair.hellos.host.slice(0, 54)

  test(`the hellos: the caller's 54 bytes, the host's 70 with its tag`, () => {
    expect(b64(pair.hellos.caller)).toBe(seal.derived.callerHello)
    expect(b64(pair.hellos.host)).toBe(seal.derived.hostHello)
    expect(hex(pair.hellos.host.slice(54))).toBe(seal.derived.hostHelloTag)
    expect([pair.hellos.caller.length, pair.hellos.host.length]).toEqual([seal.helloBytes.caller, seal.helloBytes.host])
    const caller = parseCallHello(pair.hellos.caller)
    const host = parseCallHello(pair.hellos.host)
    const shown = (hello: typeof host) => ({ version: hello.version, role: hello.role, epoch: hello.epoch, rootId: hello.rootId, eph: hex(hello.eph), tag: hello.tag && hex(hello.tag) })
    expect(shown(caller)).toEqual({ version: 1, role: `caller`, epoch: seal.inputs.epoch, rootId: seal.inputs.rootId, eph: seal.derived.callerEph, tag: null })
    expect(shown(host)).toEqual({ version: 1, role: `host`, epoch: seal.inputs.epoch, rootId: seal.inputs.rootId, eph: seal.derived.hostEph, tag: seal.derived.hostHelloTag })
    // A host has no hello before it has the key: only `finish` gives it.
    expect(refusal(() => fixed(`host`, seal.inputs).hello)).toBe(`input`)
  })

  test(`the hello on the wire: a binary frame up, base64url text down`, () => {
    expect(hex(callHelloFrame(pair.hellos.caller))).toBe(seal.derived.callerHelloFrame)
    expect(callHelloOfFrame(unhex(seal.derived.callerHelloFrame))).toEqual(pair.hellos.caller)
    expect(callHelloText(pair.hellos.host)).toBe(seal.derived.hostHello)
    expect(callHelloOfText(seal.derived.hostHello)).toEqual(pair.hellos.host)
    expect(seal.derived.hostHello.length).toBe(94)
    // A hello is type 1 on the up kind only; anything else is no hello.
    const frame = unhex(seal.derived.callerHelloFrame)
    expect(callHelloOfFrame(frame.slice(0, 55))).toBeNull()
    expect(callHelloOfFrame(Uint8Array.of(2, ...frame.slice(1)))).toBeNull()
    expect(callHelloOfFrame(Uint8Array.of(1, 0, ...frame.slice(2)))).toBeNull()
    expect(callHelloOfText(seal.derived.callerHello)).toBeNull()
    expect(callHelloOfText(`not base64url!`)).toBeNull()
  })

  test(`dh, the transcript and the four keys, the same on both ends`, () => {
    const i = seal.inputs
    const derive = (ephSecret: string, peerEph: string) =>
      CallHandshake[`derive`](unhex(ephSecret), unhex(peerEph), unhex(i.secret), i.callId, i.agentId, i.deviceId, pair.hellos.caller, body)
    for (const end of [derive(i.callerEphSecret, seal.derived.hostEph), derive(i.hostEphSecret, seal.derived.callerEph)]) {
      expect(hex(end.dh)).toBe(seal.derived.dh)
      expect(end.transcript.length).toBe(188)
      expect(sha256(end.transcript)).toBe(seal.derived.transcriptSha256)
      expect(hex(end.helloKey)).toBe(seal.derived.keys.downHello)
    }
    const { downHello: _, ...keys } = seal.derived.keys
    expect(keysOf(pair.caller)).toEqual(keys)
    expect(keysOf(pair.host)).toEqual(keys)
  })
})

describe(`EXP-1941: seal.vectors`, () => {
  for (const vector of seal.vectors) {
    test(vector.name, () => {
      if (vector.op === `audio`) {
        const direction = vector.direction as Direction
        const { sealer, opener } = pairFor(direction, { step: [direction, vector.step!], after: vector.after })
        // The sealer owns its counter: `ctr` is reached by sealing, never set.
        for (let n = 0; n < vector.ctr!; n++) sealer.sealAudio(0, 0, new Uint8Array(0))
        const frame = sealer.sealAudio(vector.seq!, vector.flags!, unhex(vector.packet!))
        expect(hex(frame)).toBe(vector.frame!)
        expect(hex(frame.slice(0, 6))).toBe(vector.aad!)
        expect(frame.length).toBe(27 + vector.step!)
        if (`frameSha256` in vector) expect(sha256(frame)).toBe(vector.frameSha256!)
        expect(opened(opener, vector.frame!)).toEqual(vector.opened!)
      } else if (vector.op === `hello`) {
        const pair = ends()
        expect(b64(pair.hellos.host)).toBe(vector.hostHello!)
        expect(hex(pair.hellos.host.slice(0, 54))).toBe(vector.aad!)
        expect(hex(pair.hellos.host.slice(54))).toBe(vector.tag!)
        // The caller's finish opened it: `ends` made the caller from these very bytes.
        expect(unfinished(`caller`).finish(unb64(vector.hostHello!)).hostHello).toEqual(pair.hellos.host)
      } else if (vector.op === `control`) {
        const { sealer, opener } = pairFor(`down`, { after: vector.after })
        const data = sealer.sealControl(vector.text!)
        expect(data).toBe(vector.data!)
        expect(hex(unb64(data).slice(0, 4))).toBe(vector.aad!)
        expect(unb64(data).length).toBe(4 + vector.plainBytes! + 16)
        const plain = opener.openControl(vector.data!)
        expect(plain).toBe(vector.text!.padEnd(vector.plainBytes!, ` `))
        // Every control parser in use takes the trailing spaces.
        expect(JSON.parse(plain)).toEqual(JSON.parse(vector.text!))
      } else if (vector.op === `open`) {
        const { opener } = pairFor(vector.direction as Direction, { after: vector.after })
        expect(opened(opener, vector.frame!)).toEqual(vector.opened!)
      } else if (vector.op === `keys`) {
        const expected = { callerHello: vector.callerHello!, hostHello: vector.hostHello!, keys: vector.keys! }
        const lower = Object.fromEntries(Object.entries(vector.with!).map(([key, value]) => [key, value.toLowerCase()]))
        for (const inputs of [vector.with!, lower]) {
          const pair = ends({ with: inputs })
          expect({ callerHello: b64(pair.hellos.caller), hostHello: b64(pair.hellos.host), keys: keysOf(pair.caller) }).toEqual(expected)
          expect(keysOf(pair.host)).toEqual(expected.keys)
        }
        if (`secret` in vector.with!) expect(expected).toMatchObject({ hostHello: seal.derived.hostHello, keys: keysOf(ends().caller) })
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
        const { opener } = pairFor(entry.direction as Direction, { after: entry.after })
        const before = opener[`audioSeen`]
        expect(refusal(() => opener.openAudio(unhex(entry.frame!)))).toBe(entry.reason)
        // A refused frame moves no mark: the next good one still opens.
        expect(opener[`audioSeen`]).toBe(before)
      } else if (entry.op === `openControl`) {
        const { opener } = pairFor(`down`)
        for (const data of entry.afterData!) opener.openControl(data)
        expect(refusal(() => opener.openControl(entry.data!))).toBe(entry.reason)
      } else if (entry.op === `sealAudio`) {
        const direction = entry.direction as Direction
        const { sealer } = pairFor(direction, { step: [direction, entry.step!] })
        if (`packetBytes` in entry) {
          expect(refusal(() => sealer.sealAudio(0, 0, new Uint8Array(entry.packetBytes!)))).toBe(entry.reason)
          // What was refused took no counter.
          expect(sealer[`audioCtr`]).toBe(0)
        } else {
          // No end can set a counter; only this test puts one at the last value.
          sealer[`audioCtr`] = entry.ctr!
          expect(hex(sealer.sealAudio(entry.seq!, entry.flags!, unhex(entry.packet!)))).toBe(entry.frame!)
          expect(refusal(() => sealer.sealAudio(entry.seq!, entry.flags!, unhex(entry.packet!)))).toBe(entry.reason)
          expect(refusal(() => sealer.sealAudio(0, 0, new Uint8Array(0)))).toBe(entry.reason)
        }
      } else if (entry.op === `sealControl`) {
        const { sealer } = pairFor(`down`, { after: entry.after })
        expect(refusal(() => sealer.sealControl(`x`.repeat(entry.textBytes!)))).toBe(entry.reason)
        expect(sealer[`controlCtr`]).toBe(0)
        if (entry.after) expect(unb64(sealer.sealControl(`x`.repeat(seal.maxPlainBytes))).length).toBe(4 + seal.maxPlainBytes + 16)
      } else if (entry.op === `finish`) {
        const end = unfinished(entry.role as CallRole)
        const target = { ...(`rootId` in entry ? { rootId: entry.rootId! } : {}), ...(`epoch` in entry ? { epoch: entry.epoch! } : {}) }
        if (`first` in entry) refusal(() => end.finish(unb64(entry.first!)))
        expect(refusal(() => end.finish(unb64(entry.peerHello!), target))).toBe(entry.reason)
        // `finish` works once whatever it answered: the eph secret is gone, there is no second try.
        expect(end.handshake[`ephSecret`]).toBeNull()
        expect(refusal(() => end.finish(unb64(seal.derived.hostHello)))).toBe(`finished`)
      } else {
        throw new Error(`unknown refused op ${entry.op}`)
      }
    })
  }
})

describe(`EXP-1941: a live call on fresh keys`, () => {
  const i = seal.inputs
  const input = { secret: unhex(i.secret), callId: i.callId, agentId: i.agentId, deviceId: i.deviceId, pad: seal }
  const target = { rootId: i.rootId, epoch: i.epoch }

  test(`a first call: hello for hello with its tag, a packet up, a caption down`, () => {
    // The caller knew no root when it dialled (zeros); it asked again before it finished.
    const callerSide = CallHandshake.create(`caller`, null, 0)
    const hostSide = CallHandshake.create(`host`, i.rootId, i.epoch)
    expect(parseCallHello(callerSide.hello).rootId).toBe(`00000000-0000-0000-0000-000000000000`)
    expect(hex(CallHandshake.create(`caller`, null, 0).hello)).not.toBe(hex(callerSide.hello))
    const hosted = hostSide.finish({ ...input, peerHello: callerSide.hello })
    const [caller, host] = [callerSide.finish({ ...input, ...target, peerHello: hosted.hostHello }).seal, hosted.seal]
    expect(callerSide[`ephSecret`]).toBeNull()

    const packet = Uint8Array.of(0xf8, 0xff, 0xfe)
    expect(refusal(() => host.sealControl(`{}`))).toBe(`unproven`)
    const up = caller.sealAudio(9, 1, packet)
    expect(up.length).toBe(27 + seal.audioPadStep.up)
    expect(host.openAudio(up)).toEqual({ seq: 9, flags: 1, packet, missing: 0 })
    const down = host.sealAudio(65535, 0, new Uint8Array(seal.audioPadStep.down).fill(7))
    expect(down.length).toBe(27 + seal.audioPadStep.down)
    expect(caller.openAudio(down).packet.length).toBe(seal.audioPadStep.down)
    const caption = `{"t":"call_event","event":{"t":"partial","text":"grüß dich"}}`
    expect(caller.openControl(host.sealControl(caption)).trimEnd()).toBe(caption)
    expect(caller.openControl(host.sealControl(`x`.repeat(257))).length).toBe(512)
    // An empty frame is the one packet its sender could not seal: one missing.
    expect(host.openAudio(caller.sealAudio(10, 0, new Uint8Array(0)))).toEqual({ seq: 10, flags: 0, packet: new Uint8Array(0), missing: 1 })
  })

  test(`a relay that answers with its own eph gets no sealer, with any tag`, () => {
    const pair = ends()
    const relay = CallHandshake.create(`host`, i.rootId, i.epoch).finish({ ...input, secret: new Uint8Array(32), peerHello: pair.hellos.caller }).hostHello
    const stolen = Uint8Array.of(...relay.slice(0, 54), ...pair.hellos.host.slice(54))
    for (const hello of [relay, stolen]) expect(refusal(() => unfinished(`caller`).finish(hello))).toBe(`tag`)
  })

  test(`each end seals its own direction only, and only the host seals text`, () => {
    const { caller, host } = ends({ after: [seal.vectors[0]!.frame!] })
    expect(refusal(() => caller.openAudio(caller.sealAudio(0, 0, new Uint8Array(0))))).toBe(`frame`)
    expect(refusal(() => caller.sealControl(`{}`))).toBe(`input`)
    expect(refusal(() => host.openControl(host.sealControl(`{}`)))).toBe(`input`)
    expect(refusal(() => caller.openControl(`not base64url!`))).toBe(`frame`)
  })

  test(`ids and secrets of the wrong shape are refused before anything is derived`, () => {
    expect(refusal(() => CallHandshake.create(`caller`, `not-a-uuid`, 0))).toBe(`input`)
    expect(refusal(() => CallHandshake.create(`caller`, null, -1))).toBe(`input`)
    const pair = ends()
    const host = () => fixed(`host`, seal.inputs)
    expect(refusal(() => host().finish({ ...pair.input, secret: new Uint8Array(31), peerHello: pair.hellos.caller }))).toBe(`input`)
    expect(refusal(() => host().finish({ ...pair.input, callId: `22222222`, peerHello: pair.hellos.caller }))).toBe(`input`)
    // A caller's finish without its own root and epoch is no finish.
    expect(refusal(() => fixed(`caller`, seal.inputs).finish({ ...pair.input, peerHello: pair.hellos.host }))).toBe(`input`)
  })

  test(`another device, agent, call or secret: the host's tag does not open, no sealer exists`, () => {
    const others: Partial<Inputs>[] = [
      { deviceId: `desk-7f3b` },
      { agentId: `33333333-3333-4333-8333-333333333334` },
      { callId: `22222222-2222-4222-8222-222222222223` },
      { secret: `ff`.repeat(32) },
    ]
    for (const other of others) expect(refusal(() => unfinished(`caller`).finish(ends({ with: other }).hellos.host))).toBe(`tag`)
    // And the caller's proof: under other keys its frame opens nowhere.
    const frame = ends().caller.sealAudio(0, 0, Uint8Array.of(1))
    for (const other of others) expect(refusal(() => ends({ with: other }).host.openAudio(frame))).toBe(`tag`)
    expect(ends().host.openAudio(frame).missing).toBe(0)
  })
})
