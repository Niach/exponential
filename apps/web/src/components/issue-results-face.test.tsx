import { render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"
import { parseSessionResultGroups } from "@exp/ui"
import { fireEvent } from "@testing-library/react"
import {
  IssueResultsBody,
  prDescriptionGroups,
} from "@/components/issue-results-face"

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

// EXP-1154: the Guide on the issue — file rows off the PR files, a row opens
// the Changes face; the PR-body fallback is one unnumbered band.
describe(`IssueResultsBody guide`, () => {
  it(`numbers the sections, counts their files and opens one`, () => {
    const groups = parseSessionResultGroups([
      { topic: `Summary`, text: `Did it.` },
      { topic: `Nav`, text: `The nav.`, files: [`src/nav.ts`] },
    ])
    const opened: string[] = []
    render(
      <IssueResultsBody
        groups={groups}
        files={[{ path: `src/nav.ts`, additions: 5, deletions: 1 }] as never}
        onOpenFile={(path) => opened.push(path)}
      />
    )
    expect(screen.getByTestId(`guide-lead`)).toBeTruthy()
    expect(screen.getByTestId(`guide-section-caption`).textContent).toBe(`01 / 01`)
    const row = screen.getByTestId(`guide-file-row`)
    expect(row.textContent).toContain(`+5`)
    fireEvent.click(row)
    expect(opened).toEqual([`src/nav.ts`])
  })

  it(`shows the PR body as one unnumbered band`, () => {
    const groups = prDescriptionGroups({
      kind: `ready`,
      title: `EXP-1: Fix login`,
      body: `  `,
      state: `open`,
    })
    expect(groups).toHaveLength(1)
    render(<IssueResultsBody groups={groups} numbered={false} />)
    expect(screen.getByText(`EXP-1: Fix login`)).toBeTruthy()
    expect(screen.getByTestId(`report-markdown`).textContent).toBe(`No description.`)
    expect(screen.queryByTestId(`guide-section-caption`)).toBeNull()
  })

  it(`names a titleless PR and waits for the body`, () => {
    expect(
      prDescriptionGroups({ kind: `ready`, title: ` `, body: `x`, state: null })[0]
        .topic
    ).toBe(`Pull request`)
    expect(prDescriptionGroups({ kind: `loading` })).toEqual([])
    render(<IssueResultsBody groups={[]} loading />)
    expect(screen.getByText(`Loading the pull request…`)).toBeTruthy()
  })
})
