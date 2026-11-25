import { describe, expect, it, vi } from "vitest"
import type { CallLogEnd } from "@exp/etalk/voice-call"

import type { CallAudio } from "./audio"
import { VoiceCall, type CallSocket } from "./call"
import { reportCallEnd } from "./log"

const notices = vi.hoisted(() => [] as unknown[])
vi.mock(`@/lib/observability/client`, () => ({ captureClientNotice: (notice: unknown) => notices.push(notice) }))

// EXP-2111: the web caller says the same lines as the phones (the fixture's
// `log.cases` replay in @exp/etalk), through the real VoiceCall.
async function call() {
  const lines: string[] = []
  const ends: CallLogEnd[] = []
  const socket = { binaryType: `blob`, readyState: 1, onopen: null, onmessage: null, onclose: null, onerror: null, send() {}, close() {} } as unknown as CallSocket
  const audio = { onPacket: null, onLevel: null, onError: null, setMuted() {}, play() {}, flush() {}, close() {} } as unknown as CallAudio
  const voice = new VoiceCall({
    startCall: async () => ({ url: `wss://relay/ws?ticket=abc`, device: `mint`, fallback: false, callId: `4410cd11-1111-4222-8333-444455556666` }),
    openSocket: () => socket,
    openAudio: async () => audio,
    log: (line) => lines.push(line),
    ended: (end) => ends.push(end),
  })
  await voice.start(`agent-1`, `mint`)
  const text = (frame: unknown) => socket.onmessage?.({ data: JSON.stringify(frame) } as MessageEvent)
  socket.onopen?.({} as Event)
  text({ t: `call_ready` })
  text({ t: `call_event`, event: { t: `rang` } })
  text({ t: `call_event`, event: { t: `ready` } })
  text({ t: `call_event`, event: { t: `said`, text: `words of the call` } })
  audio.onPacket?.(new Uint8Array([1, 2]))
  socket.onmessage?.({ data: new Uint8Array([2, 0, 0, 9]).buffer } as MessageEvent)
  return { voice, lines, ends, text, socket }
}

describe(`EXP-2111 the web caller's log`, () => {
  it(`a dropped call reads from its lines alone and is one event`, async () => {
    const { lines, ends, socket } = await call()
    socket.onclose?.({} as CloseEvent)
    expect(lines.map((line) => line.replace(/\d+ ms/g, `N ms`))).toEqual([
      `call -: started (fallback false)`,
      `call -: the microphone is held`,
      `call 4410cd11: ticket after N ms (fallback false, sealed false)`,
      `call 4410cd11: the relay socket is open after N ms`,
      `call 4410cd11: call_ready: the host joined after N ms`,
      `call 4410cd11: the host rang`,
      `call 4410cd11: the host's voice is ready`,
      `call 4410cd11: answered after N ms`,
      `call 4410cd11: the first microphone frame went out after N ms`,
      `call 4410cd11: the host's first audio frame came after N ms`,
      `call 4410cd11: ended by=link reason=socket_closed stage=paired phase=thinking answered=true lasted=N ms sealed=false up=1 down=1 last_down=N ms ago`,
    ])
    expect(ends).toMatchObject([{ tag: `4410cd11`, by: `link`, reason: `socket_closed` }])
    expect(lines.join()).not.toMatch(/words of the call|mint|ticket=|agent-1/)
    reportCallEnd(ends[0]!)
    expect(notices).toMatchObject([
      { message: `Voice call ended: socket_closed`, fingerprint: [`voice-call-end`, `socket_closed`], tags: { call_tag: `4410cd11`, call_by: `link` }, extra: { answered: true, upFrames: 1 } },
    ])
  })

  it(`End call is no event`, async () => {
    const { voice, ends } = await call()
    voice.end()
    expect(ends).toEqual([])
  })
})
