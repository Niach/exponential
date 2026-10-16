import { act, fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { buildSessionThread, type ThreadRow } from "@exp/domain-contract/session-tree"
import {
  suggestionKey,
  ThreadConversation,
  ThreadSessionTab,
  ThreadSessionTree,
  ThreadTabs,
  ThreadsListItem,
  threadTreeRows,
} from "./session-thread-view"
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
  {
    type: `message`,
    id: `m1`,
    author: `user`,
    authorId: `u1`,
    body: `Do it`,
    queued: true,
    authorSessionId: null,
  },
  {
    type: `message`,
    id: `m2`,
    author: `agent`,
    authorId: null,
    body: `On it`,
    queued: false,
    authorSessionId: null,
  },
  {
    type: `question`,
    id: `q1`,
    question: `Which one?`,
    options: [
      { label: `A`, description: `The first one` },
      { label: `B`, description: null },
    ],
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

  it(`shows a described option's description under its label, a plain one none`, () => {
    renderRows()
    const question = document.querySelector(`[data-slot="thread-question"]`)!
    const options = question.querySelectorAll(`[data-testid="ask-option"]`)
    expect(options[0]!.textContent).toContain(`The first one`)
    expect(options[0]!.querySelectorAll(`span > span`)).toHaveLength(2)
    expect(options[1]!.querySelectorAll(`span > span`)).toHaveLength(1)
    expect(options[2]!.textContent).toContain(`Type something.`)
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
        options: [
          { label: `A`, description: null },
          { label: `B`, description: null },
        ],
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
      { type: `message`, id: `m1`, author: `user`, authorId: `u1`, body: `Mine`, queued: true, authorSessionId: null },
      { type: `message`, id: `m3`, author: `user`, authorId: `u2`, body: `Theirs`, queued: true, authorSessionId: null },
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

// EXP-1316: the strip's session chips and the tree popover.
describe(`session chips`, () => {
  it(`sit after the "+" in their own group and open in place`, () => {
    const onSelect = vi.fn()
    render(
      <ThreadTabs
        conversation={{ label: `Conversation`, active: false, onSelect: () => {} }}
        issues={[]}
        add={<span data-testid="add">+</span>}
        sessions={
          <ThreadSessionTab
            agent="claude"
            markState="needs_you"
            name="Profile the launch path"
            active
            onSelect={onSelect}
          />
        }
        guide={{ label: `Guide`, active: false, dot: false, onSelect: () => {} }}
      />
    )
    const chip = screen.getByTestId(`thread-tab-session`)
    expect(chip.getAttribute(`aria-selected`)).toBe(`true`)
    expect(chip.className).toContain(`bg-glass-active`)
    expect(
      screen.getByTestId(`add`).compareDocumentPosition(chip) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
    fireEvent.click(chip)
    expect(onSelect).toHaveBeenCalled()
  })

  it(`the tree lists every session depth-first with the open one washed`, () => {
    const rows = [
      { sessionId: `r`, parentId: null, name: `Session 1`, agent: `claude`, markState: `working` as const },
      { sessionId: `b`, parentId: `r`, name: `B`, agent: `claude`, markState: `ended` as const },
      {
        sessionId: `a`,
        parentId: `r`,
        name: `A`,
        agent: `claude`,
        markState: `needs_you` as const,
        stateWord: `Needs you`,
        stateTone: `amber`,
      },
      { sessionId: `a1`, parentId: `a`, name: `A1`, agent: `claude`, markState: `waiting` as const },
    ]
    expect(threadTreeRows(rows).map((entry) => [entry.row.sessionId, entry.depth])).toEqual([
      [`r`, 0],
      [`b`, 1],
      [`a`, 1],
      [`a1`, 2],
    ])
    // A parent cycle reaches no root: its rows still list (promoted), once.
    const cyclic = [
      ...rows,
      { sessionId: `x`, parentId: `y`, name: `X`, agent: null, markState: `waiting` as const },
      { sessionId: `y`, parentId: `x`, name: `Y`, agent: null, markState: `waiting` as const },
      { sessionId: `y1`, parentId: `y`, name: `Y1`, agent: null, markState: `waiting` as const },
    ]
    expect(threadTreeRows(cyclic).map((entry) => [entry.row.sessionId, entry.depth])).toEqual([
      [`r`, 0],
      [`b`, 1],
      [`a`, 1],
      [`a1`, 2],
      [`x`, 0],
      [`y`, 1],
      [`y1`, 2],
    ])
    const onOpen = vi.fn()
    render(<ThreadSessionTree rows={rows} activeId="a" onOpen={onOpen} />)
    const items = screen.getAllByTestId(`thread-session-tree-row`)
    expect(items.map((item) => item.textContent)).toEqual([`Session 1`, `B`, `ANeeds you`, `A1`])
    const word = items[2]!.querySelector(`[data-slot="thread-session-tree-word"]`)!
    expect(word.className).toContain(`text-amber-400`)
    expect(items[2]!.className).toContain(`bg-glass-active`)
    expect(items[1]!.className).not.toContain(`bg-glass-active`)
    fireEvent.click(items[3]!)
    expect(onOpen).toHaveBeenCalledWith(`a1`)
  })
})

// EXP-1316 ×4: phones carry the session chips in the pager too (bordered,
// the Conversation tab's height), between the "+" and the Guide.
describe(`session chips on the pager`, () => {
  it(`render bordered between the + and the Guide`, () => {
    render(
      <ThreadTabs
        variant="pager"
        conversation={{ label: `Conversation`, active: true, onSelect: () => {} }}
        issues={[]}
        add={<span data-testid="add">+</span>}
        sessions={
          <ThreadSessionTab
            agent="claude"
            markState="working"
            name="Session 1"
            active={false}
            bordered
            onSelect={() => {}}
          />
        }
        guide={{ label: `Guide`, active: false, dot: false, onSelect: () => {} }}
      />
    )
    const chip = screen.getByTestId(`thread-tab-session`)
    expect(chip.className).toContain(`border-glass-stroke`)
    expect(chip.className).toContain(`h-8`)
    expect(
      screen.getByTestId(`add`).compareDocumentPosition(chip) & Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
    expect(
      chip.compareDocumentPosition(screen.getByTestId(`thread-tab-guide`)) &
        Node.DOCUMENT_POSITION_FOLLOWING
    ).toBeTruthy()
  })
})

// EXP-1316: ONE scrolling row — every tab a direct, non-shrinking child.
describe(`the strip row`, () => {
  it(`keeps Conversation, the issues, +, sessions and Guide in one scrolling row`, () => {
    render(
      <ThreadTabs
        variant="pager"
        conversation={{ label: `Conversation`, active: true, onSelect: () => {} }}
        issues={[<span key="i" data-testid="issue">APP-5</span>]}
        add={<span data-testid="add">+</span>}
        sessions={<span data-testid="sessions">S</span>}
        guide={{ label: `Guide`, active: false, dot: false, onSelect: () => {} }}
      />
    )
    const strip = screen.getByTestId(`thread-tabs`)
    expect(strip.className).toContain(`overflow-x-auto`)
    expect(strip.className).toContain(`[&>*]:shrink-0`)
    const order = [`thread-tab-conversation`, `issue`, `add`, `sessions`, `thread-tab-guide`]
    expect(order.map((id) => screen.getByTestId(id).parentElement)).toEqual(order.map(() => strip))
  })
})

// EXP-1318: the row ends on the ROOT's device glyph, after the word and lock.
describe(`ThreadsListItem device`, () => {
  it(`trails the word and the lock, named for AT`, () => {
    render(
      <ThreadsListItem
        lead={null}
        title="T"
        word="merged"
        locked
        device={{ icon: () => <svg />, name: `Alex's MacBook Pro` }}
      />
    )
    const device = document.querySelector(`[data-slot="session-row-device"]`)!
    expect(device.getAttribute(`aria-label`)).toBe(`Alex's MacBook Pro`)
    const lock = screen.getByLabelText(`Private`)
    expect(lock.compareDocumentPosition(device) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })
})

// EXP-1323: the fixture's dispatcher case (suggestions in every state, a
// report, a btw aside card with its caption).
type Input = Parameters<typeof buildSessionThread>[0]
const DISPATCHER = fixture.cases.find((entry) => entry.name.startsWith(`EXP-1323: a live`))!
const MORE = fixture.cases.find((entry) => entry.name.startsWith(`EXP-1323: an ended`))!
const rowsOf = (entry: typeof DISPATCHER) =>
  buildSessionThread(entry.input as unknown as Input).rows

describe(`ThreadConversation EXP-1323`, () => {
  function renderCase(rows: ThreadRow[], over: Record<string, unknown> = {}) {
    const actions = {
      onStart: vi.fn(),
      onStartAll: vi.fn(),
      onHere: vi.fn(),
      onDismiss: vi.fn(),
      onFile: vi.fn(),
      devices: [{ id: `dev-mac`, name: `Mac` }],
      models: [{ value: `opus`, label: `Opus` }],
    }
    const onOpenSession = vi.fn()
    const view = render(
      <TooltipProvider>
        <ThreadConversation
          rows={rows}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onOpenSession={onOpenSession}
          suggestions={actions}
          renderIssueRef={(_id, identifier) => <a data-testid="issue-ref">{identifier}</a>}
          tail={<div data-testid="live-tail" />}
          {...over}
        />
      </TooltipProvider>
    )
    return { ...view, actions, onOpenSession }
  }

  it(`paints every suggestion state, opens a started child, ends on the tail`, () => {
    const { onOpenSession } = renderCase(rowsOf(DISPATCHER))
    const card = document.querySelector(`[data-slot="thread-suggestions"]`)!
    expect(card.textContent).toContain(`6 suggested tasks · 3 started`)
    // A started item IS the child's session card, its caption under it.
    expect(screen.getByText(`Reading CartLineItem.tsx`)).toBeTruthy()
    fireEvent.click(screen.getByText(`Add width and height to cart thumbnails`))
    expect(onOpenSession).toHaveBeenCalledWith(`c1`)
    expect(card.textContent).toContain(`Sent here`)
    expect(screen.getByTestId(`issue-ref`).textContent).toBe(`EXP-9`)
    expect(card.textContent).toContain(`Filed EXP-9`)
    // ONE open item: its buttons, no Start all.
    expect(screen.queryByTestId(`thread-suggestions-start-all`)).toBeNull()
    expect(screen.getAllByTestId(`thread-suggestion-start`)).toHaveLength(1)
    const conversation = screen.getByTestId(`thread-conversation`)
    expect(conversation.lastElementChild?.getAttribute(`data-testid`)).toBe(`live-tail`)
  })

  it(`a report is one line opening the child, the summary not repeated`, () => {
    const { onOpenSession } = renderCase(rowsOf(DISPATCHER))
    const report = document.querySelector(`[data-slot="thread-report"]`)!
    expect(report.textContent).toBe(`Report from Lazy-load product images`)
    expect(screen.queryByText(`Lazy-loading is in; PR #12 is open.`)).toBeNull()
    fireEvent.click(report)
    expect(onOpenSession).toHaveBeenCalledWith(`c2`)
  })

  it(`the aside's card keeps its answer as the caption`, () => {
    renderCase(rowsOf(DISPATCHER))
    expect(screen.getByText(`It returns the cart store's items and totals.`)).toBeTruthy()
  })

  it(`Start, Do it here, × and Start all call back with the card and item`, () => {
    const { actions } = renderCase(rowsOf(MORE))
    fireEvent.click(screen.getByTestId(`thread-suggestions-start-all`))
    expect(actions.onStartAll).toHaveBeenCalledWith(`m4`)
    const starts = screen.getAllByTestId(`thread-suggestion-start`)
    expect(starts).toHaveLength(4)
    fireEvent.click(starts[0]!)
    expect(actions.onStart).toHaveBeenCalledWith(`m2`, `s1`)
    fireEvent.click(screen.getAllByTestId(`thread-suggestion-here`)[1]!)
    expect(actions.onHere).toHaveBeenCalledWith(`m4`, `s1`)
    fireEvent.click(screen.getAllByTestId(`thread-suggestion-dismiss`)[3]!)
    expect(actions.onDismiss).toHaveBeenCalledWith(`m4`, `s3`)
  })

  it(`a pending item locks its buttons; read-only cards draw none`, () => {
    renderCase(rowsOf(MORE), {
      suggestions: {
        onStart: vi.fn(),
        onStartAll: vi.fn(),
        onHere: vi.fn(),
        onDismiss: vi.fn(),
        onFile: vi.fn(),
        pending: new Set([suggestionKey(`m2`, `s1`)]),
      },
    })
    expect(
      (screen.getAllByTestId(`thread-suggestion-here`)[0] as HTMLButtonElement).disabled
    ).toBe(true)
    expect(
      (screen.getAllByTestId(`thread-suggestion-here`)[1] as HTMLButtonElement).disabled
    ).toBe(false)
  })

  it(`read-only: no buttons`, () => {
    renderCase(rowsOf(MORE), { suggestions: undefined })
    expect(screen.queryAllByTestId(`thread-suggestion-start`)).toHaveLength(0)
    expect(screen.queryByTestId(`thread-suggestions-start-all`)).toBeNull()
  })
})

describe(`ThreadsListItem EXP-1323`, () => {
  it(`bolds an unread title and carries the caption`, () => {
    render(
      <ThreadsListItem lead={null} title="Speed up the shop" unread caption="Dispatching" />
    )
    const title = screen.getByText(`Speed up the shop`)
    expect(title.getAttribute(`data-unread`)).toBe(`true`)
    expect(title.className).toContain(`font-semibold`)
    expect(screen.getByText(`Dispatching`)).toBeTruthy()
  })
  it(`a read row is plain`, () => {
    render(<ThreadsListItem lead={null} title="Done" />)
    expect(screen.getByText(`Done`).getAttribute(`data-unread`)).toBeNull()
  })
})
