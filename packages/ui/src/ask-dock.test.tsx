import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import type { ThreadRow } from "@exp/domain-contract/session-tree"
import { ThreadAskDock } from "./session-thread-view"

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

function renderDock(rows = ROWS) {
  const onDecide = vi.fn()
  const onAnswer = vi.fn()
  const names: Record<string, string> = { a: `Ask dock`, b: `Thread list` }
  render(
    <ThreadAskDock
      rows={rows}
      sessionName={(id) => names[id] ?? null}
      renderText={(text) => <p>{text}</p>}
      onDecide={onDecide}
      onAnswer={onAnswer}
    />
  )
  return { onDecide, onAnswer }
}

describe(`ThreadAskDock`, () => {
  it(`stacks several waiting cards, a tap focuses one, back returns`, () => {
    renderDock()
    const rows = screen.getAllByTestId(`ask-dock-row`)
    expect(rows.map((row) => row.textContent)).toEqual([
      `Plan readyAsk dockFold the composer`,
      `Plan readyThread listTwo`,
      `QuestionWhich branch?`,
    ])
    fireEvent.click(rows[1]!)
    expect(screen.getByTestId(`ask-dock`).dataset.mode).toBe(`focused`)
    expect(screen.queryAllByTestId(`ask-dock-row`)).toHaveLength(0)
    expect(screen.getByText(`1. Two`)).toBeTruthy()
    fireEvent.click(screen.getByTestId(`ask-dock-back`))
    expect(screen.getAllByTestId(`ask-dock-row`)).toHaveLength(3)
  })

  it(`one waiting card is the big card, no back arrow`, () => {
    const { onDecide } = renderDock([ROWS[0]!])
    expect(screen.getByTestId(`ask-dock`).dataset.mode).toBe(`single`)
    expect(screen.queryByTestId(`ask-dock-back`)).toBeNull()
    // NEW-6: the decision rows start on "Yes"; the button decides.
    fireEvent.click(document.querySelector(`[data-slot="plan-card-decide"]`)!)
    expect(onDecide).toHaveBeenCalledWith(`p1`, { decision: `approved` })
  })

  it(`nothing waits: nothing renders`, () => {
    const { container } = render(
      <ThreadAskDock rows={[]} sessionName={() => null} renderText={(t) => t} onDecide={vi.fn()} onAnswer={vi.fn()} />
    )
    expect(container.textContent).toBe(``)
  })
})

describe(`ThreadAskDock rich plan (EXP-1280)`, () => {
  it(`the big card carries the questions; approving waits on them`, () => {
    const rich: ThreadRow = {
      ...plan(`p9`, `1. Rich`, `a`),
      questions: [
        {
          id: `q`,
          prompt: `Which layout?`,
          multi: false,
          custom: false,
          options: [
            { label: `Two`, description: null, attachmentId: `att-2` },
            { label: `One`, description: null, attachmentId: null },
          ],
        },
      ],
    } as ThreadRow
    const { onDecide } = renderDock([rich])
    // One question = no step chrome.
    expect(screen.queryByTestId(`plan-card-step`)).toBeNull()
    const decide = document.querySelector<HTMLButtonElement>(`[data-slot="plan-card-decide"]`)!
    expect(decide.disabled).toBe(true)
    expect(screen.getByTestId(`plan-card-issue`)).toBeTruthy()
    fireEvent.click(screen.getByRole(`radio`, { name: `Two` }))
    expect(screen.getByTestId(`plan-card-tile`).querySelector(`img`)?.getAttribute(`src`)).toBe(`/api/attachments/att-2`)
    expect(decide.disabled).toBe(false)
    fireEvent.click(decide)
    expect(onDecide).toHaveBeenCalledWith(`p9`, { decision: `approved`, answers: { q: [`Two`] } })
  })
})
