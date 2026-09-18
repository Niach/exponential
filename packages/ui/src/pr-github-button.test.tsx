import { fireEvent, render, screen } from "@testing-library/react"
import { afterEach, describe, expect, it, vi } from "vitest"
import { PrGithubButton } from "./pr-github-button"

// EXP-916: THE GitHub control of every diff surface. Three shapes, one set of
// words and one behaviour — each shape keeping the `data-testid` its surface
// had before they were merged.

const PR_URL = `https://github.com/niach/exponential/pull/1`

afterEach(() => {
  vi.restoreAllMocks()
})

describe(`PrGithubButton`, () => {
  it(`keeps each variant's own default data-testid`, () => {
    const { rerender } = render(<PrGithubButton prUrl={PR_URL} />)
    expect(screen.getByTestId(`changes-github-action`)).toBeTruthy()
    rerender(<PrGithubButton prUrl={PR_URL} variant="circle" />)
    expect(screen.getByTestId(`changes-github-circle`)).toBeTruthy()
    rerender(<PrGithubButton prUrl={PR_URL} variant="glass" />)
    expect(screen.getByTestId(`changes-github-link`)).toBeTruthy()
  })

  it(`lets a surface override the test id`, () => {
    render(<PrGithubButton prUrl={PR_URL} testId="run-github" />)
    expect(screen.queryByTestId(`changes-github-action`)).toBeNull()
    expect(screen.getByTestId(`run-github`)).toBeTruthy()
  })

  // The bar circle is the 52px work-bar glass slot, not a Button.
  it(`the circle wears the shared work-bar chrome`, () => {
    render(<PrGithubButton prUrl={PR_URL} variant="circle" />)
    const button = screen.getByTestId(`changes-github-circle`)
    expect(button.className).toContain(`size-[52px]`)
    expect(button.querySelector(`svg`)).not.toBeNull()
  })

  it(`opens the PR in a new tab with the noopener guard`, () => {
    const open = vi.spyOn(window, `open`).mockReturnValue(null)
    render(<PrGithubButton prUrl={PR_URL} />)
    fireEvent.click(screen.getByTestId(`changes-github-action`))
    expect(open).toHaveBeenCalledWith(PR_URL, `_blank`, `noopener,noreferrer`)
  })
})
