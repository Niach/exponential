import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { sessionThread } from "./session-results"
import { SessionThreadView } from "./session-thread-view"

// EXP-1175: the results thread — publish order, Summary last as the reply,
// the host's pending cards after it.
describe(`SessionThreadView`, () => {
  it(`renders text, pictures, the reply and the children in order`, () => {
    const thread = sessionThread([
      { topic: `Summary`, text: `All done.` },
      { topic: `Plan`, text: `First step.` },
      { topic: `Issue detail`, label: `web`, attachmentId: `att-1`, width: 1280, height: 800 },
      { topic: `Tests`, text: `Green.` },
    ])
    render(
      <SessionThreadView
        thread={thread}
        attachmentSrc={(id) => `/api/attachments/${id}`}
        renderText={(text) => <p>{text}</p>}
        renderReply={(text) => <p data-testid="reply">{text}</p>}
      >
        <div data-testid="pending-card">card</div>
      </SessionThreadView>
    )
    const root = screen.getByTestId(`session-thread`)
    const order = Array.from(
      root.querySelectorAll(
        `[data-slot="session-thread-text"], [data-testid="session-inline-result"], [data-testid="reply"], [data-testid="pending-card"]`
      )
    ).map((el) => el.getAttribute(`data-testid`) === `session-inline-result` ? `session-inline-result` : el.textContent?.trim())
    expect(order[0]).toContain(`First step.`)
    expect(order[1]).toBe(`session-inline-result`)
    expect(order[2]).toContain(`Green.`)
    expect(order[3]).toBe(`All done.`)
    expect(order[4]).toBe(`card`)
    expect(screen.getByText(`Plan`)).toBeTruthy()
    expect(screen.queryByText(`Summary`)).toBeNull()
  })

  it(`renders no reply without a Summary`, () => {
    render(
      <SessionThreadView
        thread={{ items: [], reply: null }}
        attachmentSrc={(id) => id}
        renderText={(text) => text}
        renderReply={(text) => <p data-testid="reply">{text}</p>}
      />
    )
    expect(screen.queryByTestId(`reply`)).toBeNull()
  })
})
