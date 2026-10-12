import { fireEvent, render, screen } from "@testing-library/react"
import { describe, expect, it, vi } from "vitest"

import { COMMENT_CALL_COPY, CommentCallCard } from "./comment-call-card"

// EXP-1241: the comment call card — "Commented on <chip>" over the comment in
// the timeline's look; the body opens the issue at the comment, Show more
// never does.

describe(`CommentCallCard`, () => {
  it(`settled: the heading, the author row and the body; the body opens`, () => {
    const onOpen = vi.fn()
    render(
      <CommentCallCard
        issue={<span>EXP-1</span>}
        author="Danny"
        caption="via MCP · 2 minutes ago"
        body={<p>Status</p>}
        onOpen={onOpen}
      />
    )
    const card = screen.getByTestId(`comment-call-card`)
    expect(card.textContent).toContain(`${COMMENT_CALL_COPY.done}EXP-1`)
    expect(card.textContent).toContain(`Danny`)
    expect(card.textContent).toContain(`via MCP · 2 minutes ago`)
    fireEvent.click(screen.getByRole(`link`))
    expect(onOpen).toHaveBeenCalledTimes(1)
  })

  it(`running: the progressive heading and no body`, () => {
    render(<CommentCallCard issue={null} author={null} body={null} running />)
    expect(screen.getByTestId(`comment-call-card`).textContent).toContain(
      COMMENT_CALL_COPY.running
    )
    expect(screen.queryByRole(`link`)).toBeNull()
  })
})
