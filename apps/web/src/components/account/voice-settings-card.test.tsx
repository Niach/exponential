import type { ReactElement } from "react"
import { act, fireEvent, render as rtlRender, screen } from "@testing-library/react"
import { TooltipProvider } from "@exp/ui"
import {
  clearOnNone,
  languageLabel,
  languageNote,
  trayHeader,
  VOICE_COPY,
  VOICE_DEFAULTS,
  VOICE_LIMITS,
  VOICE_TEST_IDS,
  wakePhrasePlaceholder,
} from "@exp/domain-contract/voice"
import { consentBody, MEETING_COPY, MEETING_TEST_IDS } from "@exp/domain-contract/meeting"
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
const startMeeting = vi.fn()
const stopMeeting = vi.fn()
const toastActionError = vi.fn()
vi.mock(`@/lib/action-error`, () => ({
  SKIP_ERROR_TOAST: { context: { skipErrorToast: true } },
  failedAction: (...args: unknown[]) => toastActionError(...args),
}))
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    voice: {
      update: { mutate: (...args: unknown[]) => update(...args) },
      startMeeting: { mutate: (...args: unknown[]) => startMeeting(...args) },
      stopMeeting: { mutate: (...args: unknown[]) => stopMeeting(...args) },
    },
  },
}))

import { VoiceSettingsCard, type VoiceSettings } from "@/components/account/voice-settings-card"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const AGENT = `11111111-1111-4111-8111-111111111111`
const DEVICES = [
  {
    deviceId: `d-mint`,
    name: `mint`,
    kind: `desktop`,
    desktopAudio: {
      capability: `per_app`,
      apps: [
        { id: `us.zoom.xos`, name: `Zoom` },
        { id: `com.tinyspeck.slackmacgap`, name: `Slack` },
      ],
    },
  },
  { deviceId: `d-phone`, name: `iPhone`, kind: `phone` },
]
const AGENTS = [{ id: AGENT, name: `Studio`, team: `Acme` }]

