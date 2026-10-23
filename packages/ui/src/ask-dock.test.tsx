import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { ThreadDock } from "./session-thread-view"

const plan = (id: string, text: string, sessionId: string): ThreadRow => ({
  type: `plan`,
  id,
  sessionId,
  plan: text,
  questions: [],
  surface: null,
  decision: null,
  fresh: false,
  answers: null,
  feedback: null,
  surfaceData: null,
})

const ROWS: ThreadRow[] = [
  plan(`p1`, `# Fold the composer\n\n1. One`, `a`),
  plan(`p2`, `1. Two`, `b`),
  {
    type: `question`,
    id: `q1`,
    sessionId: `root`,
    question: `Which branch?`,
    options: [{ label: `main`, description: null, attachmentId: null }],
    custom: true,
    multi: false,
    surface: null,
    answer: null,
    answers: null,
    surfaceData: null,
    answered: false,
  },
]

function renderDock(rows = ROWS, props: Partial<Parameters<typeof ThreadDock>[0]> = {}) {
  const onDecide = vi.fn()
  const onAnswer = vi.fn()
  const names: Record<string, string> = { a: `Ask dock`, b: `Thread list` }
  render(
    <ThreadDock
      rows={rows}
      sessionName={(id) => names[id] ?? null}
      renderText={(text) => <p>{text}</p>}
      onDecide={onDecide}
      onAnswer={onAnswer}
      autoOpen={false}
      {...props}
    />
  )
  return { onDecide, onAnswer }
}

describe(`ThreadDock (N-12)`, () => {
  it(`piles several waiting cards oldest first, the newest in front, their words hidden until fanned`, () => {
    renderDock()
    const dock = screen.getByTestId(`dock`)
    expect(dock.dataset.mode).toBe(`pile`)
    const cards = screen.getAllByTestId(`dock-card`)
    expect(cards.map((card) => card.dataset.id)).toEqual([`p1`, `p2`, `q1`])
    expect(cards.map((card) => card.textContent)).toEqual([
      `Plan readyAsk dockFold the composer`,
      `Plan readyThread listTwo`,
      `QuestionWhich branch?`,
    ])
    expect(cards[0]!.getAttribute(`aria-hidden`)).toBe(`true`)
    expect(cards[2]!.getAttribute(`aria-hidden`)).toBeNull()
  })

  it(`one waiting card is a single card; a tap opens its sheet with the decision footer`, () => {
    const { onDecide } = renderDock([ROWS[0]!])
    expect(screen.getByTestId(`dock`).dataset.mode).toBe(`single`)
    fireEvent.click(screen.getByTestId(`dock-card`))
    const sheet = screen.getByTestId(`dock-sheet`)
    expect(sheet.textContent).toContain(`Fold the composer`)
    fireEvent.click(sheet.querySelector(`[data-slot="plan-card-decide"][data-decision="approved"]`)!)
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `approved` })
  })

  it(`No, keep planning opens the feedback field; the note rides with the rejection`, () => {
    const { onDecide } = renderDock([ROWS[0]!], { initialOpenId: `p1` })
    const sheet = screen.getByTestId(`dock-sheet`)
    fireEvent.click(sheet.querySelector(`[data-slot="plan-card-reject"]`)!)
    fireEvent.change(screen.getByTestId(`plan-card-feedback`), { target: { value: `Smaller steps` } })
    fireEvent.click(sheet.querySelector(`[data-slot="plan-card-decide"][data-decision="rejected"]`)!)
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `rejected`, feedback: `Smaller steps` })
  })

  it(`a question's sheet answers through its footer`, () => {
    const { onAnswer } = renderDock([ROWS[2]!], { initialOpenId: `q1` })
    const sheet = screen.getByTestId(`dock-sheet`)
    fireEvent.click(screen.getAllByTestId(`plan-card-option`)[0]!)
    fireEvent.click(screen.getByTestId(`thread-question-answer`))
    expect(onAnswer).toHaveBeenCalledWith(`q1`, { answer: `main`, answers: [`main`] })
    expect(sheet).toBeTruthy()
  })

  it(`nothing waits and no composer: nothing renders`, () => {
    renderDock([])
    expect(screen.queryByTestId(`dock`)).toBeNull()
  })
})

describe(`ThreadDock rich plan (EXP-1280, N-12 pages)`, () => {
  const rich: ThreadRow = {
    ...plan(`p1`, `# Plan`, `a`),
    questions: [
      { id: `q`, prompt: `Which layout?`, multi: false, custom: false, options: [{ label: `One`, description: null, attachmentId: null }, { label: `Two`, description: null, attachmentId: null }] },
      { id: `r`, prompt: `Ship it?`, multi: false, custom: false, options: [{ label: `Yes`, description: null, attachmentId: null }] },
    ],
  } as ThreadRow

  it(`pages through the plan and the questions; approving waits on every answer`, () => {
    const { onDecide } = renderDock([rich], { initialOpenId: `p1` })
    const sheet = screen.getByTestId(`dock-sheet`)
    expect(sheet.textContent).toContain(`Plan`)
    expect(screen.queryByTestId(`plan-card-step`)).toBeNull()
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    expect(screen.getByTestId(`plan-card-step`).textContent).toBe(`Question 1 of 2`)
    fireEvent.click(screen.getAllByTestId(`plan-card-option`)[1]!)
    fireEvent.click(screen.getByTestId(`plan-card-step-next`))
    expect(screen.getByTestId(`plan-card-step`).textContent).toBe(`Question 2 of 2`)
    const approve = sheet.querySelector<HTMLButtonElement>(`[data-slot="plan-card-decide"][data-decision="approved"]`)!
    expect(approve.disabled).toBe(true)
    expect(screen.getByTestId(`plan-card-issue`).textContent).toBe(`Answer every question to approve`)
    fireEvent.click(screen.getAllByTestId(`plan-card-option`)[0]!)
    expect(approve.disabled).toBe(false)
    fireEvent.click(approve)
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `approved`, answers: { q: [`Two`], r: [`Yes`] } })
  })
})
