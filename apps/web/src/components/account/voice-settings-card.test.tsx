import type { ReactElement } from "react"
import { act, fireEvent, render as rtlRender, screen } from "@testing-library/react"
import { TooltipProvider } from "@exp/ui"
import {
  clearOnNone,
  trayHeader,
  VOICE_COPY,
  VOICE_DEFAULTS,
  VOICE_TEST_IDS,
  wakePhrasePlaceholder,
} from "@exp/domain-contract/voice"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// NEW-67 (`voice.json`): Settings → Voice. The device + agent pickers (None
// first), the switches off and disabled while either is None, None clearing
// them (`clearOnNone`), and each change one `voice.update` partial.

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: {},
  agentCollection: {},
  teamCollection: {},
}))
vi.mock(`@/hooks/use-session`, () => ({ useSession: () => ({ data: null }) }))
const update = vi.fn()
const toastActionError = vi.fn()
vi.mock(`@/lib/action-error`, () => ({
  SKIP_ERROR_TOAST: { context: { skipErrorToast: true } },
  failedAction: (...args: unknown[]) => toastActionError(...args),
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: { voice: { update: { mutate: (...args: unknown[]) => update(...args) } } },
}))

import { VoiceSettingsCard, type VoiceSettings } from "@/components/account/voice-settings-card"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const AGENT = `11111111-1111-4111-8111-111111111111`
const DEVICES = [
  { deviceId: `d-mint`, name: `mint`, kind: `desktop` },
  { deviceId: `d-phone`, name: `iPhone`, kind: `phone` },
]
const AGENTS = [{ id: AGENT, name: `Studio`, team: `Acme` }]

const render = (ui: ReactElement) => rtlRender(<TooltipProvider>{ui}</TooltipProvider>)
// What the server holds: the rendered settings, each update merged in the
// way `voice.update` stores it (`clearOnNone`).
let stored: VoiceSettings = { ...VOICE_DEFAULTS }
const renderCard = (settings: Partial<VoiceSettings> = {}) => {
  stored = { ...VOICE_DEFAULTS, ...settings }
  return render(
    <VoiceSettingsCard
      settings={stored}
      devices={DEVICES}
      agents={AGENTS}
    />
  )
}

const switchOf = (id: string) => document.getElementById(id) as HTMLButtonElement
const isOn = (id: string) => switchOf(id).getAttribute(`aria-checked`) === `true`
// The picker's test id marks its open SURFACE; the row trigger is found by
// its leading label.
const trigger = (label: string) => screen.getByText(label).closest(`button`) as HTMLButtonElement
const flush = () => act(async () => {})

beforeEach(() => {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  update.mockReset()
  update.mockImplementation(async (change: Partial<VoiceSettings>) => {
    stored = clearOnNone(stored, change)
    return stored
  })
  toastActionError.mockReset()
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`Settings → Voice (NEW-67)`, () => {
  it(`renders the fixture's copy; the tray group waits for a voice device`, () => {
    renderCard()
    expect(screen.getByTestId(VOICE_TEST_IDS.page)).toBeTruthy()
    expect(screen.getByText(VOICE_COPY.voiceDevice)).toBeTruthy()
    expect(screen.getByText(VOICE_COPY.agent)).toBeTruthy()
    expect(screen.getByText(VOICE_COPY.speakReplies)).toBeTruthy()
    expect(screen.queryByText(VOICE_COPY.pushToTalk)).toBeNull()
    expect(screen.queryByText(trayHeader(`mint`))).toBeNull()
  })

  it(`shows the tray group under the device's name while a voice device is picked`, () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    expect(screen.getByText(trayHeader(`mint`))).toBeTruthy()
    expect(screen.getByText(VOICE_COPY.pushToTalk)).toBeTruthy()
    expect(screen.getByText(VOICE_COPY.shortcut)).toBeTruthy()
    expect(screen.getByText(VOICE_COPY.wakeWord)).toBeTruthy()
    const phrase = screen.getByTestId(VOICE_TEST_IDS.wakePhrase) as HTMLInputElement
    expect(phrase.placeholder).toBe(wakePhrasePlaceholder(`Studio`))
  })

  it(`None for the device or the agent: every switch reads off and is disabled`, () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: null, speakReplies: true, pushToTalk: true, wakeWord: true })
    for (const id of [VOICE_TEST_IDS.speakReplies, VOICE_TEST_IDS.pushToTalk, VOICE_TEST_IDS.wakeWord]) {
      expect(isOn(id)).toBe(false)
      expect(switchOf(id).disabled).toBe(true)
    }
  })

  it(`both picked: the switches are live and a toggle sends its one field`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    expect(switchOf(VOICE_TEST_IDS.speakReplies).disabled).toBe(false)
    fireEvent.click(switchOf(VOICE_TEST_IDS.speakReplies))
    await flush()
    expect(update).toHaveBeenCalledWith({ speakReplies: true }, expect.anything())
    expect(isOn(VOICE_TEST_IDS.speakReplies)).toBe(true)
  })

  it(`the device picker lists desktops only, None first; a pick sends voiceDeviceId`, async () => {
    renderCard({ agentId: AGENT })
    fireEvent.click(trigger(VOICE_COPY.voiceDevice))
    expect(await screen.findByTestId(VOICE_TEST_IDS.voiceDevice)).toBeTruthy()
    expect(await screen.findByText(VOICE_COPY.voiceDeviceNone)).toBeTruthy()
    expect(screen.queryByText(`iPhone`)).toBeNull()
    fireEvent.click(await screen.findByText(`mint`))
    await flush()
    expect(update).toHaveBeenCalledWith({ voiceDeviceId: `d-mint` }, expect.anything())
    expect(screen.getByText(trayHeader(`mint`))).toBeTruthy()
  })

  it(`choosing None for the agent clears the switches (clearOnNone)`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT, speakReplies: true, pushToTalk: true, wakeWord: true })
    expect(isOn(VOICE_TEST_IDS.speakReplies)).toBe(true)
    fireEvent.click(trigger(VOICE_COPY.agent))
    fireEvent.click(await screen.findByText(VOICE_COPY.agentNone))
    await flush()
    expect(update).toHaveBeenCalledWith({ agentId: null }, expect.anything())
    for (const id of [VOICE_TEST_IDS.speakReplies, VOICE_TEST_IDS.pushToTalk, VOICE_TEST_IDS.wakeWord]) {
      expect(isOn(id)).toBe(false)
      expect(switchOf(id).disabled).toBe(true)
    }
  })

  it(`a refused write reverts and raises the failed-action toast`, async () => {
    update.mockRejectedValueOnce(new Error(`That agent isn't in your teams`))
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    fireEvent.click(switchOf(VOICE_TEST_IDS.speakReplies))
    await flush()
    expect(isOn(VOICE_TEST_IDS.speakReplies)).toBe(false)
    expect(toastActionError).toHaveBeenCalledWith(`save`, `the voice settings`, expect.any(Error))
  })

  it(`the wake phrase saves on blur, trimmed`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    const phrase = screen.getByTestId(VOICE_TEST_IDS.wakePhrase)
    fireEvent.change(phrase, { target: { value: ` Hey Studio ` } })
    fireEvent.blur(phrase)
    await flush()
    expect(update).toHaveBeenCalledTimes(1)
    expect(update).toHaveBeenCalledWith({ wakePhrase: `Hey Studio` }, expect.anything())
  })
})
