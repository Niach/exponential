import type { ReactNode } from "react"
import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { buildSessionThread, type ThreadRow } from "@exp/domain-contract/session-tree"
import {
  suggestionKey,
  ThreadConversation,
  ThreadIssueTab,
  ThreadSessionTree,
  ThreadTabs,
  ThreadsListItem,
  ThreadDock,
  threadDockedIds,
  threadTreeRows,
  type ThreadStripIssue,
  type ThreadTreeRow,
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
    xref: null,
  },
  {
    type: `message`,
    id: `m2`,
    author: `agent`,
    authorId: null,
    body: `On it`,
    queued: false,
    authorSessionId: null,
    xref: null,
  },
  {
    type: `question`,
    id: `q1`,
    sessionId: `s2`,
    question: `Which one?`,
    options: [
      { label: `A`, description: `The first one`, attachmentId: null },
      { label: `B`, description: null, attachmentId: null },
    ],
    custom: true,
    multi: false,
    surface: null,
    answer: null,
    answers: null,
    surfaceData: null,
    answered: false,
  },
  {
    type: `plan`,
    id: `p1`,
    sessionId: `s1`,
    plan: `Step 1`,
    questions: [],
    surface: null,
    decision: null,
    fresh: false,
    answers: null,
    feedback: null,
    surfaceData: null,
  },
  { type: `event`, id: `e1`, event: `issue_linked`, text: `Linked APP-5`, sessionId: null },
]

/** N-12: the plan sheet's footer: the Yes button (its menu holds the fresh
 *  context), the reject button, the reject's send. */
const decideButton = (scope: ParentNode) =>
  scope.querySelector<HTMLButtonElement>(`[data-slot="plan-card-decide"][data-decision="approved"]`)!
const rejectButton = (scope: ParentNode) => scope.querySelector<HTMLButtonElement>(`[data-slot="plan-card-reject"]`)!
const rejectSend = (scope: ParentNode) =>
  scope.querySelector<HTMLButtonElement>(`[data-slot="plan-card-decide"][data-decision="rejected"]`)!
