import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/voice-call.json"
import { callView, CALL_IDLE, callStep } from "@exp/domain-contract/voice-call"

import { CallControls, VoiceCallPill } from "./call-controls"
import { TooltipProvider } from "./tooltip"
import { VoiceCallSurface } from "./voice-call-card"

// S-3 (voice-call.json PHONE-CALL UI): the round call buttons in callView's
// order, their labels as accessibility labels, the red end button (Cancel
// written under it while ringing), the pill's mute + end only.

const ids = fixture.testIds
const wrap = (node: React.ReactNode) => render(<TooltipProvider>{node}</TooltipProvider>)

describe(`CallControls`, () => {
  it(`draws the buttons in order with the fixture's labels; mute toggles pressed`, () => {
    const onMute = vi.fn()
    const { rerender } = wrap(<CallControls buttons={[`mute`, `speaker`, `end`]} muted={false} end={fixture.copy.endCall} onMute={onMute} />)
    const buttons = [...document.querySelectorAll(`[data-slot="call-button"]`)]
    expect(buttons.map((button) => button.getAttribute(`aria-label`))).toEqual([fixture.copy.mute, fixture.copy.speaker, fixture.copy.endCall])
    expect(buttons.map((button) => button.getAttribute(`data-tone`))).toEqual([`neutral`, `neutral`, `destructive`])
    expect(buttons.map((button) => button.getAttribute(`data-size`))).toEqual([`icon-lg`, `icon-lg`, `icon-lg`])
    expect(screen.getByTestId(ids.mute).getAttribute(`aria-pressed`)).toBe(`false`)
    fireEvent.click(screen.getByTestId(ids.mute))
    expect(onMute).toHaveBeenCalledOnce()
    rerender(
      <TooltipProvider>
        <CallControls buttons={[`mute`, `end`]} muted end={fixture.copy.endCall} />
      </TooltipProvider>
    )
    expect(screen.getByTestId(ids.mute).getAttribute(`aria-label`)).toBe(fixture.copy.unmute)
    expect(screen.getByTestId(ids.mute).getAttribute(`aria-pressed`)).toBe(`true`)
    expect(screen.queryByText(fixture.copy.endCall)).toBeNull()
  })

  it(`ringing writes Cancel under the red end button`, () => {
    wrap(<CallControls buttons={[`end`]} muted={false} end={fixture.copy.cancel} ringing />)
    expect(screen.getByTestId(ids.end).getAttribute(`aria-label`)).toBe(fixture.copy.cancel)
    expect(screen.getByText(fixture.copy.cancel)).toBeTruthy()
  })
})

describe(`VoiceCallSurface`, () => {
  it(`ringing: the status, no timer; answered: the timer and the caption`, () => {
    const ringing = callView(callStep(CALL_IDLE, { t: `start`, device: `mint` }), `web`)!
    const { rerender } = wrap(<VoiceCallSurface agent="Assistant" avatar={null} view={ringing} timer="00:00" />)
    expect(screen.getByTestId(ids.surface).textContent).toContain(fixture.copy.calling)
    expect(screen.queryByTestId(ids.timer)).toBeNull()
    const speaking = fixture.surfaceSpecimens.find((row) => row.name === `in call, speaking`)!
    rerender(
      <TooltipProvider>
        <VoiceCallSurface agent="Assistant" avatar={null} view={speaking as never} timer={speaking.timer!} />
      </TooltipProvider>
    )
    expect(screen.getByTestId(ids.timer).textContent).toBe(`04:12`)
    expect(screen.getByTestId(ids.surface).textContent).toContain(speaking.caption)
    expect(screen.getByTestId(ids.speaker)).toBeTruthy()
  })
})

describe(`VoiceCallPill`, () => {
  it(`is the fixture's fixed size and shows only mute and end`, () => {
    wrap(
      <VoiceCallPill agent="Assistant" avatar={null} status="04:12" activity="listening" muted={false} buttons={[`mute`, `speaker`, `end`]} end={fixture.copy.endCall} />
    )
    const pill = screen.getByTestId(ids.pill)
    expect(pill.style.width).toBe(`${fixture.pill.width}px`)
    expect(pill.style.height).toBe(`${fixture.pill.height}px`)
    expect(screen.queryByTestId(ids.speaker)).toBeNull()
    expect(screen.getByTestId(ids.mute).getAttribute(`data-size`)).toBe(`icon`)
    expect(screen.getByTestId(ids.timer).textContent).toBe(`04:12`)
  })
})
