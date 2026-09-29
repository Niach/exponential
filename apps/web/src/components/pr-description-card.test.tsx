import { beforeEach, describe, expect, it, vi } from "vitest"
import { fireEvent, render, screen, waitFor } from "@testing-library/react"

// EXP-1139: the review page's PR description card — GitHub's title/body over
// the diff, Edit for a member while the PR is open, one `issues.updatePr`
// carrying ONLY the fields that changed.

const mocks = vi.hoisted(() => ({
  prDescription: vi.fn(),
  updatePr: vi.fn(),
}))

vi.mock(`@/lib/trpc-client`, () => ({
  trpc: {
    issues: {
      prDescription: { query: mocks.prDescription },
      updatePr: { mutate: mocks.updatePr },
    },
  },
}))
// The read-only TipTap renderer needs a real editor; the card's contract is
// that it hands the body over, so a plain node stands in for it.
vi.mock(`@/components/issue-editor/markdown-editor`, () => ({
  MarkdownEditor: ({ markdown }: { markdown: string }) => (
    <div data-testid="markdown">{markdown}</div>
  ),
}))

import { PrDescriptionCard } from "@/components/pr-description-card"

const ISSUE_ID = `11111111-1111-4111-8111-111111111111`
const issue = (prState: `open` | `merged`) => ({
  id: ISSUE_ID,
  prNumber: 880,
  prState,
})

beforeEach(() => {
  vi.clearAllMocks()
  mocks.prDescription.mockResolvedValue({
    repo: `acme/app`,
    prNumber: 880,
    url: `https://github.com/acme/app/pull/880`,
    title: `EXP-792: MCP servers rework`,
    body: `Six-tile catalog, names Vercel.`,
    state: `open`,
  })
  mocks.updatePr.mockResolvedValue({
    updated: true,
    url: `https://github.com/acme/app/pull/880`,
    number: 880,
  })
})

describe(`PrDescriptionCard (EXP-1139)`, () => {
  it(`shows GitHub's title, number and body, with Edit for a member on an open PR`, async () => {
    render(<PrDescriptionCard issue={issue(`open`)} canEdit />)
    expect(await screen.findByText(`EXP-792: MCP servers rework`)).toBeTruthy()
    expect(screen.getByText(`#880`)).toBeTruthy()
    expect(screen.getByTestId(`markdown`).textContent).toBe(
      `Six-tile catalog, names Vercel.`
    )
    expect(screen.getByTestId(`pr-description-edit`)).toBeTruthy()
    expect(mocks.prDescription).toHaveBeenCalledWith({ issueId: ISSUE_ID })
  })

  it(`offers no Edit on a merged PR or to a non-member`, async () => {
    const { unmount } = render(
      <PrDescriptionCard issue={issue(`merged`)} canEdit />
    )
    await screen.findByText(`EXP-792: MCP servers rework`)
    expect(screen.queryByTestId(`pr-description-edit`)).toBeNull()
    unmount()

    render(<PrDescriptionCard issue={issue(`open`)} canEdit={false} />)
    await screen.findByText(`EXP-792: MCP servers rework`)
    expect(screen.queryByTestId(`pr-description-edit`)).toBeNull()
  })

  it(`renders nothing for an issue without a PR`, () => {
    const { container } = render(
      <PrDescriptionCard
        issue={{ id: ISSUE_ID, prNumber: null, prState: null }}
        canEdit
      />
    )
    expect(container.innerHTML).toBe(``)
    expect(mocks.prDescription).not.toHaveBeenCalled()
  })

  it(`saves only the changed field and shows the new body`, async () => {
    render(<PrDescriptionCard issue={issue(`open`)} canEdit />)
    fireEvent.click(await screen.findByTestId(`pr-description-edit`))
    const body = (await screen.findByPlaceholderText(
      `Description (GFM)`
    )) as HTMLTextAreaElement
    expect(body.value).toBe(`Six-tile catalog, names Vercel.`)
    fireEvent.change(body, {
      target: { value: `A searchable picker over a 19-entry catalog.` },
    })
    fireEvent.click(screen.getByText(`Save changes`))
    await waitFor(() =>
      expect(mocks.updatePr).toHaveBeenCalledWith(
        {
          issueId: ISSUE_ID,
          body: `A searchable picker over a 19-entry catalog.`,
        },
        { context: { skipErrorToast: true } }
      )
    )
    await waitFor(() =>
      expect(screen.getByTestId(`markdown`).textContent).toBe(
        `A searchable picker over a 19-entry catalog.`
      )
    )
    // The title did not change, so it is NOT in the mutation input.
    expect(
      Object.keys(mocks.updatePr.mock.calls[0]![0] as object)
    ).toEqual([`issueId`, `body`])
  })

  it(`keeps the dialog open and shows a refusal`, async () => {
    mocks.updatePr.mockRejectedValueOnce(
      new Error(`The pull request is merged. Only open pull requests can be edited.`)
    )
    render(<PrDescriptionCard issue={issue(`open`)} canEdit />)
    fireEvent.click(await screen.findByTestId(`pr-description-edit`))
    const title = (await screen.findByPlaceholderText(
      `Title`
    )) as HTMLInputElement
    fireEvent.change(title, { target: { value: `Renamed` } })
    fireEvent.click(screen.getByText(`Save changes`))
    expect(
      await screen.findByText(
        `The pull request is merged. Only open pull requests can be edited.`
      )
    ).toBeTruthy()
    expect(screen.getByTestId(`pr-description-dialog`)).toBeTruthy()
  })
})
