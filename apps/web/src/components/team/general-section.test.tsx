import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"
import type { Team } from "@/db/schema"
import { TeamGeneralSection } from "@/components/team/general-section"

// EXP-1275 (EXP-1259 D9): the team prompt autosaves `autosave.debounceMs`
// after the last keystroke and on blur; the caption reads Saving → Saved →
// Edited {ago} and there is no Unsaved state. A failed save is a toast with
// Retry (FEED-79).

const { autosave } = FEEDBACK_STATES

const mockState = vi.hoisted(() => ({
  getAgentPrompt: vi.fn(),
  update: vi.fn(),
  toastError: vi.fn(),
}))

vi.mock(`sonner`, async (importOriginal) => ({
  ...(await importOriginal<typeof import("sonner")>()),
  toast: { success: vi.fn(), error: mockState.toastError },
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    teams: {
      getAgentPrompt: { query: mockState.getAgentPrompt },
      update: { mutate: mockState.update },
    },
  },
}))

const team = { id: `team-1`, name: `Acme`, yoloMode: false } as Team

async function renderLoaded() {
  render(<TeamGeneralSection team={team} />)
  await act(async () => {})
  return screen.getByTestId(`team-agent-prompt`) as HTMLTextAreaElement
}

beforeEach(() => {
  vi.useFakeTimers({ shouldAdvanceTime: true })
  mockState.getAgentPrompt.mockResolvedValue({ agentPrompt: `old`, agentPromptUpdatedAt: null })
  mockState.update.mockReset().mockResolvedValue({})
  mockState.toastError.mockReset()
})

afterEach(() => {
  vi.useRealTimers()
})

describe(`team prompt autosave`, () => {
  it(`saves once, debounceMs after the last keystroke, then reads Saved`, async () => {
    const field = await renderLoaded()
    fireEvent.change(field, { target: { value: `new` } })
    await act(async () => {
      vi.advanceTimersByTime(autosave.debounceMs - 100)
    })
    fireEvent.change(field, { target: { value: `newer` } })
    await act(async () => {
      vi.advanceTimersByTime(autosave.debounceMs - 100)
    })
    expect(mockState.update).not.toHaveBeenCalled()
    await act(async () => {
      vi.advanceTimersByTime(200)
    })
    expect(mockState.update).toHaveBeenCalledTimes(1)
    expect(mockState.update.mock.calls[0]![0]).toEqual({ teamId: `team-1`, agentPrompt: `newer` })
    expect(screen.getByText(autosave.captions.saved)).toBeTruthy()
    expect(screen.queryByText(`Unsaved`)).toBeNull()
    await act(async () => {
      vi.advanceTimersByTime(autosave.savedHoldMs)
    })
    expect(screen.getByText(/^Edited /)).toBeTruthy()
  })

  it(`saves at once on blur`, async () => {
    const field = await renderLoaded()
    fireEvent.change(field, { target: { value: `blurred` } })
    await act(async () => {
      fireEvent.blur(field)
    })
    expect(mockState.update).toHaveBeenCalledTimes(1)
  })

  it(`toasts a failed save with Retry`, async () => {
    mockState.update.mockRejectedValueOnce(new Error(`Server said no`))
    const field = await renderLoaded()
    fireEvent.change(field, { target: { value: `broken` } })
    await act(async () => {
      fireEvent.blur(field)
    })
    expect(mockState.toastError).toHaveBeenCalledTimes(1)
    const [title, options] = mockState.toastError.mock.calls[0]!
    expect(title).toBe(`Couldn't save the team prompt`)
    expect(options.action.label).toBe(FEEDBACK_STATES.errors.retry)
  })
})
