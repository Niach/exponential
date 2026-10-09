import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"
import { sessionThread, sessionTurns } from "./session-results"
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

// EXP-1245: the owner's thread as turns — bubble, the turn's own status row,
// then its items and reply.
describe(`SessionThreadView turns`, () => {
  it(`renders each turn's bubble, row, items and reply in order`, () => {
    const t0 = 1_760_000_000_000
    const { turns } = sessionTurns(
      [
        { topic: `Summary`, text: `The x is gone.`, at: t0 + 1000 },
        { topic: `Reviews`, text: `Resolved by pr_url.`, at: t0 + 9000 },
      ],
      [
        { kind: `turn`, state: `started`, at: t0 },
        { kind: `turn`, state: `ended`, at: t0 + 2000 },
        { kind: `user_message`, at: t0 + 5000, text: `why is this PR not connected?`, images: [`/i1`] },
        { kind: `turn`, state: `started`, at: t0 + 5100 },
      ]
    )
    render(
      <SessionThreadView
        turns={turns}
        renderStatusRow={(_turn, index) => <div data-testid="turn-row">row {index}</div>}
        messageCaption={() => `Danny · 21:40 · from mint`}
        attachmentSrc={(id) => id}
        renderText={(text) => <p>{text}</p>}
        renderReply={(text) => <p data-testid="reply">{text}</p>}
      />
    )
    const root = screen.getByTestId(`session-thread`)
    const order = Array.from(
      root.querySelectorAll(
        `[data-testid="turn-row"], [data-testid="user-message-bubble"], [data-slot="session-thread-text"], [data-testid="reply"]`
      )
    ).map((el) => el.getAttribute(`data-testid`) ?? `text`)
    expect(order).toEqual([`turn-row`, `reply`, `user-message-bubble`, `turn-row`, `text`])
    expect(screen.getByText(`Danny · 21:40 · from mint`)).toBeTruthy()
    expect(root.querySelector(`[data-slot="user-message-thumb"]`)?.getAttribute(`src`)).toBe(`/i1`)
  })
})

// EXP-1245: the bubble's caption parts.
import { userMessageCaption, userMessageTime } from "./user-message-bubble"

describe(`userMessageCaption`, () => {
  it(`joins name, local time and device, dropping missing parts`, () => {
    const at = new Date(2026, 9, 9, 21, 40).getTime()
    expect(userMessageTime(at)).toBe(`21:40`)
    expect(userMessageCaption({ name: `Danny`, at, device: `mint` })).toBe(`Danny · 21:40 · from mint`)
    expect(userMessageCaption({ name: `Danny`, at: null, device: ` ` })).toBe(`Danny`)
    expect(userMessageCaption({ at })).toBe(`21:40`)
  })
})