/** A Radix trigger opens on pointer down (never on click). */
function pressOpen(trigger: HTMLElement) {
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: `mouse` })
}
/** THE Dock with one card's sheet open (the pending plan or question). */
function renderSheet(rows: ThreadRow[], openId: string, over: Record<string, unknown> = {}) {
  const onAnswer = vi.fn()
  const onDecide = vi.fn()
  const renderText = vi.fn((text: string) => <p>{text}</p>)
  render(
    <TooltipProvider>
      <ThreadDock
        rows={rows}
        sessionName={() => null}
        renderText={renderText}
        attachmentSrc={(id) => `/files/${id}`}
        onAnswer={onAnswer}
        onDecide={onDecide}
        asks
        initialOpenId={openId}
        {...over}
      />
    </TooltipProvider>
  )
  return { onAnswer, onDecide, renderText, sheet: screen.getByTestId(`dock-sheet`) }
}

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
    const answer = within(question as HTMLElement).getByTestId(`thread-question-answer`)
    expect((answer as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(question.querySelectorAll(`[data-testid="plan-card-option"]`)[1]!)
    fireEvent.click(answer)
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `B`, answers: [`B`] })
    // FEED-140 (chats.json): a waiting plan decides INLINE too (the dock's
    // sheet holds the same decision).
    const plan = document.querySelector(`[data-slot="thread-plan"]`)!
    const approve = plan.querySelector(`[data-slot="plan-card-decide"][data-decision="approved"]`)!
    fireEvent.click(approve)
    expect(onDecide).toHaveBeenCalledWith(`p1`, expect.objectContaining({ decision: `approved` }))
  })

  it(`#EXP-1464: a Needs you card expands into its waiting card, answers there, then reads Answered`, () => {
    const waiting = ROWS.find((row) => row.type === `question`)!
    const needs = { ...CARD, sessionId: `s9`, kindWord: `Needs you`, state: `needs_you`, tone: `amber`, running: false }
    const WaitingCard = ({ children }: { sessionId: string; children: (row: never) => ReactNode }) => (
      <>{children({ ...waiting, sessionId: `s9` } as never)}</>
    )
    const { onAnswer, onOpenSession, rerender } = renderRows([{ type: `session`, id: `c9`, card: needs as never }], {
      rootId: `r`,
      WaitingCard,
    })
    const card = screen.getByTestId(`session-card`)
    fireEvent.click(card)
    expect(onOpenSession).not.toHaveBeenCalled()
    const expanded = screen.getByTestId(`thread-row-expanded`)
    const question = expanded.querySelector(`[data-slot="thread-question"]`) as HTMLElement
    expect(question.textContent).toContain(`Question`)
    fireEvent.click(within(question).getByTestId(`thread-open-chat`))
    expect(onOpenSession).toHaveBeenCalledWith(`s9`)
    fireEvent.click(question.querySelectorAll(`[data-testid="plan-card-option"]`)[0]!)
    fireEvent.click(within(question).getByTestId(`thread-question-answer`))
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `A`, answers: [`A`] })
    expect(screen.queryByTestId(`thread-row-expanded`)).toBeNull()
    // The session works again: the row reads the quiet Answered and opens.
    rerender(
      <TooltipProvider>
        <ThreadConversation
          rows={[{ type: `session`, id: `c9`, card: { ...needs, kindWord: `Working`, state: `working`, tone: `foreground` } as never }]}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onOpenSession={onOpenSession}
          onAnswer={onAnswer}
          rootId="r"
          WaitingCard={WaitingCard}
        />
      </TooltipProvider>
    )
    expect(screen.getByTestId(`session-card`).textContent).toContain(`Answered`)
  })

  it(`#EXP-1464: event and report lines about a session open it; a line about nothing stays plain`, () => {
    const onOpenXref = vi.fn()
    renderRows(
      [
        { type: `event`, id: `e1`, event: `session_unclaimed`, text: `Fix was not picked up`, sessionId: `s3` },
        { type: `event`, id: `e2`, event: `session_ended`, text: `Session ended`, sessionId: null },
        {
          type: `report`,
          id: `r1`,
          sessionId: `s4`,
          text: `Report from Ship`,
          summary: `Done`,
          xref: { kind: `report`, sessionId: `s4`, name: `Ship`, agentId: null },
        },
      ],
      { rootId: `r`, onOpenXref }
    )
    fireEvent.click(screen.getByTestId(`thread-event`))
    expect(onOpenXref).toHaveBeenCalledWith(`s3`)
    expect(screen.getByText(`Session ended`).closest(`button`)).toBeNull()
    fireEvent.click(screen.getByTestId(`thread-xref`))
    expect(onOpenXref).toHaveBeenCalledWith(`s4`)
  })

  it(`FEED-140: a docked waiting card stays inline; a superseded plan collapses`, () => {
    const plan = ROWS.find((row) => row.type === `plan`)!
    renderRows(
      [
        plan,
        { ...plan, id: `p-old`, decision: `superseded`, plan: `# Old plan body` } as ThreadRow,
      ],
      { docked: new Set([plan.id]) }
    )
    const cards = document.querySelectorAll(`[data-slot="thread-plan"]`)
    expect(cards).toHaveLength(2)
    const old = document.querySelector(`[data-decision="superseded"]`)!
    expect(old.textContent).toContain(`Replaced by a newer plan`)
    expect(old.textContent).not.toContain(`Old plan body`)
    expect(old.querySelector(`[data-slot="plan-card-decide"]`)).toBeNull()
  })

  it(`chats.json: dividers, a surface card and the phone bubbles`, () => {
    const rows: ThreadRow[] = [
      { type: `message`, id: `u`, author: `user`, authorId: `x`, body: `Hi`, queued: false, authorSessionId: null, xref: null },
      { type: `message`, id: `a`, author: `agent`, authorId: null, body: `Hello`, queued: false, authorSessionId: null, xref: null },
      { type: `divider`, id: `d`, kind: `compact`, at: `2026-10-23T18:20:00.000Z` },
      { type: `divider`, id: `n`, kind: `new`, at: `2026-10-23T18:30:00.000Z` },
      {
        type: `surface`,
        id: `s`,
        sessionId: `r`,
        title: `Launch path`,
        surface: { root: { id: `root`, component: `Text`, props: { text: `740 ms` } } },
      },
    ]
    renderRows(rows, { layout: `bubbles`, messageTime: () => `21:02` })
    const dividers = screen.getAllByTestId(`transcript-divider`)
    expect(dividers.map((el) => el.getAttribute(`data-kind`))).toEqual([`compact`, `new`])
    expect(dividers[0]!.textContent).toMatch(/^Conversation compacted · \d\d:\d\d$/)
    expect(screen.getByTestId(`surface-card`).textContent).toContain(`Launch path`)
    expect(screen.getByTestId(`chat-bubble`).textContent).toContain(`21:02`)
    expect(screen.getByTestId(`user-message-bubble`).textContent).toContain(`21:02`)
  })

  it(`N-12: the plan's sheet approves from its footer`, () => {
    const { onDecide, sheet } = renderSheet(ROWS, `p1`)
    expect(sheet.textContent).toContain(`Step 1`)
    fireEvent.click(decideButton(sheet))
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `approved` })
  })

  it(`shows a described option's description under its label, a plain one none`, () => {
    renderRows()
    const question = document.querySelector(`[data-slot="thread-question"]`)!
    const options = question.querySelectorAll(`[data-testid="plan-card-option"]`)
    expect(options).toHaveLength(2)
    expect(options[0]!.textContent).toContain(`The first one`)
    expect(options[0]!.querySelectorAll(`span > span`)).toHaveLength(2)
    expect(options[1]!.querySelectorAll(`span > span`)).toHaveLength(1)
  })

  it(`answers a question with free text (the custom row wins over a single pick)`, () => {
    const { onAnswer } = renderRows()
    const question = document.querySelector(`[data-slot="thread-question"]`) as HTMLElement
    fireEvent.click(question.querySelectorAll(`[data-testid="plan-card-option"]`)[0]!)
    fireEvent.change(within(question).getByTestId(`plan-card-custom`), { target: { value: `Neither, use C` } })
    fireEvent.click(within(question).getByTestId(`thread-question-answer`))
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `Neither, use C`, answers: [`Neither, use C`] })
  })

  it(`offers THE plan options in the sheet's footer: Yes, its menu's fresh context, No, keep planning`, () => {
    const { onDecide, sheet } = renderSheet(ROWS, `p1`)
    const footer = within(sheet).getByTestId(`dock-sheet-footer`)
    expect(footer.querySelectorAll(`[data-testid="plan-card-option"]`)).toHaveLength(0)
    expect(rejectButton(footer).textContent).toBe(`No, keep planning`)
    expect(decideButton(footer).textContent).toBe(`Yes`)
    pressOpen(within(footer).getByRole(`button`, { name: `Yes, and start with a fresh context` }))
    fireEvent.click(screen.getByRole(`menuitem`, { name: `Yes, and start with a fresh context` }))
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `approved`, fresh: true })
  })

  it(`the feedback field appears only once the reject button is picked`, () => {
    const { onDecide, sheet } = renderSheet(ROWS, `p1`)
    expect(within(sheet).queryByTestId(`plan-card-feedback`)).toBeNull()
    fireEvent.click(rejectButton(sheet))
    fireEvent.change(within(sheet).getByTestId(`plan-card-feedback`), { target: { value: `Smaller steps` } })
    expect(rejectSend(sheet).textContent).toBe(`No, keep planning`)
    fireEvent.click(rejectSend(sheet))
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `rejected`, feedback: `Smaller steps` })
  })

  it(`renders an answered card closed: the answer, no options`, () => {
    renderRows([
      {
        type: `question`,
        id: `q2`,
        sessionId: `s1`,
        question: `Which one?`,
        options: [
          { label: `A`, description: null, attachmentId: null },
          { label: `B`, description: null, attachmentId: null },
        ],
        custom: true,
        multi: false,
        surface: null,
        answer: `Use C`,
        answers: null,
        surfaceData: null,
        answered: true,
      },
      {
        type: `plan`,
        id: `p2`,
        sessionId: `s1`,
        plan: `Step 1`,
        questions: [],
        surface: null,
        decision: `approved`,
        fresh: true,
        answers: null,
        feedback: `Ship it`,
        surfaceData: null,
      },
    ])
    expect(screen.queryAllByTestId(`ask-option`)).toHaveLength(0)
    expect(screen.queryAllByTestId(`thread-question-answer`)).toHaveLength(0)
    expect(screen.queryAllByTestId(`plan-card-custom`)).toHaveLength(0)
    expect(screen.getByText(`Use C`)).toBeTruthy()
    expect(screen.getByText(`Yes, and start with a fresh context`)).toBeTruthy()
    expect(screen.getByTestId(`plan-card-feedback`).textContent).toBe(`Feedback: Ship it`)
  })

  it(`puts the X only on the viewer's own queued row, and its tooltip leaves with it`, async () => {
    const onUnqueue = vi.fn()
    const rows: ThreadRow[] = [
      { type: `message`, id: `m1`, author: `user`, authorId: `u1`, body: `Mine`, queued: true, authorSessionId: null, xref: null },
      { type: `message`, id: `m3`, author: `user`, authorId: `u2`, body: `Theirs`, queued: true, authorSessionId: null, xref: null },
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

const CONVERSATION = { label: `Conversation`, active: false, onSelect: () => {} }

const issue = (identifier: string): ThreadStripIssue => ({
  issueId: `id-${identifier}`,
  identifier,
  title: `Title of ${identifier}`,
  status: { icon: `circle-dashed` },
})

const session = (sessionId: string, name: string, parentId: string | null = null): ThreadTreeRow => ({
  sessionId,
  parentId,
  name,
  agent: `claude`,
  markState: `working`,
})

const openGroup = () => {
  act(() => {
    fireEvent.pointerDown(screen.getByTestId(fixture.testIds.group), {
      button: 0,
      ctrlKey: false,
      pointerType: `mouse`,
    })
  })
}

describe(`ThreadTabs`, () => {
  it(`draws Conversation icon-only on the pager, labelled; no Guide tab (S-3)`, () => {
    render(
      <ThreadTabs variant="pager" conversation={{ ...CONVERSATION, active: true }} issues={[issue(`APP-5`)]} />
    )
    const conversation = screen.getByTestId(`thread-tab-conversation`)
    expect(conversation.textContent).toBe(``)
    expect(conversation.getAttribute(`aria-label`)).toBe(`Conversation`)
    expect(conversation.querySelector(`svg`)).not.toBeNull()
    expect(screen.queryByTestId(`thread-tab-guide`)).toBeNull()
    // The pager's one issue chip = its identifier alone.
    expect(screen.getByTestId(`thread-tab-APP-5`).textContent).toBe(`APP-5`)
  })

  it(`keeps icon + label from md up`, () => {
    render(<ThreadTabs conversation={CONVERSATION} issues={[]} />)
    expect(screen.getByTestId(`thread-tab-conversation`).textContent).toBe(`Conversation`)
  })

  // #EXP-1433: every `stripModeCases` row picks the chip or the group tab.
  for (const entry of fixture.stripModeCases) {
    it(`stripModeCases: ${entry.name}`, () => {
      const issues = Array.from({ length: entry.input.issues }, (_, index) => issue(`APP-${index + 1}`))
      const sessions = Array.from({ length: entry.input.sessions }, (_, index) =>
        session(`s${index}`, `Session ${index}`)
      )
      render(<ThreadTabs conversation={CONVERSATION} issues={issues} sessions={sessions} />)
      expect(screen.getByTestId(`thread-tabs`).getAttribute(`data-mode`)).toBe(entry.expected.mode)
      const group = screen.queryByTestId(fixture.testIds.group)
      if (entry.expected.mode === `chips`) {
        expect(group).toBeNull()
        expect(screen.queryAllByRole(`tab`).filter((tab) => tab.dataset.testid?.startsWith(`thread-tab-APP`))).toHaveLength(
          entry.input.issues
        )
      } else {
        expect(group).not.toBeNull()
        expect(group!.querySelector(`[data-slot="thread-strip-group-count"]`)!.textContent).toBe(
          String(entry.expected.count)
        )
        expect(screen.queryByTestId(`thread-tab-APP-1`)).toBeNull()
      }
    })
  }

  it(`pins + and Conversation left, the one issue chip, Stories right`, () => {
    const onOpenIssue = vi.fn()
    render(
      <ThreadTabs
        conversation={{ ...CONVERSATION, active: true }}
        issues={[issue(`APP-5`)]}
        add={<span data-testid="add">+</span>}
        onOpenIssue={onOpenIssue}
        stories={{ label: `Stories`, active: false, dot: false, onSelect: () => {} }}
      />
    )
    const order = [
      screen.getByTestId(`add`),
      screen.getByTestId(`thread-tab-conversation`),
      screen.getByTestId(`thread-tab-APP-5`),
      screen.getByRole(`tab`, { name: /Stories/ }),
    ]
    for (let index = 1; index < order.length; index++) {
      expect(
        order[index - 1]!.compareDocumentPosition(order[index]!) & Node.DOCUMENT_POSITION_FOLLOWING
      ).toBeTruthy()
    }
    expect(screen.getByTestId(`thread-tab-APP-5`).textContent).toContain(`Title of APP-5`)
    fireEvent.click(screen.getByTestId(`thread-tab-APP-5`))
    expect(onOpenIssue).toHaveBeenCalledWith(`id-APP-5`)
  })

  it(`the group lists issues, a separator, then the whole session tree; a pick opens it`, () => {
    const onOpenIssue = vi.fn()
    const onOpenSession = vi.fn()
    render(
      <ThreadTabs
        conversation={CONVERSATION}
        issues={[issue(`APP-1`), issue(`APP-2`)]}
        sessions={[session(`a`, `Threads impl`), session(`b`, `Android lane`, `a`)]}
        onOpenIssue={onOpenIssue}
        onOpenSession={onOpenSession}
      />
    )
    const group = screen.getByTestId(fixture.testIds.group)
    expect(group.closest(`[data-slot="strip-chip"]`)!.getAttribute(`data-active`)).toBeNull()
    expect(group.textContent).toBe(`4`)
    openGroup()
    const rows = screen.getByTestId(`thread-strip-group-rows`)
    const issueRows = rows.querySelectorAll(`[data-value]`)
    expect([...issueRows].map((row) => row.getAttribute(`data-value`))).toEqual([`id-APP-1`, `id-APP-2`])
    const separator = rows.querySelector(`[role="separator"]`)!
    const tree = within(rows).getByTestId(`thread-session-tree`)
    expect(issueRows[1]!.compareDocumentPosition(separator) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(separator.compareDocumentPosition(tree) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
    expect(within(tree).getAllByTestId(`thread-session-tree-row`).map((row) => row.textContent)).toEqual([
      `Threads impl`,
      `Android lane`,
    ])
    // Nothing open: no row carries the check.
    expect(rows.querySelector(`[data-selected-glyph="check"]`)).toBeNull()
    fireEvent.click(within(tree).getAllByTestId(`thread-session-tree-item`)[1]!)
    expect(onOpenSession).toHaveBeenCalledWith(`b`)
    openGroup()
    fireEvent.click(screen.getByTestId(`thread-strip-group-rows`).querySelector(`[data-value="id-APP-2"]`)!)
    expect(onOpenIssue).toHaveBeenCalledWith(`id-APP-2`)
  })

  it(`reads the open item's label while active; its row carries the check`, () => {
    const issues = [issue(`APP-1`), issue(`APP-2`)]
    const sessions = [session(`a`, `Threads impl`), session(`b`, `Android lane`, `a`)]
    const view = render(
      <ThreadTabs conversation={CONVERSATION} issues={issues} sessions={sessions} open={{ kind: `issue`, id: `id-APP-2` }} />
    )
    let group = screen.getByTestId(fixture.testIds.group)
    expect(group.closest(`[data-slot="strip-chip"]`)!.getAttribute(`data-active`)).toBe(`true`)
    const label = group.querySelector<HTMLElement>(`[data-slot="thread-strip-group-label"]`)!
    expect(label.textContent).toBe(`APP-2`)
    expect(label.style.maxWidth).toBe(`${fixture.stripGroupMaxWidth}px`)
    expect(group.querySelector(`[data-slot="thread-strip-group-count"]`)).toBeNull()
    openGroup()
    const checked = screen.getByTestId(`thread-strip-group-rows`).querySelector(`[data-selected-glyph="check"]`)!
    expect(checked.closest(`[data-value]`)!.getAttribute(`data-value`)).toBe(`id-APP-2`)
    view.unmount()

    render(
      <ThreadTabs conversation={CONVERSATION} issues={issues} sessions={sessions} open={{ kind: `session`, id: `b` }} />
    )
    group = screen.getByTestId(fixture.testIds.group)
    expect(group.textContent).toBe(`Android lane`)
    openGroup()
    const items = screen.getAllByTestId(`thread-session-tree-item`)
    expect(items[1]!.getAttribute(`aria-checked`)).toBe(`true`)
    expect(items[1]!.querySelector(`[data-selected-glyph="check"]`)).not.toBeNull()
    expect(items[0]!.querySelector(`[data-selected-glyph="check"]`)).toBeNull()
  })

  it(`never draws a "+N", a nested chip menu or a session chip`, () => {
    render(
      <ThreadTabs
        conversation={CONVERSATION}
        issues={[issue(`APP-1`), issue(`APP-2`), issue(`APP-3`), issue(`APP-4`)]}
        sessions={[session(`a`, `One`)]}
      />
    )
    expect(document.querySelector(`[data-slot="strip-overflow"]`)).toBeNull()
    expect(document.querySelector(`[data-slot="strip-chip-chevron"]`)).toBeNull()
    expect(screen.queryByTestId(`thread-tab-session`)).toBeNull()
    expect(screen.getByTestId(fixture.testIds.group).textContent).toBe(`5`)
  })
})

describe(`ThreadIssueTab`, () => {
  it(`draws no glyph for an unsynced status`, () => {
    render(<ThreadIssueTab identifier="APP-1" title="Parent" status={null} active={false} data-testid="chip" />)
    expect(screen.getByTestId(`chip`).querySelector(`svg`)).toBeNull()
  })
})

describe(`ThreadsListItem`, () => {
  it(`shows the word and never a lock (EXP-1339)`, () => {
    render(<ThreadsListItem lead={null} title="T" word="stack" />)
    expect(screen.getByText(`stack`)).toBeTruthy()
    expect(screen.queryByLabelText(`Private`)).toBeNull()
  })
})

describe(`session tree`, () => {
  it(`lists every session depth-first with the open one washed`, () => {
    const rows = [
      { sessionId: `r`, parentId: null, name: `Session 1`, agent: `claude`, markState: `working` as const },
      { sessionId: `b`, parentId: `r`, name: `B`, agent: `claude`, markState: `done` as const },
      {
        sessionId: `a`,
        parentId: `r`,
        name: `A`,
        agent: `claude`,
        markState: `needs_you` as const,
        stateWord: `Needs you`,
        stateTone: `amber`,
      },
      { sessionId: `a1`, parentId: `a`, name: `A1`, agent: `claude`, markState: `idle` as const },
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
      { sessionId: `x`, parentId: `y`, name: `X`, agent: null, markState: `idle` as const },
      { sessionId: `y`, parentId: `x`, name: `Y`, agent: null, markState: `idle` as const },
      { sessionId: `y1`, parentId: `y`, name: `Y1`, agent: null, markState: `idle` as const },
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
    expect(word.className).toContain(`text-yellow-400`)
    expect(items[2]!.className).toContain(`bg-glass-active`)
    expect(items[1]!.className).not.toContain(`bg-glass-active`)
    fireEvent.click(items[3]!)
    expect(onOpen).toHaveBeenCalledWith(`a1`)
  })
})

// EXP-1318: the row ends on the ROOT's device glyph, after the word.
describe(`ThreadsListItem device`, () => {
  it(`trails the word, named for AT`, () => {
    render(
      <ThreadsListItem
        lead={null}
        title="T"
        word="stack"
        device={{ icon: () => <svg />, name: `Alex's MacBook Pro` }}
      />
    )
    const device = document.querySelector(`[data-slot="session-row-device"]`)!
    expect(device.getAttribute(`aria-label`)).toBe(`Alex's MacBook Pro`)
    const word = screen.getByText(`stack`)
    expect(word.compareDocumentPosition(device) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy()
  })
})

/** A Radix menu trigger opens on pointer down (never on click). */
function openMenu(trigger: HTMLElement) {
  fireEvent.pointerDown(trigger, { button: 0, ctrlKey: false, pointerType: `mouse` })
}

// EXP-1323: the fixture's dispatcher case (suggestions in every state, a
// report). NEW-8: no captions and no aside cards.
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
    // A started item IS the child's session card.
    fireEvent.click(within(card as HTMLElement).getByText(`Add width and height to cart thumbnails`))
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

  it(`a report is one xref line opening the child, the summary not repeated`, () => {
    const { onOpenSession } = renderCase(rowsOf(DISPATCHER))
    const report = document.querySelector(`[data-slot="thread-xref"][data-kind="report"]`)!
    expect(report.getAttribute(`aria-label`)).toBe(`Report from Lazy-load product images`)
    expect(screen.queryByText(`Lazy-loading is in; PR #12 is open.`)).toBeNull()
    fireEvent.click(report)
    expect(onOpenSession).toHaveBeenCalledWith(`c2`)
  })

  it(`Start, Start all and the ⋯ menu (Do it here, Dismiss) call back with the card and item`, () => {
    const { actions } = renderCase(rowsOf(MORE))
    fireEvent.click(screen.getByTestId(`thread-suggestions-start-all`))
    expect(actions.onStartAll).toHaveBeenCalledWith(`m4`)
    const starts = screen.getAllByTestId(`thread-suggestion-start`)
    expect(starts).toHaveLength(4)
    fireEvent.click(starts[0]!)
    expect(actions.onStart).toHaveBeenCalledWith(`m2`, `s1`)
    // N-12: Do it here and Dismiss live in the row's ⋯ menu.
    expect(screen.queryByTestId(`thread-suggestion-here`)).toBeNull()
    expect(screen.queryByTestId(`thread-suggestion-dismiss`)).toBeNull()
    openMenu(screen.getAllByTestId(`thread-suggestion-options`)[1]!)
    fireEvent.click(screen.getByRole(`menuitem`, { name: `Do it here` }))
    expect(actions.onHere).toHaveBeenCalledWith(`m4`, `s1`)
    openMenu(screen.getAllByTestId(`thread-suggestion-options`)[3]!)
    fireEvent.click(screen.getByRole(`menuitem`, { name: `Dismiss` }))
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
      (screen.getAllByTestId(`thread-suggestion-options`)[0] as HTMLButtonElement).disabled
    ).toBe(true)
    expect(
      (screen.getAllByTestId(`thread-suggestion-options`)[1] as HTMLButtonElement).disabled
    ).toBe(false)
  })

  it(`N-12: open suggestions are a DockCard whose sheet lists them with Start all in the footer`, () => {
    const actions = { onStart: vi.fn(), onStartAll: vi.fn(), onHere: vi.fn(), onDismiss: vi.fn(), onFile: vi.fn() }
    const rows = rowsOf(MORE)
    render(
      <TooltipProvider>
        <ThreadDock
          rows={rows}
          sessionName={() => null}
          renderText={(text) => <p>{text}</p>}
          onAnswer={vi.fn()}
          onDecide={vi.fn()}
          suggestions={actions}
          initialOpenId="m4"
        />
      </TooltipProvider>
    )
    expect(threadDockedIds(rows, () => null).has(`m4`)).toBe(true)
    const card = screen.getAllByTestId(`dock-card`).find((node) => node.dataset.id === `m4`)!
    expect(card.dataset.kind).toBe(`suggestions`)
    const sheet = screen.getByTestId(`dock-sheet`)
    expect(within(sheet).getByTestId(`thread-suggestions-sheet`)).toBeTruthy()
    fireEvent.click(within(sheet).getByTestId(`thread-suggestions-start-all`))
    expect(actions.onStartAll).toHaveBeenCalledWith(`m4`)
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

describe(`ThreadConversation EXP-1280 rich cards`, () => {
  const richCase = fixture.cases.find((c) => c.name.startsWith(`EXP-1280: a rich plan card`))!
  const rows = buildSessionThread(richCase.input as never).rows

  function renderRich(only: string[]) {
    const onAnswer = vi.fn()
    const onDecide = vi.fn()
    const renderText = vi.fn((text: string) => <p>{text}</p>)
    render(
      <TooltipProvider>
        <ThreadConversation
          rows={rows.filter((row) => only.includes(row.id))}
          renderText={renderText}
          attachmentSrc={(id) => `/files/${id}`}
          onAnswer={onAnswer}
          onDecide={onDecide}
        />
      </TooltipProvider>
    )
    return { onAnswer, onDecide, renderText }
  }

  it(`paints the markdown without a pending upload, the surface frame, then pages through the questions`, () => {
    const { renderText } = renderSheet(rows.filter((row) => row.id === `n1`), `n1`)
    expect(renderText.mock.calls[0]![0]).not.toContain(`upload:`)
    expect(renderText.mock.calls[0]![0]).toContain(`/api/attachments/att-1`)
    const surface = screen.getByTestId(`plan-card-surface`)
    expect(surface.textContent).toContain(`Preview`)
    expect(surface.dataset.disabled).toBeUndefined()
    // N-12: page 0 = the plan; two questions = one page each, the step in
    // the sheet's header, Back/Next in its footer.
    expect(screen.queryByTestId(`plan-card-step`)).toBeNull()
    expect(screen.queryByTestId(`plan-card-step-back`)).toBeNull()
    expect(screen.queryByTestId(`plan-card-decision`)).toBeNull()
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    expect(screen.getByTestId(`plan-card-step`).textContent).toBe(`Question 1 of 2`)
    expect(screen.getByTestId(`plan-card-step-back`)).toBeTruthy()
    expect(screen.queryByTestId(`plan-card-decision`)).toBeNull()
    let questions = screen.getAllByTestId(`plan-card-question`)
    expect(questions.map((q) => q.dataset.question)).toEqual([`layout`])
    expect(questions[0]!.textContent).toContain(`Choose one`)
    // Any picture = tiles for every option of that question (a missing one
    // keeps the frame); `custom: false` = no free-text row.
    const tiles = within(questions[0]!).getByTestId(`plan-card-tile`)
    expect(within(tiles).getAllByRole(`radio`)).toHaveLength(3)
    expect(tiles.querySelector(`img`)?.getAttribute(`src`)).toBe(`/files/att-2`)
    expect(within(questions[0]!).queryByTestId(`plan-card-custom`)).toBeNull()
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    expect(screen.getByTestId(`plan-card-step`).textContent).toBe(`Question 2 of 2`)
    expect(screen.queryByTestId(`plan-card-step-next`)).toBeNull()
    expect(screen.getByTestId(`plan-card-step-back`)).toBeTruthy()
    questions = screen.getAllByTestId(`plan-card-question`)
    expect(questions.map((q) => q.dataset.question)).toEqual([`platforms`])
    expect(questions[0]!.textContent).toContain(`Choose any`)
    // Text only = THE option rows; the default = a free-text row.
    expect(within(questions[0]!).getAllByTestId(`plan-card-option`)).toHaveLength(2)
    expect(within(questions[0]!).getByTestId(`plan-card-custom`)).toBeTruthy()
    // The LAST page carries the decision in the footer; Back returns.
    expect(screen.getByTestId(`plan-card-decision`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`plan-card-step-back`))
    expect(screen.getByTestId(`plan-card-step`).textContent).toBe(`Question 1 of 2`)
  })

  it(`approving waits on every question, rejecting never does`, () => {
    const { onDecide, sheet } = renderSheet(rows.filter((row) => row.id === `n1`), `n1`)
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    expect(decideButton(sheet).disabled).toBe(true)
    expect(screen.getByTestId(`plan-card-issue`).textContent).toBe(`Answer every question to approve`)
    fireEvent.click(rejectButton(sheet))
    expect(screen.queryByTestId(`plan-card-issue`)).toBeNull()
    fireEvent.click(rejectSend(sheet))
    expect(onDecide).toHaveBeenCalledWith(`n1`, { decision: `rejected`, surfaceData: { accent: `blue` } })
  })

  it(`multi picks and a tile pick approve with the answers and the surface data`, () => {
    const { onDecide, sheet } = renderSheet(rows.filter((row) => row.id === `n1`), `n1`)
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    fireEvent.click(screen.getByRole(`radio`, { name: `Two columns` }))
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    const question = screen.getByTestId(`plan-card-question`)
    const list = within(question).getAllByTestId(`plan-card-option`)
    fireEvent.click(list[1]!)
    fireEvent.click(list[0]!)
    expect(list[0]!.getAttribute(`aria-pressed`)).toBe(`true`)
    expect(screen.queryByTestId(`plan-card-issue`)).toBeNull()
    expect(decideButton(sheet).disabled).toBe(false)
    fireEvent.click(decideButton(sheet))
    expect(onDecide).toHaveBeenCalledWith(`n1`, {
      decision: `approved`,
      answers: { layout: [`Two columns`], platforms: [`iOS`, `Android`] },
      surfaceData: { accent: `blue` },
    })
  })

  it(`an answered multi-pick question is closed: the picks keep their ring, the rest inert`, () => {
    renderRich([`n2`])
    const card = screen.getByTestId(`plan-card`)
    expect(card.dataset.answered).toBe(`true`)
    const boxes = within(card).getAllByRole(`checkbox`) as HTMLButtonElement[]
    expect(boxes.map((b) => [b.getAttribute(`aria-label`), b.getAttribute(`aria-checked`), b.disabled])).toEqual([
      [`Web`, `true`, false],
      [`iOS`, `false`, true],
      [`Android`, `true`, false],
    ])
    expect(screen.queryByTestId(`thread-question-answer`)).toBeNull()
    expect(screen.queryByTestId(`plan-card-custom`)).toBeNull()
  })

  it(`an answered plan is closed: the picks, the decision label and the feedback`, () => {
    renderRich([`n3`])
    expect(screen.queryAllByTestId(`ask-option`)).toHaveLength(0)
    expect(screen.queryByTestId(`plan-card-decision`)).toBeNull()
    expect(screen.queryByTestId(`plan-card-step`)).toBeNull()
    const option = screen.getByTestId(`plan-card-option`)
    expect(option.dataset.picked).toBe(`true`)
    expect(screen.getByText(`Yes`, { selector: `[data-slot="ask-answered"] span` })).toBeTruthy()
    expect(screen.getByTestId(`plan-card-feedback`).textContent).toBe(`Feedback: Go.`)
    // A malformed surface never paints.
    expect(screen.queryByTestId(`plan-card-surface`)).toBeNull()
  })

  it(`a decided plan lists every question closed, no stepper; a single-question plan has no step chrome`, () => {
    const q = (id: string) => ({
      id,
      prompt: id,
      multi: false,
      custom: false,
      options: [{ label: `x`, description: null, attachmentId: null }],
    })
    const base = { type: `plan` as const, sessionId: `s1`, plan: `P`, surface: null, fresh: false, feedback: null, surfaceData: null }
    render(
      <ThreadConversation
        rows={[
          { ...base, id: `d1`, questions: [q(`a`), q(`b`)], decision: `approved` as const, answers: { a: [`x`], b: [`x`] } },
          { ...base, id: `d2`, questions: [q(`c`)], decision: null, answers: null },
        ]}
        renderText={(text) => <p>{text}</p>}
        attachmentSrc={(id) => id}
        onDecide={vi.fn()}
      />
    )
    const [closed, single] = screen.getAllByTestId(`plan-card`)
    expect(within(closed!).getAllByTestId(`plan-card-question`)).toHaveLength(2)
    expect(within(closed!).queryByTestId(`plan-card-step`)).toBeNull()
    expect(within(single!).getAllByTestId(`plan-card-question`)).toHaveLength(1)
    expect(within(single!).queryByTestId(`plan-card-step`)).toBeNull()
    expect(within(single!).queryByTestId(`plan-card-step-next`)).toBeNull()
    // N-12: a single question sits under the plan on the one page; the
    // sheet's footer is the decision.
    const { sheet } = renderSheet([{ ...base, id: `d3`, questions: [q(`c`)], decision: null, answers: null }], `d3`)
    expect(within(sheet).getAllByTestId(`plan-card-question`)).toHaveLength(1)
    expect(within(sheet).queryByTestId(`plan-card-step`)).toBeNull()
    expect(within(sheet).queryByTestId(`plan-card-step-next`)).toBeNull()
    expect(within(sheet).getByTestId(`plan-card-decision`)).toBeTruthy()
  })

  it(`a pending question with a surface answers with its data model`, () => {
    const onAnswer = vi.fn()
    const surface = { root: { id: `t`, component: `Text`, props: { text: `Size` } }, data: { size: `lg` } }
    render(
      <ThreadConversation
        rows={[
          {
            type: `question`,
            id: `q9`,
            sessionId: `s1`,
            question: `Which first?`,
            options: [
              { label: `Web`, description: null, attachmentId: null },
              { label: `iOS`, description: null, attachmentId: null },
            ],
            custom: false,
            multi: true,
            surface,
            answer: null,
            answers: null,
            surfaceData: null,
            answered: false,
          },
        ]}
        renderText={(text) => <p>{text}</p>}
        attachmentSrc={(id) => id}
        onAnswer={onAnswer}
      />
    )
    expect(screen.getByText(`Choose any`)).toBeTruthy()
    expect(screen.getByTestId(`plan-card-surface`).textContent).toContain(`Size`)
    const options = screen.getAllByTestId(`plan-card-option`)
    fireEvent.click(options[1]!)
    fireEvent.click(options[0]!)
    fireEvent.click(screen.getByTestId(`thread-question-answer`))
    expect(onAnswer).toHaveBeenCalledWith(`q9`, {
      answer: `Web, iOS`,
      answers: [`Web`, `iOS`],
      surfaceData: { size: `lg` },
    })
  })
})

// S-3: the fixture's inter-session links: another session's line heads its
// bubble with `Message from …`, a report IS its line, the view's origin
// opens the Conversation; a tap goes through `onOpenXref` (the back stack).
const XREF = fixture.cases.find((entry) => entry.name.startsWith(`S-3: inter-session links`))!

describe(`ThreadConversation S-3 xrefs`, () => {
  it(`paints the origin first, a message's link above its bubble, the report as its line`, () => {
    const view = buildSessionThread(XREF.input as unknown as Input)
    const onOpenXref = vi.fn()
    const onOpenSession = vi.fn()
    render(
      <TooltipProvider>
        <ThreadConversation
          rows={view.rows}
          origin={view.origin}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onOpenSession={onOpenSession}
          onOpenXref={onOpenXref}
          xrefAgent={(id) => (id === `a1` ? { icon: `bot`, color: `#8b5cf6` } : null)}
        />
      </TooltipProvider>
    )
    const links = screen.getAllByTestId(`thread-xref`)
    expect(links.map((link) => link.getAttribute(`data-kind`))).toEqual([
      `started`,
      `message`,
      `message`,
      `report`,
    ])
    expect(links[0]!.getAttribute(`aria-label`)).toBe(`Started by Big Brother`)
    // The agent's mark through the root's startedByAgentId; a child's own
    // tree (no agent) also reads through its root.
    expect(links[0]!.querySelector(`[data-slot="agent-avatar"]`)).not.toBeNull()
    const childLine = links[2]!
    expect(childLine.getAttribute(`aria-label`)).toBe(`Message from Batch 2`)
    expect(childLine.parentElement!.textContent).toContain(`Half the tests pass.`)
    expect(screen.queryByText(`Batch 2 is in; PR #12 is open.`)).toBeNull()
    fireEvent.click(links[3]!)
    expect(onOpenXref).toHaveBeenCalledWith(`c1`)
    expect(onOpenSession).not.toHaveBeenCalled()
  })
})
