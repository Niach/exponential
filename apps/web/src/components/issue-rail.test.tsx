import { render } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { IssueRailLayer } from "./issue-rail"

// EXP-1057: the rail's painter — one dot per row, no lanes, no arrows; the
// dot is the mini-graph popover's trigger (mocked here to its trigger).

vi.mock(`@/components/issue-blocks-badge`, () => ({
  IssueBlocksPopover: ({ trigger }: { trigger: React.ReactElement }) => (
    <div data-testid="popover">{trigger}</div>
  ),
}))

describe(`IssueRailLayer`, () => {
  it(`draws nothing for a row with no relation, or a list without a rail`, () => {
    const none = render(
      <IssueRailLayer blockedBy={0} blocking={0} width={32} issueId="a" teamId="t" />
    )
    expect(none.container.firstChild).toBeNull()
    const noRail = render(
      <IssueRailLayer blockedBy={1} blocking={0} width={0} issueId="a" teamId="t" />
    )
    expect(noRail.container.firstChild).toBeNull()
  })

  it(`fills a blocker's dot and rings a blocked row's, labelled by the counts`, () => {
    const blocking = render(
      <IssueRailLayer blockedBy={0} blocking={1} width={32} issueId="a" teamId="t" />
    )
    const a = blocking.getByTestId(`issue-rail-node`)
    expect(a.getAttribute(`data-kind`)).toBe(`blocking`)
    expect(a.getAttribute(`aria-label`)).toBe(`Blocking 1`)

    const blocked = render(
      <IssueRailLayer blockedBy={2} blocking={1} width={32} issueId="b" teamId="t" />
    )
    const b = blocked.container.querySelector(`[data-testid="issue-rail-node"]`)!
    expect(b.getAttribute(`data-kind`)).toBe(`blocked`)
    expect(b.getAttribute(`aria-label`)).toBe(`Blocked by 2, blocking 1`)
    // The dot hangs the mini-graph; no lane or arrow is drawn anywhere.
    expect(blocked.container.querySelector(`[data-testid="popover"]`)).toBeTruthy()
    expect(blocked.container.querySelector(`svg`)).toBeNull()
  })

  it(`keeps an inert dot without a graph scope`, () => {
    const { queryByTestId, getByTestId } = render(
      <IssueRailLayer blockedBy={1} blocking={0} width={32} issueId="a" teamId={undefined} />
    )
    expect(getByTestId(`issue-rail-node`)).toBeTruthy()
    expect(queryByTestId(`popover`)).toBeNull()
  })
})
