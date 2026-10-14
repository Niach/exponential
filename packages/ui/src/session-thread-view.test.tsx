import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { ThreadConversation, ThreadTabs, ThreadsListItem } from "./session-thread-view"

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
    answer: null,
    answered: false,
  },
  { type: `plan`, id: `p1`, plan: `Step 1`, decision: null },
  { type: `event`, id: `e1`, event: `issue_linked`, text: `Linked APP-5` },
]

describe(`ThreadConversation`, () => {
  it(`renders every row kind and answers in the card`, () => {
    const onAnswer = vi.fn()
    const onDecide = vi.fn()
    const onOpenSession = vi.fn()
    render(
      <ThreadConversation
        rows={ROWS}
        renderText={(text) => <p>{text}</p>}
        messageCaption={(row) => (row.queued ? `queued` : null)}
        attachmentSrc={(id) => `/api/attachments/${id}`}
        onOpenSession={onOpenSession}
        onAnswer={onAnswer}
        onDecide={onDecide}
      />
    )
    expect(screen.getByText(`Do it`)).toBeTruthy()
    expect(screen.getByText(`queued`)).toBeTruthy()
    expect(screen.getByText(`On it`)).toBeTruthy()
    expect(screen.getByText(`Linked APP-5`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`session-card`))
    expect(onOpenSession).toHaveBeenCalledWith(`s1`)
    fireEvent.click(screen.getAllByTestId(`thread-question-option`)[1]!)
    expect(onAnswer).toHaveBeenCalledWith(`q1`, `B`)
    fireEvent.click(screen.getByTestId(`thread-plan-approve`))
    expect(onDecide).toHaveBeenCalledWith(`p1`, `approved`)
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
  it(`shows the lock instead of the word when private`, () => {
    const { rerender } = render(
      <ThreadsListItem lead={null} identifier="APP-5" title="T" word="merged" />
    )
    expect(screen.getByText(`merged`)).toBeTruthy()
    rerender(<ThreadsListItem lead={null} identifier="APP-5" title="T" word="merged" locked />)
    expect(screen.queryByText(`merged`)).toBeNull()
    expect(screen.getByLabelText(`Private`)).toBeTruthy()
  })
})
