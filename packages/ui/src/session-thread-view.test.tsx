import type { ReactNode } from "react"
import { act, fireEvent, render, screen, within } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import fixture from "@exp/domain-contract/fixtures/session-thread.json"
import { buildSessionThread, type ThreadRow } from "@exp/domain-contract/session-tree"
import {
  suggestionKey,
  ThreadChipSubtree,
  ThreadConversation,
  ThreadIssueTab,
  ThreadIssueTree,
  ThreadSessionTab,
  ThreadSessionTree,
  ThreadTabs,
  ThreadsListItem,
  ThreadDock,
  threadDockedIds,
  threadTreeRows,
  type ThreadStripItem,
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
  { type: `event`, id: `e1`, event: `issue_linked`, text: `Linked APP-5` },
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
        autoOpen={false}
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
    // N-12: a pending plan decides in its Dock sheet, never inline.
    const plan = document.querySelector(`[data-slot="thread-plan"]`)!
    expect(plan.querySelector(`[data-slot="plan-card-decide"]`)).toBeNull()
    expect(onDecide).not.toHaveBeenCalled()
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

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver ??= ResizeObserverStub as never

const GUIDE = { label: `Guide`, active: false, dot: false, onSelect: () => {} }
const CONVERSATION = { label: `Conversation`, active: false, onSelect: () => {} }

function item(
  key: string,
  node: (short: boolean, measure: boolean) => ReactNode,
  active = false
): ThreadStripItem {
  return {
    key,
    active,
    render: ({ short, measure }) => node(short, measure),
    entry: { kind: `item`, id: key, label: key, onSelect: () => {} },
  }
}

const issueItem = (identifier: string, active = false) =>
  item(
    identifier,
    (short, measure) => (
      <ThreadIssueTab
        identifier={identifier}
        title={`Title of ${identifier}`}
        status={{ icon: `circle-dashed` }}
        active={active}
        short={short}
        data-testid={measure ? undefined : `issue-${identifier}`}
      />
    ),
    active
  )

const sessionItem = (name: string, active = false) =>
  item(
    name,
    (_short, measure) => (
      <ThreadSessionTab
        agent="claude"
        markState="working"
        name={name}
        active={active}
        data-testid={measure ? undefined : `thread-tab-session`}
      />
    ),
    active
  )

/** jsdom lays nothing out: the ruler's chips and the zone get fixed widths
 *  (an issue 140 full / 70 short, a session 110, "+N" 32, divider 1). */
function stubWidths(zone: number) {
  return vi.spyOn(HTMLElement.prototype, `getBoundingClientRect`).mockImplementation(function (
    this: HTMLElement
  ) {
    const slot = this.getAttribute(`data-measure`)
    const width =
      this.getAttribute(`data-testid`) === `thread-tabs-zones`
        ? zone
        : slot === `issue-full`
          ? 140
          : slot === `issue-short`
            ? 70
            : slot === `session`
              ? 110
              : slot === `overflow`
                ? 32
                : slot === `divider`
                  ? 1
                  : 0
    return { width, height: 32, top: 0, left: 0, right: width, bottom: 32, x: 0, y: 0, toJSON() {} } as DOMRect
  })
}

describe(`ThreadTabs`, () => {
  it(`draws Conversation and Guide icon-only on the pager, labelled, the dot on the icon`, () => {
    render(
      <ThreadTabs
        variant="pager"
        conversation={{ ...CONVERSATION, active: true }}
        issues={[issueItem(`APP-5`)]}
        guide={{ ...GUIDE, dot: true }}
      />
    )
    const conversation = screen.getByTestId(`thread-tab-conversation`)
    const guide = screen.getByTestId(`thread-tab-guide`)
    expect(conversation.textContent).toBe(``)
    expect(conversation.getAttribute(`aria-label`)).toBe(`Conversation`)
    expect(conversation.querySelector(`svg`)).not.toBeNull()
    expect(guide.getAttribute(`aria-label`)).toBe(`Guide`)
    expect(guide.textContent).toBe(`The Guide has content`)
    expect(guide.className).toContain(`border-glass-stroke`)
    expect(screen.getByTestId(`thread-guide-dot`).className).toContain(`absolute`)
  })

  it(`keeps icon + label from md up`, () => {
    render(<ThreadTabs conversation={CONVERSATION} issues={[]} guide={GUIDE} />)
    expect(screen.getByTestId(`thread-tab-conversation`).textContent).toBe(`Conversation`)
    expect(screen.getByTestId(`thread-tab-guide`).textContent).toBe(`Guide`)
  })

  it(`pins + and Conversation left, Guide right, issues then sessions between`, () => {
    render(
      <ThreadTabs
        conversation={{ ...CONVERSATION, active: true }}
        issues={[issueItem(`APP-5`)]}
        add={<span data-testid="add">+</span>}
        sessions={[sessionItem(`Profile the launch path`)]}
        guide={{ ...GUIDE, dot: true }}
      />
    )
    const strip = screen.getByTestId(`thread-tabs`)
    expect(strip.className).not.toContain(`overflow-x-auto`)
    const order = [
      screen.getByTestId(`add`),
      screen.getByTestId(`thread-tab-conversation`),
      screen.getByTestId(`issue-APP-5`),
      screen.getByTestId(`thread-tab-session`),
      screen.getByTestId(`thread-tab-guide`),
    ]
    for (let index = 1; index < order.length; index++) {
      expect(
        order[index - 1]!.compareDocumentPosition(order[index]!) & Node.DOCUMENT_POSITION_FOLLOWING
      ).toBeTruthy()
    }
    expect(screen.getByTestId(`thread-guide-dot`)).toBeTruthy()
  })

  it(`goes full → identifiers only → folds each zone into its own "+N"`, () => {
    const issues = [issueItem(`APP-1`), issueItem(`APP-2`), issueItem(`APP-3`), issueItem(`APP-4`)]
    const sessions = [sessionItem(`One`), sessionItem(`Two`), sessionItem(`Three`)]
    const draw = (zone: number) => {
      const spy = stubWidths(zone)
      const view = render(
        <ThreadTabs conversation={CONVERSATION} issues={issues} sessions={sessions} guide={GUIDE} />
      )
      spy.mockRestore()
      return view
    }
    // 4 × 140 + 3 × 110 + gaps fits 1000.
    let view = draw(1000)
    expect(screen.getByTestId(`thread-tabs`).getAttribute(`data-tier`)).toBe(`full`)
    expect(within(screen.getByTestId(`thread-tabs-zones`)).getByText(`Title of APP-1`)).toBeTruthy()
    view.unmount()
    // 4 × 70 + 3 × 110 + gaps ≈ 640.
    view = draw(660)
    expect(screen.getByTestId(`thread-tabs`).getAttribute(`data-tier`)).toBe(`short`)
    expect(within(screen.getByTestId(`thread-tabs-zones`)).queryByText(`Title of APP-1`)).toBeNull()
    expect(screen.queryByTestId(`thread-tabs-more-issues`)).toBeNull()
    view.unmount()
    view = draw(420)
    expect(screen.getByTestId(`thread-tabs`).getAttribute(`data-tier`)).toBe(`fold`)
    expect(screen.getByTestId(`thread-tabs-more-issues`).textContent).toMatch(/^\+\d+$/)
    expect(screen.getByTestId(`thread-tabs-more-sessions`).textContent).toMatch(/^\+\d+$/)
    view.unmount()
  })

  it(`never folds the open chip away`, () => {
    const spy = stubWidths(200)
    render(
      <ThreadTabs
        conversation={CONVERSATION}
        issues={[issueItem(`APP-1`), issueItem(`APP-2`), issueItem(`APP-3`), issueItem(`APP-4`, true)]}
        sessions={[]}
        guide={GUIDE}
      />
    )
    spy.mockRestore()
    expect(screen.getByTestId(`issue-APP-4`)).toBeTruthy()
    expect(screen.getByTestId(`thread-tabs-more-issues`)).toBeTruthy()
  })
})

// EXP-1334: a folded chip = the chevron, `<chip> › <open one>`, and its
// subtree on a CLICK (THE Menu).
describe(`folded chips`, () => {
  it(`read the open nested session and open the subtree rows on a click`, () => {
    const onOpen = vi.fn()
    render(
      <ThreadChipSubtree
        title="Threads impl"
        trigger={
          <ThreadSessionTab
            agent="claude"
            markState="needs_you"
            name="Threads impl"
            active
            path="Android lane"
            chevron
          />
        }
      >
        {(close) => (
          <ThreadSessionTree
            menu
            rows={[
              { sessionId: `a`, parentId: null, name: `Threads impl`, agent: `claude`, markState: `needs_you` },
              { sessionId: `c`, parentId: `a`, name: `Android lane`, agent: `claude`, markState: `working` },
            ]}
            activeId="c"
            onOpen={(id) => {
              close()
              onOpen(id)
            }}
          />
        )}
      </ThreadChipSubtree>
    )
    const chip = screen.getByTestId(`thread-tab-session`)
    // A folded chip opens a menu, so it is no tab: its state is data-active.
    expect(chip.getAttribute(`role`)).toBeNull()
    expect(chip.getAttribute(`aria-selected`)).toBeNull()
    expect(chip.closest(`[data-slot="strip-chip"]`)!.getAttribute(`data-active`)).toBe(`true`)
    expect(chip.textContent).toContain(`› Android lane`)
    expect(chip.querySelector(`[data-slot="strip-chip-chevron"]`)).not.toBeNull()
    act(() => {
      fireEvent.pointerDown(chip, { button: 0, ctrlKey: false, pointerType: `mouse` })
    })
    const rows = screen.getAllByTestId(`thread-session-tree-row`)
    expect(rows.map((row) => row.textContent)).toEqual([`Threads impl`, `Android lane`])
    expect(rows[1]!.className).toContain(`bg-glass-active`)
    fireEvent.click(screen.getAllByTestId(`thread-session-tree-item`)[1]!)
    expect(onOpen).toHaveBeenCalledWith(`c`)
  })

  it(`lists an issue subtree with its identifiers and guides`, () => {
    const onOpen = vi.fn()
    render(
      <ThreadIssueTree
        rows={[
          { issueId: `1`, identifier: `APP-1`, title: `Parent`, status: { icon: `circle-dashed` }, depth: 0 },
          { issueId: `2`, identifier: `APP-2`, title: `Child`, status: { icon: `circle-dashed` }, depth: 1 },
        ]}
        activeId="2"
        onOpen={onOpen}
      />
    )
    const rows = screen.getAllByTestId(`thread-issue-tree-row`)
    expect(rows.map((row) => row.textContent)).toEqual([`APP-1Parent`, `APP-2Child`])
    expect(rows[1]!.querySelector(`[data-testid="tree-guides"]`)).not.toBeNull()
    fireEvent.click(rows[0]!)
    expect(onOpen).toHaveBeenCalledWith(`1`)
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
          autoOpen={false}
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
