import type { ReactNode } from "react"
import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/session-chat.json"
import { buildSessionChat, type SessionChatRow } from "@exp/domain-contract/session-tree"
import {
  suggestionKey,
  PlanRail,
  ChatConversation,
  ChatSessionTree,
  ChatsListItem,
  ChatDock,
  chatDockedIds,
  chatTreeRows,
} from "./session-chat-view"
import { TooltipProvider } from "./tooltip"

// EXP-1293: the chat screen's parts render the contract's rows as given.
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

const ROWS: SessionChatRow[] = [
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
function renderSheet(rows: SessionChatRow[], openId: string, over: Record<string, unknown> = {}) {
  const onAnswer = vi.fn()
  const onDecide = vi.fn()
  const renderText = vi.fn((text: string) => <p>{text}</p>)
  render(
    <TooltipProvider>
      <ChatDock
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

describe(`ChatConversation`, () => {
  function renderRows(rows: SessionChatRow[] = ROWS, over: Record<string, unknown> = {}) {
    const onAnswer = vi.fn()
    const onDecide = vi.fn()
    const onOpenSession = vi.fn()
    const view = render(
      <TooltipProvider>
        <ChatConversation
          rows={rows}
          renderText={(text) => <p>{text}</p>}
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

  it(`#EXP-1736: one bubble everywhere: the viewer's own right with its time, no name; others under an avatar + name, grouped`, () => {
    const msg = (id: string, author: `user` | `agent`, authorId: string | null, extra: Partial<SessionChatRow> = {}) =>
      ({
        type: `message`,
        id,
        author,
        authorId,
        body: `text ${id}`,
        queued: false,
        authorSessionId: null,
        xref: null,
        ...extra,
      }) as SessionChatRow
    const onOpenXref = vi.fn()
    const view = renderRows(
      [
        msg(`a`, `user`, `me`),
        msg(`b`, `user`, `dana`),
        msg(`c`, `agent`, null),
        msg(`d`, `agent`, null),
        msg(`e`, `agent`, null, {
          authorSessionId: `child`,
          xref: { kind: `message`, sessionId: `child`, name: `Fix it`, agentId: null },
        } as Partial<SessionChatRow>),
      ],
      {
        viewerId: `me`,
        chat: { sessionId: `root`, title: `Cold start`, agentId: null },
        member: (id: string) => (id === `dana` ? { id, name: `Dana Reyes` } : null),
        messageTime: () => `09:02`,
        onOpenXref,
      }
    )
    const own = view.getByTestId(`user-message-bubble`)
    expect(own.querySelector(`[data-slot="user-message-time"]`)!.textContent).toBe(`09:02`)
    expect(own.querySelector(`[data-slot="message-bubble-author"]`)).toBeNull()
    const names = [...document.querySelectorAll(`[data-slot="message-bubble-author"]`)].map((el) => el.textContent)
    expect(names).toEqual([`Dana Reyes`, `Cold start`])
    // #EXP-1772: a relay names its session ABOVE the bubble, only the name opens it.
    const relay = document.querySelector(`[data-slot="message-bubble-relay"]`)!
    expect(relay.textContent).toBe(`Fix it ›`)
    expect(relay.closest(`[data-slot="message-bubble-link"]`)).toBeNull()
    expect(document.querySelector(`[data-slot="message-bubble-link"]`)).toBeNull()
    const rows = [...document.querySelectorAll(`[data-slot="transcript-row"]`)] as HTMLElement[]
    expect(rows.map((row) => row.dataset.side)).toEqual([`own`, `other`, `other`, `other`, `other`])
    expect(rows.map((row) => row.style.paddingTop)).toEqual([``, `12px`, `12px`, `3px`, `12px`])
    fireEvent.click(view.getByRole(`button`, { name: `Fix it ›` }))
    expect(onOpenXref).toHaveBeenCalledWith(`child`)
  })

  it(`renders every row kind and answers in the card`, () => {
    const { onAnswer, onOpenSession } = renderRows()
    expect(screen.getByText(`Do it`)).toBeTruthy()
    // #FEED-237: no `queued` caption; the viewer's own row wears its mark.
    expect(screen.queryByText(`queued`)).toBeNull()
    expect(screen.getByText(`On it`)).toBeTruthy()
    expect(screen.getByText(`Linked APP-5`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`session-card`))
    expect(onOpenSession).toHaveBeenCalledWith(`s1`)
    const question = document.querySelector(`[data-slot="chat-question"]`)!
    const answer = within(question as HTMLElement).getByTestId(`chat-question-answer`)
    expect((answer as HTMLButtonElement).disabled).toBe(true)
    fireEvent.click(question.querySelectorAll(`[data-testid="plan-card-option"]`)[1]!)
    fireEvent.click(answer)
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `B`, answers: [`B`] })
    // #EXP-1662 (plan-page.json): a plan in the Conversation is ONE row.
    const plan = document.querySelector(`[data-slot="chat-plan"]`)!
    expect(plan.querySelector(`[data-slot="plan-card-decide"]`)).toBeNull()
    expect(plan.querySelector(`[data-testid="plan-card-option"]`)).toBeNull()
    expect(plan.textContent).toContain(`Plan: Step 1`)
    expect(plan.textContent).toContain(`Needs you`)
  })

  it(`#EXP-1662: a plan row opens the plan tab on that plan`, () => {
    const onOpenPlan = vi.fn()
    const plan = ROWS.find((row) => row.type === `plan`)!
    renderRows([plan], { onOpenPlan })
    fireEvent.click(screen.getByTestId(`plan-row`))
    expect(onOpenPlan).toHaveBeenCalledWith(plan)
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
    const expanded = screen.getByTestId(`chat-row-expanded`)
    const question = expanded.querySelector(`[data-slot="chat-question"]`) as HTMLElement
    expect(question.textContent).toContain(`Question`)
    fireEvent.click(within(question).getByTestId(`chat-row-open`))
    expect(onOpenSession).toHaveBeenCalledWith(`s9`)
    fireEvent.click(question.querySelectorAll(`[data-testid="plan-card-option"]`)[0]!)
    fireEvent.click(within(question).getByTestId(`chat-question-answer`))
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `A`, answers: [`A`] })
    expect(screen.queryByTestId(`chat-row-expanded`)).toBeNull()
    // The session works again: the row reads the quiet Answered and opens.
    rerender(
      <TooltipProvider>
        <ChatConversation
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
    // #EXP-1772: every event is THE inline marker.
    expect(screen.getAllByTestId(`transcript-marker`)).toHaveLength(2)
    fireEvent.click(within(screen.getByTestId(`chat-event`)).getByRole(`button`))
    expect(onOpenXref).toHaveBeenCalledWith(`s3`)
    expect(screen.getByText(`Session ended`).closest(`button`)).toBeNull()
    // #EXP-1772: a report is its child's bubble; its name above opens it.
    fireEvent.click(screen.getByRole(`button`, { name: `Ship ›` }))
    expect(onOpenXref).toHaveBeenCalledWith(`s4`)
  })

  it(`FEED-140: a docked waiting card stays inline; a superseded plan collapses`, () => {
    const plan = ROWS.find((row) => row.type === `plan`)!
    renderRows(
      [
        plan,
        { ...plan, id: `p-old`, decision: `superseded`, plan: `# Old plan body` } as SessionChatRow,
      ],
      { docked: new Set([plan.id]) }
    )
    const cards = document.querySelectorAll(`[data-slot="chat-plan"]`)
    expect(cards).toHaveLength(2)
    const old = document.querySelector(`[data-decision="superseded"]`)!
    expect(old.textContent).toContain(`Replaced by a newer plan`)
    expect(old.textContent).not.toContain(`Old plan body`)
    expect(old.querySelector(`[data-slot="plan-card-decide"]`)).toBeNull()
  })

  it(`chats.json: dividers, a surface card and the phone bubbles`, () => {
    const rows: SessionChatRow[] = [
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
    renderRows(rows, { viewerId: `x`, messageTime: () => `21:02` })
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
    const question = document.querySelector(`[data-slot="chat-question"]`)!
    const options = question.querySelectorAll(`[data-testid="plan-card-option"]`)
    expect(options).toHaveLength(2)
    expect(options[0]!.textContent).toContain(`The first one`)
    expect(options[0]!.querySelectorAll(`span > span`)).toHaveLength(2)
    expect(options[1]!.querySelectorAll(`span > span`)).toHaveLength(1)
  })

  it(`answers a question with free text (the custom row wins over a single pick)`, () => {
    const { onAnswer } = renderRows()
    const question = document.querySelector(`[data-slot="chat-question"]`) as HTMLElement
    fireEvent.click(question.querySelectorAll(`[data-testid="plan-card-option"]`)[0]!)
    fireEvent.change(within(question).getByTestId(`plan-card-custom`), { target: { value: `Neither, use C` } })
    fireEvent.click(within(question).getByTestId(`chat-question-answer`))
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
    expect(screen.queryAllByTestId(`chat-question-answer`)).toHaveLength(0)
    expect(screen.queryAllByTestId(`plan-card-custom`)).toHaveLength(0)
    expect(screen.getByText(`Use C`)).toBeTruthy()
    expect(screen.getByText(`Yes, and start with a fresh context`)).toBeTruthy()
  })

  it(`marks only the viewer's own rows and their menu cancels a queued one (#FEED-237)`, async () => {
    const onUnqueue = vi.fn()
    const rows: SessionChatRow[] = [
      { type: `message`, id: `m1`, author: `user`, authorId: `u1`, body: `Mine`, queued: true, authorSessionId: null, xref: null },
      { type: `message`, id: `m3`, author: `user`, authorId: `u2`, body: `Theirs`, queued: true, authorSessionId: null, xref: null },
    ]
    const { rerender } = renderRows(rows, { viewerId: `u1`, onUnqueue })
    const marks = screen.getAllByTestId(`message-status`)
    expect(marks.map((mark) => mark.getAttribute(`data-state`))).toEqual([`sending`])
    await act(async () => {
      fireEvent.contextMenu(screen.getByText(`Mine`))
    })
    fireEvent.click(await screen.findByText(`Cancel sending`))
    expect(onUnqueue).toHaveBeenCalledWith(`m1`)
    // The device consumed it: two checks, no menu.
    rerender(
      <TooltipProvider>
        <ChatConversation
          rows={[{ ...rows[0]!, queued: false } as SessionChatRow, rows[1]!]}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          viewerId="u1"
          onUnqueue={onUnqueue}
        />
      </TooltipProvider>
    )
    expect(screen.getAllByTestId(`message-status`).map((mark) => mark.getAttribute(`data-state`))).toEqual([`read`])
    expect(document.querySelector(`[data-slot="user-message-more"]`)).toBeNull()
  })
})

describe(`ChatsListItem`, () => {
  it(`shows the word and never a lock (EXP-1339)`, () => {
    render(<ChatsListItem lead={null} title="T" word="stack" />)
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
    expect(chatTreeRows(rows).map((entry) => [entry.row.sessionId, entry.depth])).toEqual([
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
    expect(chatTreeRows(cyclic).map((entry) => [entry.row.sessionId, entry.depth])).toEqual([
      [`r`, 0],
      [`b`, 1],
      [`a`, 1],
      [`a1`, 2],
      [`x`, 0],
      [`y`, 1],
      [`y1`, 2],
    ])
    const onOpen = vi.fn()
    render(<ChatSessionTree rows={rows} activeId="a" onOpen={onOpen} />)
    const items = screen.getAllByTestId(`chat-session-tree-row`)
    expect(items.map((item) => item.textContent)).toEqual([`Session 1`, `B`, `ANeeds you`, `A1`])
    const word = items[2]!.querySelector(`[data-slot="chat-session-tree-word"]`)!
    expect(word.className).toContain(`text-yellow-400`)
    expect(items[2]!.className).toContain(`bg-glass-active`)
    expect(items[1]!.className).not.toContain(`bg-glass-active`)
    fireEvent.click(items[3]!)
    expect(onOpen).toHaveBeenCalledWith(`a1`)
  })
})

// EXP-1318: the row ends on the ROOT's device glyph, after the word.
describe(`ChatsListItem device`, () => {
  it(`trails the word, named for AT`, () => {
    render(
      <ChatsListItem
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
type Input = Parameters<typeof buildSessionChat>[0]
const DISPATCHER = fixture.cases.find((entry) => entry.name.startsWith(`EXP-1323: a live`))!
const MORE = fixture.cases.find((entry) => entry.name.startsWith(`EXP-1323: an ended`))!
const rowsOf = (entry: typeof DISPATCHER) =>
  buildSessionChat(entry.input as unknown as Input).rows

describe(`ChatConversation EXP-1323`, () => {
  function renderCase(rows: SessionChatRow[], over: Record<string, unknown> = {}) {
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
        <ChatConversation
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
    const card = document.querySelector(`[data-slot="chat-suggestions"]`)!
    expect(card.textContent).toContain(`6 suggested tasks · 3 started`)
    // A started item IS the child's session card.
    fireEvent.click(within(card as HTMLElement).getByText(`Add width and height to cart thumbnails`))
    expect(onOpenSession).toHaveBeenCalledWith(`c1`)
    expect(card.textContent).toContain(`Sent here`)
    expect(screen.getByTestId(`issue-ref`).textContent).toBe(`EXP-9`)
    expect(card.textContent).toContain(`Filed EXP-9`)
    // ONE open item: its buttons, no Start all.
    expect(screen.queryByTestId(`chat-suggestions-start-all`)).toBeNull()
    expect(screen.getAllByTestId(`chat-suggestion-start`)).toHaveLength(1)
    const conversation = screen.getByTestId(`chat-conversation`)
    expect(conversation.lastElementChild?.getAttribute(`data-testid`)).toBe(`live-tail`)
  })

  it(`#EXP-1772: a report is the child's bubble (its summary), its name + › above it opening the child`, () => {
    const { onOpenSession } = renderCase(rowsOf(DISPATCHER))
    const report = screen.getByRole(`button`, { name: `Lazy-load product images ›` })
    expect(report.dataset.slot).toBe(`message-bubble-relay`)
    expect(report.parentElement!.textContent).toContain(`Lazy-loading is in; PR #12 is open.`)
    fireEvent.click(report)
    expect(onOpenSession).toHaveBeenCalledWith(`c2`)
  })

  it(`Start, Start all and the ⋯ menu (Do it here, Dismiss) call back with the card and item`, () => {
    const { actions } = renderCase(rowsOf(MORE))
    fireEvent.click(screen.getByTestId(`chat-suggestions-start-all`))
    expect(actions.onStartAll).toHaveBeenCalledWith(`m4`)
    const starts = screen.getAllByTestId(`chat-suggestion-start`)
    expect(starts).toHaveLength(4)
    fireEvent.click(starts[0]!)
    expect(actions.onStart).toHaveBeenCalledWith(`m2`, `s1`)
    // N-12: Do it here and Dismiss live in the row's ⋯ menu.
    expect(screen.queryByTestId(`chat-suggestion-here`)).toBeNull()
    expect(screen.queryByTestId(`chat-suggestion-dismiss`)).toBeNull()
    openMenu(screen.getAllByTestId(`chat-suggestion-options`)[1]!)
    fireEvent.click(screen.getByRole(`menuitem`, { name: `Do it here` }))
    expect(actions.onHere).toHaveBeenCalledWith(`m4`, `s1`)
    openMenu(screen.getAllByTestId(`chat-suggestion-options`)[3]!)
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
      (screen.getAllByTestId(`chat-suggestion-options`)[0] as HTMLButtonElement).disabled
    ).toBe(true)
    expect(
      (screen.getAllByTestId(`chat-suggestion-options`)[1] as HTMLButtonElement).disabled
    ).toBe(false)
  })

  it(`N-12: open suggestions are a DockCard whose sheet lists them with Start all in the footer`, () => {
    const actions = { onStart: vi.fn(), onStartAll: vi.fn(), onHere: vi.fn(), onDismiss: vi.fn(), onFile: vi.fn() }
    const rows = rowsOf(MORE)
    render(
      <TooltipProvider>
        <ChatDock
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
    expect(chatDockedIds(rows, () => null).has(`m4`)).toBe(true)
    const card = screen.getAllByTestId(`dock-card`).find((node) => node.dataset.id === `m4`)!
    expect(card.dataset.kind).toBe(`suggestions`)
    const sheet = screen.getByTestId(`dock-sheet`)
    expect(within(sheet).getByTestId(`chat-suggestions-sheet`)).toBeTruthy()
    fireEvent.click(within(sheet).getByTestId(`chat-suggestions-start-all`))
    expect(actions.onStartAll).toHaveBeenCalledWith(`m4`)
  })

  it(`read-only: no buttons`, () => {
    renderCase(rowsOf(MORE), { suggestions: undefined })
    expect(screen.queryAllByTestId(`chat-suggestion-start`)).toHaveLength(0)
    expect(screen.queryByTestId(`chat-suggestions-start-all`)).toBeNull()
  })
})

describe(`ChatsListItem EXP-1323`, () => {
  it(`bolds an unread title and carries the caption`, () => {
    render(
      <ChatsListItem lead={null} title="Speed up the shop" unread caption="Dispatching" />
    )
    const title = screen.getByText(`Speed up the shop`)
    expect(title.getAttribute(`data-unread`)).toBe(`true`)
    expect(title.className).toContain(`font-semibold`)
    expect(screen.getByText(`Dispatching`)).toBeTruthy()
  })
  it(`a read row is plain`, () => {
    render(<ChatsListItem lead={null} title="Done" />)
    expect(screen.getByText(`Done`).getAttribute(`data-unread`)).toBeNull()
  })
})

describe(`ChatConversation EXP-1280 rich cards`, () => {
  const richCase = fixture.cases.find((c) => c.name.startsWith(`EXP-1280: a rich plan card`))!
  const rows = buildSessionChat(richCase.input as never).rows

  function renderRich(only: string[]) {
    const onAnswer = vi.fn()
    const onDecide = vi.fn()
    const renderText = vi.fn((text: string) => <p>{text}</p>)
    render(
      <TooltipProvider>
        <ChatConversation
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
    expect(screen.queryByTestId(`chat-question-answer`)).toBeNull()
    expect(screen.queryByTestId(`plan-card-custom`)).toBeNull()
  })

  it(`a decided plan's row reads its decision; a malformed surface never paints`, () => {
    renderRich([`n3`])
    expect(screen.queryAllByTestId(`ask-option`)).toHaveLength(0)
    expect(screen.queryByTestId(`plan-card-decision`)).toBeNull()
    expect(screen.getByTestId(`plan-card`).textContent).toContain(`Yes`)
    expect(screen.queryByTestId(`plan-card-surface`)).toBeNull()
  })

  it(`a decided plan's panel lists every question closed, no stepper; one question has no step chrome`, () => {
    const q = (id: string) => ({
      id,
      prompt: id,
      multi: false,
      custom: false,
      options: [{ label: `x`, description: null, attachmentId: null }],
    })
    const base = { type: `plan` as const, sessionId: `s1`, plan: `P`, surface: null, fresh: false, feedback: `Go.`, surfaceData: null }
    const closed = render(
      <PlanRail row={{ ...base, id: `d1`, questions: [q(`a`), q(`b`)], decision: `approved` as const, answers: { a: [`x`], b: [`x`] } }} busy={false} />
    )
    expect(within(closed.container).getAllByTestId(`plan-card-question`)).toHaveLength(2)
    expect(within(closed.container).queryByTestId(`plan-card-step`)).toBeNull()
    expect(within(closed.container).getByTestId(`plan-card-feedback`).textContent).toBe(`Feedback: Go.`)
    closed.unmount()
    const single = render(<PlanRail row={{ ...base, id: `d2`, questions: [q(`c`)], decision: null, answers: null }} busy={false} onDecide={vi.fn()} />)
    expect(within(single.container).getAllByTestId(`plan-card-question`)).toHaveLength(1)
    expect(within(single.container).queryByTestId(`plan-card-step`)).toBeNull()
    expect(within(single.container).queryByTestId(`plan-card-step-next`)).toBeNull()
    expect(within(single.container).getByTestId(`plan-card-decision`)).toBeTruthy()
  })

  it(`a pending question with a surface answers with its data model`, () => {
    const onAnswer = vi.fn()
    const surface = { root: { id: `t`, component: `Text`, props: { text: `Size` } }, data: { size: `lg` } }
    render(
      <ChatConversation
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
    fireEvent.click(screen.getByTestId(`chat-question-answer`))
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

describe(`ChatConversation S-3 xrefs`, () => {
  it(`#EXP-1736: the origin's prompt, another session's message and the report are that session's bubbles`, () => {
    const view = buildSessionChat(XREF.input as unknown as Input)
    const onOpenXref = vi.fn()
    const onOpenSession = vi.fn()
    render(
      <TooltipProvider>
        <ChatConversation
          rows={view.rows}
          origin={view.origin}
          chat={{ sessionId: view.rootId, title: view.title, agentId: null }}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => `/api/attachments/${id}`}
          onOpenSession={onOpenSession}
          onOpenXref={onOpenXref}
          xrefAgent={(id) => (id === `a1` ? { icon: `bot`, color: `#8b5cf6` } : null)}
        />
      </TooltipProvider>
    )
    // No link rows any more: the bubbles carry the sender.
    expect(screen.queryAllByTestId(`chat-xref`)).toHaveLength(0)
    // #EXP-1772: the relay's name above its bubble is the link.
    const links = screen.getAllByRole(`button`).filter((el) => el.dataset.slot === `message-bubble-relay`)
    expect(links.map((link) => link.textContent)).toContain(`Batch 2 ›`)
    // The agent's mark leads an agent sender's group.
    expect(document.querySelector(`[data-slot="agent-avatar"]`)).not.toBeNull()
    const child = links.find((link) => link.textContent === `Batch 2 ›`)!
    fireEvent.click(child)
    expect(onOpenXref).toHaveBeenCalledWith(`c1`)
    expect(onOpenSession).not.toHaveBeenCalled()
  })
})

// #EXP-1561 / #EXP-1562 (fixtures/waiting-cards.json): THE card's ×, the
// dismissed line, and the strip's waiting chips.
describe(`waiting cards`, () => {
  const question: SessionChatRow = {
    type: `question`,
    id: `q1`,
    sessionId: `s1`,
    question: `Which layout should the header chips use on phones?`,
    options: [{ label: `A`, description: null, attachmentId: null }],
    custom: true,
    multi: false,
    surface: null,
    answer: null,
    answers: null,
    surfaceData: null,
    answered: false,
  }
  const plan: SessionChatRow = {
    type: `plan`,
    id: `p1`,
    sessionId: `s1`,
    plan: `## Plan\n\n1. One`,
    questions: [],
    surface: null,
    decision: null,
    fresh: false,
    answers: null,
    feedback: null,
    surfaceData: null,
  }
  const renderCards = (rows: SessionChatRow[], onDismiss?: (id: string) => void) =>
    render(
      <TooltipProvider>
        <ChatConversation
          rows={rows}
          renderText={(text) => <p>{text}</p>}
          attachmentSrc={(id) => id}
          onAnswer={() => {}}
          onDecide={() => {}}
          onDismiss={onDismiss}
        />
      </TooltipProvider>
    )

  it(`a waiting card carries THE × and dismisses at once`, () => {
    const onDismiss = vi.fn()
    renderCards([question, plan], onDismiss)
    const buttons = screen.getAllByTestId(`card-dismiss`)
    expect(buttons).toHaveLength(2)
    fireEvent.click(buttons[1]!)
    expect(onDismiss).toHaveBeenCalledWith(`p1`)
  })

  it(`no × without the dismiss path, nor on a settled card`, () => {
    renderCards([question])
    expect(screen.queryByTestId(`card-dismiss`)).toBeNull()
    renderCards([{ ...question, id: `q2`, answered: true, answer: `A` }], vi.fn())
    expect(screen.queryByTestId(`card-dismiss`)).toBeNull()
  })

  it(`a dismissed card is ONE quiet line: Dismissed + the title`, () => {
    renderCards([{ ...question, answered: true, dismissed: true }, { ...plan, decision: `dismissed` }], vi.fn())
    const lines = screen.getAllByTestId(`card-dismissed`)
    expect(lines.map((line) => line.textContent)).toEqual([
      `DismissedWhich layout should the header chips use on phones?`,
      `DismissedPlan`,
    ])
    expect(screen.queryByTestId(`card-dismiss`)).toBeNull()
    expect(screen.queryByText(`A`)).toBeNull()
  })
})