const render = (ui: ReactElement) => rtlRender(<TooltipProvider>{ui}</TooltipProvider>)
// What the server holds: the rendered settings, each update merged in the
// way `voice.update` stores it (`clearOnNone`).
const DEFAULTS: VoiceSettings = { ...VOICE_DEFAULTS, desktopAudio: `off`, desktopAudioApps: [], meetingStartedAt: null }
let stored: VoiceSettings = { ...DEFAULTS }
const renderCard = (settings: Partial<VoiceSettings> = {}) => {
  stored = { ...DEFAULTS, ...settings }
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
  startMeeting.mockReset()
  startMeeting.mockImplementation(async () => {
    stored = { ...stored, meetingStartedAt: `2026-10-28T10:00:00.000Z` }
    return stored
  })
  stopMeeting.mockReset()
  stopMeeting.mockImplementation(async () => {
    stored = { ...stored, meetingStartedAt: null }
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

  it(`#NEW-56: a too-short wake phrase is never sent; its line shows inline until the next edit`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    const phrase = screen.getByTestId(VOICE_TEST_IDS.wakePhrase)
    fireEvent.change(phrase, { target: { value: `Ok` } })
    fireEvent.blur(phrase)
    await flush()
    expect(update).not.toHaveBeenCalled()
    expect(screen.getByRole(`alert`).textContent).toBe(VOICE_COPY.wakePhraseTooShort)
    expect(phrase.getAttribute(`aria-invalid`)).toBe(`true`)
    fireEvent.change(phrase, { target: { value: `Okay computer` } })
    expect(screen.queryByRole(`alert`)).toBeNull()
  })

  it(`#NEW-73: Language follows Speak replies; the voice file + transcript wait for a voice device`, () => {
    renderCard()
    expect(trigger(VOICE_COPY.language).textContent).toContain(`English`)
    expect(screen.queryByTestId(VOICE_TEST_IDS.voiceFile)).toBeNull()
    expect(screen.queryByTestId(VOICE_TEST_IDS.voiceTranscript)).toBeNull()
  })

  it(`#NEW-73: with a voice device the file + transcript rows show their placeholders`, () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    const file = screen.getByTestId(VOICE_TEST_IDS.voiceFile) as HTMLInputElement
    const transcript = screen.getByTestId(VOICE_TEST_IDS.voiceTranscript) as HTMLInputElement
    expect(file.placeholder).toBe(VOICE_COPY.voiceFilePlaceholder)
    expect(transcript.placeholder).toBe(VOICE_COPY.voiceTranscriptPlaceholder)
  })

  it(`#NEW-73: a language pick sends {language}; German shows the device's note`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    fireEvent.click(trigger(VOICE_COPY.language))
    expect(await screen.findByTestId(VOICE_TEST_IDS.language)).toBeTruthy()
    fireEvent.click(await screen.findByText(languageLabel(`de`, `mint`)))
    await flush()
    expect(update).toHaveBeenCalledWith({ language: `de` }, expect.anything())
    expect(trigger(VOICE_COPY.language).textContent).toContain(languageLabel(`de`, `mint`))
  })

  it(`#NEW-73: German without a voice device has no note`, () => {
    renderCard({ language: `de` })
    expect(trigger(VOICE_COPY.language).textContent).toContain(`Deutsch`)
    expect(screen.queryByText(languageNote(`de`, `mint`)!, { exact: false })).toBeNull()
  })

  it(`#NEW-73: the voice file and transcript save on blur, trimmed`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    const file = screen.getByTestId(VOICE_TEST_IDS.voiceFile)
    fireEvent.change(file, { target: { value: ` /home/me/voice.wav ` } })
    fireEvent.blur(file)
    const transcript = screen.getByTestId(VOICE_TEST_IDS.voiceTranscript)
    fireEvent.change(transcript, { target: { value: ` Hello there. ` } })
    fireEvent.blur(transcript)
    await flush()
    expect(update).toHaveBeenCalledTimes(2)
    expect(update).toHaveBeenCalledWith({ voiceFile: `/home/me/voice.wav` }, expect.anything())
    expect(update).toHaveBeenCalledWith({ voiceTranscript: `Hello there.` }, expect.anything())
  })

  it(`#NEW-75/#FEED-160: THE input rule as you type: line breaks and tabs become a space, other controls drop, the cap counts UTF-16 units`, async () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    const transcript = screen.getByTestId(VOICE_TEST_IDS.voiceTranscript) as HTMLInputElement
    // A text input's value sanitizer strips CR/LF itself, so the tab and NUL carry the rule here.
    fireEvent.change(transcript, { target: { value: `Hello\t\tthere\u0000 friend` } })
    expect(transcript.value).toBe(`Hello there friend`)
    const phrase = screen.getByTestId(VOICE_TEST_IDS.wakePhrase) as HTMLInputElement
    fireEvent.change(phrase, { target: { value: `${`a`.repeat(VOICE_LIMITS.wakePhrase - 1)}\u{1F600}` } })
    expect(phrase.value).toBe(`a`.repeat(VOICE_LIMITS.wakePhrase - 1))
    fireEvent.blur(phrase)
    await flush()
    expect(update).toHaveBeenCalledWith({ wakePhrase: `a`.repeat(VOICE_LIMITS.wakePhrase - 1) }, expect.anything())
  })

  it(`#NEW-73: a refused voice file reverts and raises the failed-action toast`, async () => {
    update.mockRejectedValueOnce(new Error(`nope`))
    renderCard({ voiceDeviceId: `d-mint`, agentId: AGENT })
    const file = screen.getByTestId(VOICE_TEST_IDS.voiceFile) as HTMLInputElement
    fireEvent.change(file, { target: { value: `/x.wav` } })
    fireEvent.blur(file)
    await flush()
    expect(file.value).toBe(``)
    expect(toastActionError).toHaveBeenCalledWith(`save`, `the voice file`, expect.any(Error))
  })
})

