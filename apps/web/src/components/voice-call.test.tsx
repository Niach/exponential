import { fireEvent, render, renderHook, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/voice-call.json"
import { TooltipProvider } from "@exp/ui"

import { VoiceCall, type CallDeps } from "@/lib/voice-call/call"

// NEW-70 (voice-call.json): THE telephone in the web agent chat's header and
// THE call surface over it (S-3: phone-call style).

vi.mock(`@/lib/trpc-client`, () => ({ trpc: { voice: { get: { query: async () => null }, startCall: { mutate: vi.fn() } } } }))
vi.mock(`@/lib/collections`, () => ({ deviceCollection: {} }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))

const { CallButton, useVoiceCall, VoiceCallOverlay } = await import(`./voice-call`)

const testIds = fixture.testIds

describe(`CallButton`, () => {
  it(`is disabled with the reason, labelled Call {agent}`, () => {
    const reason = fixture.copy.reasons.noGpu.replace(`{device}`, `macbook`)
    const onCall = vi.fn()
    render(
      <TooltipProvider>
        <CallButton availability={{ enabled: false, reason }} agentName="Mira" active={false} onCall={onCall} />
      </TooltipProvider>
    )
    const button = screen.getByTestId(testIds.call)
    expect((button as HTMLButtonElement).disabled).toBe(true)
    expect(button.getAttribute(`aria-label`)).toBe(`Call Mira`)
    expect(button.getAttribute(`data-reason`)).toBe(reason)
    fireEvent.click(button)
    expect(onCall).not.toHaveBeenCalled()
  })

  it(`calls when available, and stays off while a call runs`, () => {
    const onCall = vi.fn()
    const { rerender } = render(
      <TooltipProvider>
        <CallButton availability={{ enabled: true, reason: null }} agentName="Mira" active={false} onCall={onCall} />
      </TooltipProvider>
    )
    fireEvent.click(screen.getByTestId(testIds.call))
    expect(onCall).toHaveBeenCalledOnce()
    rerender(
      <TooltipProvider>
        <CallButton availability={{ enabled: true, reason: null }} agentName="Mira" active onCall={onCall} />
      </TooltipProvider>
    )
    expect((screen.getByTestId(testIds.call) as HTMLButtonElement).disabled).toBe(true)
  })
})

describe(`VoiceCallOverlay`, () => {
  function connected() {
    const socket = {
      binaryType: `blob` as BinaryType,
      readyState: 1,
      onopen: null,
      onmessage: null as ((event: MessageEvent) => void) | null,
      onclose: null,
      onerror: null,
      sent: [] as unknown[],
      send(data: unknown) {
        socket.sent.push(data)
      },
      close() {},
    }
    const audio = { onPacket: null, onLevel: null, onError: null, setMuted: vi.fn(), play() {}, flush() {}, close() {} }
    const call = new VoiceCall({ startCall: async () => ({ url: `wss://r` }), openSocket: () => socket, openAudio: async () => audio })
    return { call, socket }
  }

  const overlay = (call: VoiceCall) => (
    <TooltipProvider>
      <VoiceCallOverlay call={call} snapshot={call.getSnapshot()} agentName="Assistant" avatar={<span>A</span>} />
    </TooltipProvider>
  )
  const hostEvent = (socket: { onmessage: ((event: MessageEvent) => void) | null }, event: Record<string, unknown>) =>
    socket.onmessage?.({ data: JSON.stringify({ t: `call_event`, event }) } as MessageEvent)

  it(`draws nothing without a call`, () => {
    const { call } = connected()
    render(overlay(call))
    expect(screen.queryByTestId(testIds.surface)).toBeNull()
  })

  it(`S-3 ringing: the name, Calling…, only the red end button labelled Cancel under it`, async () => {
    const { call } = connected()
    await call.start(`agent-1`, `mint`)
    render(overlay(call))
    const surface = screen.getByTestId(testIds.surface)
    expect(surface.textContent).toContain(`Assistant`)
    expect(surface.textContent).toContain(fixture.copy.calling)
    expect(screen.queryByTestId(testIds.mute)).toBeNull()
    expect(screen.queryByTestId(testIds.timer)).toBeNull()
    const end = screen.getByTestId(testIds.end)
    expect(end.getAttribute(`aria-label`)).toBe(fixture.copy.cancel)
    expect(end.getAttribute(`data-tone`)).toBe(`destructive`)
    expect(surface.textContent).toContain(fixture.copy.cancel)
  })

  it(`S-3 in call: the timer, the caption, round mute + end (no speaker on web), mute toggles`, async () => {
    const { call, socket } = connected()
    await call.start(`agent-1`, `mint`)
    socket.onmessage?.({ data: JSON.stringify({ t: `call_ready` }) } as MessageEvent)
    hostEvent(socket, { t: `rang` })
    hostEvent(socket, { t: `ready` })
    hostEvent(socket, { t: `replyFinished` })
    hostEvent(socket, { t: `partial`, text: `what is still open` })
    const { rerender } = render(overlay(call))
    const surface = screen.getByTestId(testIds.surface)
    expect(surface.textContent).toContain(`“what is still open”`)
    expect(screen.getByTestId(testIds.timer).textContent).toBe(`00:00`)
    expect(screen.queryByTestId(testIds.speaker)).toBeNull()
    expect(screen.getByTestId(testIds.end).getAttribute(`aria-label`)).toBe(fixture.copy.endCall)
    expect(screen.getByRole(`img`, { name: `Assistant, ${fixture.copy.listening}` })).toBeTruthy()
    fireEvent.click(screen.getByTestId(testIds.mute))
    rerender(overlay(call))
    const mute = screen.getByTestId(testIds.mute)
    expect(mute.getAttribute(`aria-label`)).toBe(fixture.copy.unmute)
    expect(mute.getAttribute(`aria-pressed`)).toBe(`true`)
    expect(screen.getByTestId(testIds.surface).textContent).toContain(fixture.copy.muted)
    expect(socket.sent.at(-1)).toBe(JSON.stringify({ t: `call_mute`, muted: true }))
    fireEvent.click(screen.getByTestId(testIds.end))
    rerender(overlay(call))
    expect(screen.queryByTestId(testIds.surface)).toBeNull()
  })
})

describe(`useVoiceCall`, () => {
  function deps() {
    const sockets: { sent: unknown[]; closed: boolean }[] = []
    const audios: { closed: boolean }[] = []
    const value: CallDeps = {
      startCall: async () => ({ url: `wss://r` }),
      openSocket: () => {
        const socket = {
          binaryType: `blob` as BinaryType,
          readyState: 1,
          onopen: null,
          onmessage: null,
          onclose: null,
          onerror: null,
          sent: [] as unknown[],
          closed: false,
          send(data: unknown) {
            socket.sent.push(data)
          },
          close() {
            socket.closed = true
          },
        }
        sockets.push(socket)
        return socket
      },
      openAudio: async () => {
        const audio = { onPacket: null, onLevel: null, onError: null, closed: false, setMuted() {}, play() {}, flush() {}, close() {
          audio.closed = true
        } }
        audios.push(audio)
        return audio
      },
    }
    return { value, sockets, audios }
  }

  async function running(agentId = `agent-1`) {
    const d = deps()
    const hook = renderHook(({ agentId }) => useVoiceCall(agentId, d.value), { initialProps: { agentId } })
    await hook.result.current.call.start(agentId, `mint`)
    expect(hook.result.current.call.active).toBe(true)
    return { ...d, hook }
  }

  it(`leaving the chat ends the call`, async () => {
    const { hook, sockets, audios } = await running()
    const { call } = hook.result.current
    hook.unmount()
    expect(call.active).toBe(false)
    expect(sockets[0]!.closed).toBe(true)
    expect(audios[0]!.closed).toBe(true)
  })

  it(`switching to another agent ends the call`, async () => {
    const { hook, audios } = await running()
    const first = hook.result.current.call
    hook.rerender({ agentId: `agent-2` })
    expect(first.active).toBe(false)
    expect(audios[0]!.closed).toBe(true)
    expect(hook.result.current.call).not.toBe(first)
  })

  it(`the page going away ends the call`, async () => {
    const { hook, sockets } = await running()
    window.dispatchEvent(new Event(`pagehide`))
    expect(hook.result.current.call.active).toBe(false)
    expect(sockets[0]!.sent).toContain(JSON.stringify({ t: `call_end` }))
  })
})
