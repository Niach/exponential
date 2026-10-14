import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { ThreadConversation, ThreadTabs, ThreadsListItem } from "./session-thread-view"
import { TooltipProvider } from "./tooltip"

// EXP-1293: the thread screen's parts render the contract's rows as given.
const CARD = {
  sessionId: `s1`,
  icon: `coding-running`,
  name: `Session 1`,
  kindWord: `Working`,
  state: `working`,
  tone: `foreground`,
  elapsed: `2m 04s`,
  running: true,
  testId: `session-card`,
}

const ROWS: ThreadRow[] = [
  { type: `session`, id: `s1`, card: CARD as never },
  { type: `message`, id: `m1`, author: `user`, authorId: `u1`, body: `Do it`, queued: true },
  { type: `message`, id: `m2`, author: `agent`, authorId: null, body: `On it`, queued: false },
  {
    type: `question`,
    id: `q1`,
    question: `Which one?`,
    options: [`A`, `B`],
    custom: true,
    answer: null,
    answered: false,
  },
  { type: `plan`, id: `p1`, plan: `Step 1`, decision: null },
  { type: `event`, id: `e1`, event: `issue_linked`, text: `Linked APP-5` },
]

describe(`ThreadConversation`, () => {
  function renderRows(rows: ThreadRow[] = ROWS, over: Record<string, unknown> = {}) {
    const onAnswer = vi.fn()
    const onDecide = vi.fn()
    const onOpenSession = vi.fn()
    const view = render(
      <TooltipProvider>
        <ThreadConversation
          rows={rows}
          renderText={(text) => <p>{text}</p>}
          messageCaption={(row) => (row.queued ? `queued` : null)}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onOpenSession={onOpenSession}
          onAnswer={onAnswer}
          onDecide={onDecide}
          {...over}
        />
      </TooltipProvider>
    )
    return { ...view, onAnswer, onDecide, onOpenSession }
  }

  it(`renders every row kind and answers in the card`, () => {
    const { onAnswer, onDecide, onOpenSession } = renderRows()
    expect(screen.getByText(`Do it`)).toBeTruthy()
    expect(screen.getByText(`queued`)).toBeTruthy()
    expect(screen.getByText(`On it`)).toBeTruthy()
    expect(screen.getByText(`Linked APP-5`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`session-card`))
    expect(onOpenSession).toHaveBeenCalledWith(`s1`)
    const question = document.querySelector(`[data-slot="thread-question"]`)!
    fireEvent.click(question.querySelectorAll(`[data-testid="ask-option"]`)[1]!)
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `B` })
    const plan = document.querySelector(`[data-slot="thread-plan"]`)!
    fireEvent.click(plan.querySelectorAll(`[data-testid="ask-option"]`)[0]!)
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `approved` })
  })

  it(`answers a question with free text (the session view's card)`, () => {
    const { onAnswer } = renderRows()
    const question = document.querySelector(`[data-slot="thread-question"]`)!
    const options = question.querySelectorAll(`[data-testid="ask-option"]`)
    expect(options).toHaveLength(3)
    fireEvent.click(options[2]!)
    const field = screen.getByTestId(`ask-inline-field`)
    fireEvent.change(field, { target: { value: `Neither, use C` } })
    fireEvent.keyDown(field, { key: `Enter` })
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `Neither, use C` })
  })

  it(`rejects a plan with a note`, () => {
    const { onDecide } = renderRows()
    const plan = document.querySelector(`[data-slot="thread-plan"]`)!
    fireEvent.click(plan.querySelectorAll(`[data-testid="ask-option"]`)[1]!)
    const field = screen.getByTestId(`ask-inline-field`)
    fireEvent.change(field, { target: { value: `Smaller steps` } })
    fireEvent.keyDown(field, { key: `Enter` })
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `rejected`, feedback: `Smaller steps` })
  })

  it(`renders an answered card closed: the answer, no options`, () => {
    renderRows([
      {
        type: `question`,
        id: `q2`,
        question: `Which one?`,
        options: [`A`, `B`],
        custom: true,
        answer: `Use C`,
        answered: true,
      },
      { type: `plan`, id: `p2`, plan: `Step 1`, decision: `approved` },
    ])
    expect(screen.queryAllByTestId(`ask-option`)).toHaveLength(0)
    expect(screen.getByText(`Use C`)).toBeTruthy()
    expect(screen.getByText(`Approved`)).toBeTruthy()
  })

  it(`puts the X only on the viewer's own queued row, and its tooltip leaves with it`, async () => {
    const onUnqueue = vi.fn()
    const rows: ThreadRow[] = [
      { type: `message`, id: `m1`, author: `user`, authorId: `u1`, body: `Mine`, queued: true },
      { type: `message`, id: `m3`, author: `user`, authorId: `u2`, body: `Theirs`, queued: true },
    ]
    const { rerender } = renderRows(rows, { viewerId: `u1`, onUnqueue })
    const buttons = screen.getAllByTestId(`thread-unqueue`)
    expect(buttons).toHaveLength(1)
    fireEvent.click(buttons[0]!)
    expect(onUnqueue).toHaveBeenCalledWith(`m1`)
    await act(async () => {
      fireEvent.focus(buttons[0]!)
    })
    expect(document.querySelector(`[data-slot="tooltip-content"]`)).not.toBeNull()
    // The device consumed it: the row flips, the button unmounts.
    rerender(
      <TooltipProvider>
        <ThreadConversation
          rows={[{ ...rows[0]!, queued: false } as ThreadRow, rows[1]!]}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          viewerId="u1"
          onUnqueue={onUnqueue}
        />
      </TooltipProvider>
    )
    expect(screen.queryByTestId(`thread-unqueue`)).toBeNull()
    expect(document.querySelector(`[data-slot="tooltip-content"]`)).toBeNull()
  })
})

describe(`ThreadTabs`, () => {
  it(`draws Conversation, the chips and the Guide dot`, () => {
    render(
      <ThreadTabs
        conversation={{ label: `Conversation`, active: true, onSelect: () => {} }}
        issues={[<span key="a">APP-5</span>]}
        guide={{ label: `Guide`, active: false, dot: true, onSelect: () => {} }}
      />
    )
    expect(screen.getByTestId(`thread-tabs`).textContent).toContain(`APP-5`)
    expect(screen.getByTestId(`thread-guide-dot`)).toBeTruthy()
  })
})

describe(`ThreadsListItem`, () => {
  it(`shows the word AND the lock when private, word first`, () => {
    const { rerender } = render(<ThreadsListItem lead={null} title="T" word="merged" />)
    expect(screen.getByText(`merged`)).toBeTruthy()
    expect(screen.queryByLabelText(`Private`)).toBeNull()
    rerender(<ThreadsListItem lead={null} title="T" word="merged" locked />)
    const word = screen.getByText(`merged`)
    const lock = screen.getByLabelText(`Private`)
    expect(word.compareDocumentPosition(lock) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })
})