describe(`Settings → Voice: meeting mode (NEW-57)`, () => {
  const live = { voiceDeviceId: `d-mint`, agentId: AGENT }

  it(`disabled while the agent is None; off without a voice device's tray group`, () => {
    renderCard({ voiceDeviceId: `d-mint`, agentId: null, meetingStartedAt: `2026-10-28T10:00:00.000Z` })
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(false)
    expect(switchOf(MEETING_TEST_IDS.meeting).disabled).toBe(true)
  })

  it(`ON opens THE consent prompt and writes nothing; Cancel leaves it off`, async () => {
    renderCard({ ...live, desktopAudio: `include`, desktopAudioApps: [{ id: `us.zoom.xos`, name: `Zoom` }] })
    fireEvent.click(switchOf(MEETING_TEST_IDS.meeting))
    await flush()
    expect(screen.getByText(MEETING_COPY.consentTitle)).toBeTruthy()
    expect(screen.getByText(consentBody(`You and Zoom`, `Studio`, 7))).toBeTruthy()
    expect(startMeeting).not.toHaveBeenCalled()
    expect(update).not.toHaveBeenCalled()
    fireEvent.click(screen.getByText(MEETING_COPY.cancel))
    await flush()
    expect(screen.queryByText(MEETING_COPY.consentTitle)).toBeNull()
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(false)
  })

  it(`Start = voice.startMeeting {consent: true}; the row reads who while on`, async () => {
    renderCard(live)
    fireEvent.click(switchOf(MEETING_TEST_IDS.meeting))
    await flush()
    fireEvent.click(screen.getByTestId(MEETING_TEST_IDS.consentStart))
    await flush()
    expect(startMeeting).toHaveBeenCalledWith({ consent: true }, expect.anything())
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(true)
    expect(screen.getByText(`You`)).toBeTruthy()
  })

  it(`OFF stops at once, no dialog; a refusal reverts with the toast`, async () => {
    renderCard({ ...live, meetingStartedAt: `2026-10-28T10:00:00.000Z` })
    fireEvent.click(switchOf(MEETING_TEST_IDS.meeting))
    await flush()
    expect(stopMeeting).toHaveBeenCalled()
    expect(screen.queryByText(MEETING_COPY.consentTitle)).toBeNull()
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(false)

    startMeeting.mockRejectedValueOnce(new Error(`nope`))
    fireEvent.click(switchOf(MEETING_TEST_IDS.meeting))
    await flush()
    fireEvent.click(screen.getByTestId(MEETING_TEST_IDS.consentStart))
    await flush()
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(false)
    expect(toastActionError).toHaveBeenCalledWith(`start`, `meeting mode`, expect.any(Error))
  })

  it(`choosing None for the agent stops the meeting (clearOnNone)`, async () => {
    renderCard({ ...live, meetingStartedAt: `2026-10-28T10:00:00.000Z` })
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(true)
    fireEvent.click(trigger(VOICE_COPY.agent))
    fireEvent.click(await screen.findByText(VOICE_COPY.agentNone))
    await flush()
    expect(isOn(MEETING_TEST_IDS.meeting)).toBe(false)
  })

  it(`Desktop audio over the device's capability; Pick apps shows the Apps row`, async () => {
    renderCard(live)
    expect(screen.queryByText(MEETING_COPY.apps)).toBeNull()
    fireEvent.click(trigger(MEETING_COPY.desktopAudio))
    expect(await screen.findByTestId(MEETING_TEST_IDS.desktopAudio)).toBeTruthy()
    fireEvent.click(await screen.findByText(`Pick apps`))
    await flush()
    expect(update).toHaveBeenCalledWith({ desktopAudio: `include` }, expect.anything())
    expect(trigger(MEETING_COPY.apps).textContent).toContain(MEETING_COPY.appsNone)
    fireEvent.click(trigger(MEETING_COPY.apps))
    fireEvent.click(await screen.findByText(`Zoom`))
    await flush()
    expect(update).toHaveBeenLastCalledWith(
      { desktopAudioApps: [{ id: `us.zoom.xos`, name: `Zoom` }] },
      expect.anything()
    )
  })

  it(`a stored Pick apps hides the Apps row on a device that can't capture per app`, () => {
    render(
      <VoiceSettingsCard
        settings={{ ...DEFAULTS, ...live, desktopAudio: `include`, desktopAudioApps: [{ id: `z`, name: `Zoom` }] }}
        devices={[{ deviceId: `d-mint`, name: `mint`, kind: `desktop`, desktopAudio: { capability: `system`, apps: [] } }]}
        agents={AGENTS}
      />
    )
    expect(screen.queryByText(MEETING_COPY.apps)).toBeNull()
  })

  it(`a device without the capability offers Off only`, async () => {
    render(
      <VoiceSettingsCard
        settings={{ ...DEFAULTS, ...live }}
        devices={[{ deviceId: `d-mint`, name: `mint`, kind: `desktop` }]}
        agents={AGENTS}
      />
    )
    fireEvent.click(trigger(MEETING_COPY.desktopAudio))
    await screen.findByTestId(MEETING_TEST_IDS.desktopAudio)
    expect(screen.queryByText(`Everything`)).toBeNull()
    expect(screen.queryByText(`Pick apps`)).toBeNull()
  })
})
