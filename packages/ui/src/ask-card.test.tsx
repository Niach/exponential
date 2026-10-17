import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { ASK_CARD_COPY, AskPrompt, type AskOption } from "./ask-card"

// EXP-1316 (1m): the question / plan card's interactive half.
const OPTIONS: AskOption[] = [
  { key: `a`, label: `Alpha` },
  { key: `b`, label: `Beta` },
  { key: `free-text`, label: ASK_CARD_COPY.freeTextRow, freeText: true },
]

describe(`AskPrompt`, () => {
  it(`opens the free-text field inside a multi-select instead of picking it`, () => {
    const onAnswer = vi.fn()
    render(<AskPrompt options={OPTIONS} multiSelect answerable onAnswer={onAnswer} />)
    const rows = screen.getAllByTestId(`ask-option`)
    fireEvent.click(rows[0]!)
    fireEvent.click(rows[2]!)
    expect(rows[2]!.getAttribute(`aria-pressed`)).toBe(`false`)
    expect(rows[2]!.getAttribute(`aria-expanded`)).toBe(`true`)
    const field = screen.getByTestId(`ask-inline-field`)
    // Submitting the picks never carries the free-text key.
    fireEvent.click(screen.getByText(ASK_CARD_COPY.submit))
    expect(onAnswer).toHaveBeenCalledWith([`a`], [`Alpha`])
    fireEvent.change(field, { target: { value: `something else` } })
    fireEvent.keyDown(field, { key: `Enter` })
    expect(onAnswer).toHaveBeenLastCalledWith([`free-text`], [`something else`], `something else`)
  })

  it(`returns focus to the option row when Escape closes the field`, () => {
    render(<AskPrompt options={OPTIONS} answerable onAnswer={() => {}} />)
    const row = screen.getAllByTestId(`ask-option`)[2]!
    fireEvent.click(row)
    const field = screen.getByTestId(`ask-inline-field`)
    expect(document.activeElement).toBe(field)
    fireEvent.keyDown(field, { key: `Escape` })
    expect(screen.queryByTestId(`ask-inline-field`)).toBeNull()
    expect(document.activeElement).toBe(screen.getAllByTestId(`ask-option`)[2])
  })

  it(`keeps the typed draft across a lock (a failed answer reopens with it)`, () => {
    function Host() {
      const [locked, setLocked] = useState(false)
      return (
        <>
          <AskPrompt
            options={OPTIONS}
            answerable
            locked={locked ? { labels: [] } : null}
            onAnswer={() => setLocked(true)}
          />
          <button type="button" onClick={() => setLocked(false)}>
            fail
          </button>
        </>
      )
    }
    render(<Host />)
    fireEvent.click(screen.getAllByTestId(`ask-option`)[2]!)
    const field = screen.getByTestId(`ask-inline-field`) as HTMLTextAreaElement
    fireEvent.change(field, { target: { value: `half typed` } })
    fireEvent.keyDown(field, { key: `Enter` })
    expect(screen.getByText(ASK_CARD_COPY.answering)).toBeTruthy()
    expect(screen.queryByTestId(`ask-inline-field`)).toBeNull()
    fireEvent.click(screen.getByText(`fail`))
    expect((screen.getByTestId(`ask-inline-field`) as HTMLTextAreaElement).value).toBe(`half typed`)
  })

  it(`re-seeds the picks when initialPicked changes on a mounted card`, () => {
    function Host() {
      const [editing, setEditing] = useState(false)
      return (
        <>
          <AskPrompt
            options={OPTIONS}
            multiSelect
            answerable
            initialPicked={editing ? [`b`] : undefined}
            onAnswer={() => {}}
          />
          <button type="button" onClick={() => setEditing(true)}>
            edit
          </button>
        </>
      )
    }
    render(<Host />)
    fireEvent.click(screen.getAllByTestId(`ask-option`)[0]!)
    expect(screen.getAllByTestId(`ask-option`)[0]!.getAttribute(`aria-pressed`)).toBe(`true`)
    fireEvent.click(screen.getByText(`edit`))
    const rows = screen.getAllByTestId(`ask-option`)
    expect(rows[0]!.getAttribute(`aria-pressed`)).toBe(`false`)
    expect(rows[1]!.getAttribute(`aria-pressed`)).toBe(`true`)
  })
})

describe(`AskPrompt disabled options (EXP-1280)`, () => {
  it(`a disabled option is inert, the others still answer`, () => {
    const onAnswer = vi.fn()
    render(
      <AskPrompt
        variant="plan"
        options={[
          { key: `yes`, label: `Yes`, disabled: true },
          { key: `no`, label: `No` },
        ]}
        answerable
        onAnswer={onAnswer}
      />
    )
    const [yes, no] = screen.getAllByTestId(`ask-option`) as HTMLButtonElement[]
    expect(yes!.disabled).toBe(true)
    fireEvent.click(yes!)
    expect(onAnswer).not.toHaveBeenCalled()
    fireEvent.click(no!)
    expect(onAnswer).toHaveBeenCalledWith([`no`], [`No`])
  })
})
