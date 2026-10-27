import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/voice-call.json"
import { TooltipProvider } from "@exp/ui"

import { VoiceCall } from "@/lib/voice-call/call"

// NEW-70 (voice-call.json): THE telephone in the web agent chat's header and
// THE call card over it.

vi.mock(`@/lib/trpc-client`, () => ({ trpc: { voice: { get: { query: async () => null }, startCall: { mutate: vi.fn() } } } }))
vi.mock(`@/lib/collections`, () => ({ deviceCollection: {} }))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))

const { CallButton, VoiceCallOverlay } = await import(`./voice-call`)

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
    const audio = { onPacket: null, onLevel: null, setMuted: vi.fn(), play() {}, flush() {}, close() {} }
    const call = new VoiceCall({ startCall: async () => ({ url: `wss://r` }), openSocket: () => socket, openAudio: async () => audio })
    return { call, socket }
  }

  it(`draws nothing without a call`, () => {
    const { call } = connected()
    render(<VoiceCallOverlay call={call} snapshot={call.getSnapshot()} />)
    expect(screen.queryByTestId(testIds.surface)).toBeNull()
  })

  it(`draws the card: the state, the line, End call primary and Mute`, async () => {
    const { call, socket } = connected()
    await call.start(`agent-1`, `mint`)
    socket.onmessage?.({ data: JSON.stringify({ t: `call_ready` }) } as MessageEvent)
    socket.onmessage?.({ data: JSON.stringify({ t: `call_event`, event: { t: `partial`, text: `what is still open` } }) } as MessageEvent)
    const { rerender } = render(<VoiceCallOverlay call={call} snapshot={call.getSnapshot()} />)
    const card = screen.getByTestId(testIds.surface)
    expect(card.textContent).toContain(fixture.copy.listening)
    expect(card.textContent).toContain(`“what is still open”`)
    expect(screen.getByTestId(testIds.end).textContent).toContain(fixture.copy.endCall)
    fireEvent.click(screen.getByTestId(testIds.mute))
    rerender(<VoiceCallOverlay call={call} snapshot={call.getSnapshot()} />)
    expect(screen.getByTestId(testIds.mute).textContent).toContain(fixture.copy.unmute)
    expect(screen.getByTestId(testIds.surface).textContent).toContain(fixture.copy.muted)
    expect(socket.sent.at(-1)).toBe(JSON.stringify({ t: `call_mute`, muted: true }))
    fireEvent.click(screen.getByTestId(testIds.end))
    rerender(<VoiceCallOverlay call={call} snapshot={call.getSnapshot()} />)
    expect(screen.queryByTestId(testIds.surface)).toBeNull()
  })
})
