import { useState } from "react"
import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { guideViewer } from "@exp/domain-contract/guide-viewer"
import { GuidePictureViewer, GuidePictureViewerPanel } from "./guide-picture-viewer"

// EXP-1283: the Guide picture viewer — topic, label, text, ‹ n / N ›.
const GUIDE = [
  { topic: `Summary`, text: `Did it.` },
  { topic: `Summary`, label: `web`, attachmentId: `a0` },
  { topic: `nav`, text: `The nav moved.` },
  { topic: `nav`, label: `ios`, attachmentId: `a1` },
  { topic: `nav`, label: `android`, attachmentId: `a2` },
]

describe(`GuidePictureViewerPanel`, () => {
  it(`names the topic, the label and the text, and pages`, () => {
    const onSelect = vi.fn()
    render(<GuidePictureViewerPanel state={guideViewer(GUIDE, `a1`)!} onSelect={onSelect} />)
    expect(screen.getByTestId(`guide-viewer-topic`).textContent).toBe(`nav`)
    expect(screen.getByTestId(`guide-viewer-label`).textContent).toBe(`ios`)
    expect(screen.getByTestId(`guide-viewer-text`).textContent).toBe(`The nav moved.`)
    expect(screen.getByTestId(`guide-viewer-index`).textContent).toBe(`2 / 3`)
    fireEvent.click(screen.getByTestId(`guide-viewer-prev`))
    expect(onSelect).toHaveBeenLastCalledWith(`a0`)
    fireEvent.click(screen.getByTestId(`guide-viewer-next`))
    expect(onSelect).toHaveBeenLastCalledWith(`a2`)
  })

  it(`does not wrap at the ends, and keeps the end button focusable (aria-disabled, no-op)`, () => {
    const onSelect = vi.fn()
    render(<GuidePictureViewerPanel state={guideViewer(GUIDE, `a0`)!} onSelect={onSelect} />)
    const prev = screen.getByTestId(`guide-viewer-prev`) as HTMLButtonElement
    expect(prev.getAttribute(`aria-disabled`)).toBe(`true`)
    expect(prev.disabled).toBe(false)
    fireEvent.click(prev)
    expect(onSelect).not.toHaveBeenCalled()
    expect(screen.getByTestId(`guide-viewer-next`).getAttribute(`aria-disabled`)).toBeNull()
  })
})

describe(`GuidePictureViewer`, () => {
  it(`opens the lightbox in its panel mode and steps with the arrow keys`, () => {
    const onSelect = vi.fn()
    render(
      <GuidePictureViewer
        state={guideViewer(GUIDE, `a1`)!}
        src="/a1.png"
        onSelect={onSelect}
        onClose={() => {}}
      />
    )
    const dialog = screen.getByTestId(`image-preview-panel-dialog`)
    expect(screen.getByTestId(`image-preview-side`).textContent).toContain(`The nav moved.`)
    fireEvent.keyDown(dialog, { key: `ArrowRight` })
    expect(onSelect).toHaveBeenLastCalledWith(`a2`)
    fireEvent.keyDown(dialog, { key: `ArrowLeft` })
    expect(onSelect).toHaveBeenLastCalledWith(`a0`)
  })

  it(`keeps ←/→ working from the Next button after it reached the last picture`, () => {
    function Host() {
      const [id, setId] = useState(`a1`)
      return (
        <GuidePictureViewer
          state={guideViewer(GUIDE, id)!}
          src={`/${id}.png`}
          onSelect={setId}
          onClose={() => {}}
        />
      )
    }
    render(<Host />)
    const next = screen.getByTestId(`guide-viewer-next`) as HTMLButtonElement
    next.focus()
    fireEvent.click(next)
    expect(screen.getByTestId(`guide-viewer-index`).textContent).toBe(`3 / 3`)
    // Still focused (aria-disabled, not disabled), so the dialog hears ←.
    expect(document.activeElement).toBe(screen.getByTestId(`guide-viewer-next`))
    fireEvent.keyDown(document.activeElement!, { key: `ArrowLeft` })
    expect(screen.getByTestId(`guide-viewer-index`).textContent).toBe(`2 / 3`)
  })
})
