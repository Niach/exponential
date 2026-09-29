import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { parseSessionResultGroups } from "@exp/ui"
import { IssueResultsBody } from "@/components/issue-results-face"

// The report text renders through the session view's markdown; importing that
// module would drag the whole steering surface into this test.
vi.mock(`@/components/agent-session`, () => ({
  renderResultText: (text: string) => (
    <div data-testid="report-markdown">{text}</div>
  ),
}))

describe(`IssueResultsBody`, () => {
  it(`renders a topic's report text above its pictures`, () => {
    const groups = parseSessionResultGroups([
      {
        topic: `Summary`,
        label: null,
        attachmentId: null,
        width: null,
        height: null,
        text: `Fixed the **login** flow.`,
      },
      {
        topic: `Login`,
        label: `web`,
        attachmentId: `a1`,
        width: 1280,
        height: 800,
      },
    ])
    render(<IssueResultsBody groups={groups} />)
    expect(screen.getByTestId(`issue-results-face`)).toBeTruthy()
    expect(screen.getByTestId(`report-markdown`).textContent).toBe(
      `Fixed the **login** flow.`
    )
    const img = screen.getByAltText(`web`) as HTMLImageElement
    expect(img.getAttribute(`src`)).toBe(`/api/attachments/a1`)
  })

  it(`renders a text-only report`, () => {
    const groups = parseSessionResultGroups([
      { topic: `Summary`, text: `Done.` },
    ])
    render(<IssueResultsBody groups={groups} />)
    expect(screen.getAllByTestId(`session-result-text`)).toHaveLength(1)
    expect(screen.queryAllByRole(`img`)).toHaveLength(0)
  })
})
