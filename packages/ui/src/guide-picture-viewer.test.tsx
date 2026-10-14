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

  it(`does not wrap at the ends`, () => {
    render(<GuidePictureViewerPanel state={guideViewer(GUIDE, `a0`)!} onSelect={() => {}} />)
    expect((screen.getByTestId(`guide-viewer-prev`) as HTMLButtonElement).disabled).toBe(true)
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
})
