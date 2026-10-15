import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { FloatingComposer } from "./floating-composer"

// EXP-1316: the phones' reply box — a capsule that expands into the card the
// host hands in, collapses when focus leaves it, and keeps the draft.
describe(`FloatingComposer`, () => {
  it(`expands from the capsule into the card and collapses on blur, keeping the draft`, () => {
    render(
      <div>
        <FloatingComposer placeholder="Reply…">
          <textarea aria-label="field" />
        </FloatingComposer>
        <button type="button">outside</button>
      </div>
    )
    const capsule = screen.getByTestId(`floating-composer-capsule`)
    expect(capsule.textContent).toBe(`Reply…`)
    expect(screen.getByTestId(`floating-composer`).getAttribute(`data-expanded`)).toBe(`false`)

    fireEvent.click(capsule)
    expect(screen.getByTestId(`floating-composer`).getAttribute(`data-expanded`)).toBe(`true`)
    expect(screen.queryByTestId(`floating-composer-capsule`)).toBeNull()
    const field = screen.getByLabelText(`field`) as HTMLTextAreaElement
    expect(document.activeElement).toBe(field)

    fireEvent.change(field, { target: { value: `half a thought` } })
    fireEvent.blur(field, { relatedTarget: screen.getByText(`outside`) })
    expect(screen.getByTestId(`floating-composer-capsule`).textContent).toBe(`half a thought`)
    // The card stayed mounted: the draft is still in it.
    expect((screen.getByLabelText(`field`) as HTMLTextAreaElement).value).toBe(`half a thought`)
  })

  // Safari does not focus buttons on tap: the textarea blurs with no
  // relatedTarget while the pointer is down on Send. The card must stay up
  // through that, or the click never lands.
  it(`stays expanded through a pointerdown on the card's Send and a relatedTarget-less blur`, () => {
    const onSend = vi.fn()
    render(
      <div>
        <FloatingComposer placeholder="Reply…">
          <textarea aria-label="field" />
          <button type="button" onClick={onSend}>
            Send
          </button>
        </FloatingComposer>
        <button type="button">outside</button>
      </div>
    )
    fireEvent.click(screen.getByTestId(`floating-composer-capsule`))
    const field = screen.getByLabelText(`field`)
    const send = screen.getByText(`Send`)

    fireEvent.pointerDown(send)
    fireEvent.blur(field, { relatedTarget: null })
    expect(screen.getByTestId(`floating-composer`).getAttribute(`data-expanded`)).toBe(`true`)
    fireEvent.click(send)
    expect(onSend).toHaveBeenCalledTimes(1)

    // A press outside the card collapses it.
    fireEvent.pointerDown(screen.getByText(`outside`))
    expect(screen.getByTestId(`floating-composer`).getAttribute(`data-expanded`)).toBe(`false`)
    expect(screen.getByTestId(`floating-composer-capsule`).textContent).toBe(`Reply…`)
  })
})
