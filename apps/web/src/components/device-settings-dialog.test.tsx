import type { ReactElement } from "react"
import { act, fireEvent, render as rtlRender, screen, within } from "@testing-library/react"
import { TooltipProvider } from "@exp/ui"
import { VOICE_COPY, VOICE_TEST_IDS } from "@exp/domain-contract/voice"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

// NEW-53 (`voice.json`): Device settings' Voice group — the Assistant picker
// over the member's agents, then Speak replies (off while no assistant);
// both ride the debounced `devices.setLaunchDefaults` save, `voice` on every
// save.

const ROW_ID = `row-1`
const AGENT_A = `11111111-1111-4111-8111-111111111111`
const AGENT_B = `22222222-2222-4222-8222-222222222222`

let deviceRow: Record<string, unknown> = {}
const teams = [
  { id: `t1`, name: `Acme` },
  { id: `t2`, name: `Side` },
]
const agents = [
  { id: AGENT_A, name: `Ops`, teamId: `t1` },
  { id: AGENT_B, name: `Scout`, teamId: `t2` },
]

vi.mock(`@exp/ui`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useIsMobile: () => false,
}))
vi.mock(`@/lib/collections`, () => ({
  deviceCollection: { tag: `devices` },
  teamCollection: { tag: `teams` },
  agentCollection: { tag: `agents` },
}))
vi.mock(`@tanstack/react-db`, async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  useLiveQuery: (fn: (query: unknown) => unknown) => {
    let tag: string | undefined
    const builder = {
      from: (source: Record<string, { tag: string }>) => {
        tag = Object.values(source)[0]?.tag
        const chain = { where: () => chain }
        return chain
      },
    }
    if (fn(builder) === undefined) return { data: undefined }
    if (tag === `devices`) return { data: [deviceRow] }
    if (tag === `teams`) return { data: teams }
    if (tag === `agents`) return { data: agents }
    return { data: [] }
  },
}))
const setLaunchDefaults = vi.fn()
vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    devices: {
      setLaunchDefaults: { mutate: (...args: unknown[]) => setLaunchDefaults(...args) },
    },
  },
}))

import { DeviceSettingsDialog } from "@/components/device-settings-dialog"

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

const render = (ui: ReactElement) => rtlRender(<TooltipProvider>{ui}</TooltipProvider>)

const device = {
  rowId: ROW_ID,
  deviceId: `mac`,
  deviceLabel: `Mac`,
  kind: `desktop`,
} as never

const renderDialog = () =>
  render(<DeviceSettingsDialog device={device} open onOpenChange={() => {}} />)

const speakSwitch = () =>
  document.getElementById(VOICE_TEST_IDS.speakReplies) as HTMLButtonElement

const assistantTrigger = () =>
  within(screen.getByTestId(VOICE_TEST_IDS.group)).getAllByRole(`button`)[0] as HTMLButtonElement

beforeEach(() => {
  globalThis.ResizeObserver ??= ResizeObserverStub as never
  Element.prototype.scrollIntoView ??= () => {}
  deviceRow = {
    id: ROW_ID,
    deviceId: `mac`,
    label: `Mac`,
    kind: `desktop`,
    icon: null,
    agents: [`claude`],
    unauthedAgents: [],
    sharedTeamIds: [],
    isDefault: false,
    lastSeenAt: new Date(),
    launchDefaults: null,
    launchDefaultsUpdatedAt: null,
    doctor: null,
  }
  setLaunchDefaults.mockReset()
  setLaunchDefaults.mockResolvedValue({ launchDefaultsUpdatedAt: null })
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`Device settings: Voice (NEW-53)`, () => {
  it(`renders the group with the fixture copy`, () => {
    renderDialog()
    expect(screen.getByText(VOICE_COPY.group)).toBeTruthy()
    const group = screen.getByTestId(VOICE_TEST_IDS.group)
    expect(within(group).getByText(VOICE_COPY.assistant)).toBeTruthy()
    expect(within(group).getByText(VOICE_COPY.speakReplies)).toBeTruthy()
  })

  it(`disables Speak replies while no assistant is picked`, () => {
    renderDialog()
    expect(speakSwitch().disabled).toBe(true)
  })

  it(`seeds from the row's voice`, () => {
    deviceRow.launchDefaults = {
      voice: { assistantAgentId: AGENT_B, speakReplies: true },
    }
    renderDialog()
    expect(speakSwitch().disabled).toBe(false)
    expect(speakSwitch().getAttribute(`aria-checked`)).toBe(`true`)
    expect(assistantTrigger().textContent).toContain(`Scout`)
  })

  it(`picking an assistant and toggling Speak replies saves voice`, async () => {
    renderDialog()
    fireEvent.click(assistantTrigger())
    fireEvent.click(await screen.findByText(`Ops`))
    expect(speakSwitch().disabled).toBe(false)
    vi.useFakeTimers()
    fireEvent.click(speakSwitch())
    await act(async () => {
      vi.advanceTimersByTime(1_100)
    })
    expect(setLaunchDefaults).toHaveBeenCalledTimes(1)
    const [payload] = setLaunchDefaults.mock.calls[0] as [
      { deviceId: string; launchDefaults: { voice: unknown } },
    ]
    expect(payload.deviceId).toBe(`mac`)
    expect(payload.launchDefaults.voice).toEqual({
      assistantAgentId: AGENT_A,
      speakReplies: true,
    })
  })

  it(`an unrelated save still sends the stored voice`, async () => {
    deviceRow.launchDefaults = {
      voice: { assistantAgentId: AGENT_A, speakReplies: false },
    }
    renderDialog()
    vi.useFakeTimers()
    fireEvent.click(speakSwitch())
    fireEvent.click(speakSwitch())
    await act(async () => {
      vi.advanceTimersByTime(1_100)
    })
    const [payload] = setLaunchDefaults.mock.calls[0] as [
      { launchDefaults: { voice: unknown } },
    ]
    expect(payload.launchDefaults.voice).toEqual({
      assistantAgentId: AGENT_A,
      speakReplies: false,
    })
  })
})
