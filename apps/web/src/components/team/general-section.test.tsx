import { act, fireEvent, render, screen } from "@testing-library/react"
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"
import { FEEDBACK_STATES } from "@exp/domain-contract/feedback-mark"
import type { Team } from "@/db/schema"
import { TeamGeneralSection } from "@/components/team/general-section"
import { _resetTeamPromptSaves } from "@/lib/team-prompt-saves"

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
  _resetTeamPromptSaves()
})

/** The next `teams.update` call waits until the returned settle runs. */
function holdNextSave() {
  let settle: (error?: Error) => void = () => {}
  mockState.update.mockImplementationOnce(
    () =>
      new Promise((resolve, reject) => {
        settle = (error) => (error ? reject(error) : resolve({}))
      })
  )
  return (error?: Error) => act(async () => settle(error))
}

async function typeAndBlur(field: HTMLElement, value: string) {
  fireEvent.change(field, { target: { value } })
  await act(async () => {
    fireEvent.blur(field)
  })
}

const savedTexts = () =>
  mockState.update.mock.calls.map(([input]) => input as { teamId: string; agentPrompt: string })

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

  // Review #8: leaving inside the debounce window still saves the edit.
  it(`flushes a dirty draft on unmount`, async () => {
    const view = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    const field = screen.getByTestId(`team-agent-prompt`)
    fireEvent.change(field, { target: { value: `typed then left` } })
    view.unmount()
    expect(mockState.update).toHaveBeenCalledTimes(1)
    expect(mockState.update.mock.calls[0]![0]).toEqual({
      teamId: `team-1`,
      agentPrompt: `typed then left`,
    })
  })

  it(`sends the draft to ITS team on a team switch`, async () => {
    const view = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    fireEvent.change(screen.getByTestId(`team-agent-prompt`), {
      target: { value: `for acme` },
    })
    view.rerender(<TeamGeneralSection team={{ ...team, id: `team-2` } as Team} />)
    await act(async () => {})
    expect(mockState.update).toHaveBeenCalledTimes(1)
    expect(mockState.update.mock.calls[0]![0]).toEqual({
      teamId: `team-1`,
      agentPrompt: `for acme`,
    })
  })

  it(`flushes nothing when clean, over the cap, or already saved`, async () => {
    const clean = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    clean.unmount()

    const over = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    fireEvent.change(screen.getByTestId(`team-agent-prompt`), {
      target: { value: `x`.repeat(20_000) },
    })
    over.unmount()

    const saved = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    const field = screen.getByTestId(`team-agent-prompt`)
    fireEvent.change(field, { target: { value: `saved` } })
    await act(async () => {
      fireEvent.blur(field)
    })
    expect(mockState.update).toHaveBeenCalledTimes(1)
    saved.unmount()
    expect(mockState.update).toHaveBeenCalledTimes(1)
  })

  // Final review #8/B7: ONE save in flight per team, the last edit wins.
  it(`queues edits behind an in-flight save and sends only the latest`, async () => {
    const settle = holdNextSave()
    const field = await renderLoaded()
    await typeAndBlur(field, `a`)
    await typeAndBlur(field, `ab`)
    await typeAndBlur(field, `abc`)
    expect(savedTexts()).toEqual([{ teamId: `team-1`, agentPrompt: `a` }])
    expect(screen.getByText(autosave.captions.saving)).toBeTruthy()
    await settle()
    expect(savedTexts()).toEqual([
      { teamId: `team-1`, agentPrompt: `a` },
      { teamId: `team-1`, agentPrompt: `abc` },
    ])
  })

  it(`the unmount flush waits for the in-flight save`, async () => {
    const settle = holdNextSave()
    const view = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    const field = screen.getByTestId(`team-agent-prompt`)
    await typeAndBlur(field, `first`)
    fireEvent.change(field, { target: { value: `first, then left` } })
    view.unmount()
    expect(mockState.update).toHaveBeenCalledTimes(1)
    await settle()
    expect(savedTexts().map((input) => input.agentPrompt)).toEqual([
      `first`,
      `first, then left`,
    ])
  })

  it(`Retry resends the CURRENT text, not the one that failed`, async () => {
    mockState.update.mockRejectedValueOnce(new Error(`Server said no`))
    const field = await renderLoaded()
    await typeAndBlur(field, `abc`)
    fireEvent.change(field, { target: { value: `abcd` } })
    const retry = mockState.toastError.mock.calls[0]![1].action.onClick
    await act(async () => retry())
    expect(savedTexts().map((input) => input.agentPrompt)).toEqual([`abc`, `abcd`])
    expect(screen.getByText(autosave.captions.saved)).toBeTruthy()
  })

  it(`Retry after a team switch stays bound to the team that failed`, async () => {
    mockState.update.mockRejectedValueOnce(new Error(`Server said no`))
    const view = render(<TeamGeneralSection team={team} />)
    await act(async () => {})
    await typeAndBlur(screen.getByTestId(`team-agent-prompt`), `for acme`)
    view.rerender(<TeamGeneralSection team={{ ...team, id: `team-2` } as Team} />)
    await act(async () => {})
    fireEvent.change(screen.getByTestId(`team-agent-prompt`), {
      target: { value: `for team two` },
    })
    const retry = mockState.toastError.mock.calls[0]![1].action.onClick
    await act(async () => retry())
    expect(savedTexts().at(-1)).toEqual({ teamId: `team-1`, agentPrompt: `for acme` })
  })
})
