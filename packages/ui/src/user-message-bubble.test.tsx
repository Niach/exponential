import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { MessageBubble, UserMessageBubble } from "./user-message-bubble"

// Wave D: a message's files render as a comment's file chips.
describe(`UserMessageBubble files`, () => {
  it(`links each file by name`, () => {
    render(
      <UserMessageBubble
        text="see"
        files={[{ name: `build.log`, url: `/api/attachments/f1` }]}
      />
    )
    const link = screen.getByRole(`link`, { name: `Open build.log` })
    expect(link.getAttribute(`href`)).toBe(`/api/attachments/f1`)
    expect(document.querySelector(`[data-slot="user-message-thumb"]`)).toBeNull()
  })
})

// #EXP-1876 (transcript-grouping.json `tail`): THE tail, start for `in`, end for `out`.
describe(`MessageBubble tail`, () => {
  it(`an in bubble with tail wears the start tail and ships its one stylesheet`, () => {
    const { container } = render(<MessageBubble side="in" text="Merged." tail />)
    expect(container.querySelector(`.exp-bubble-tail-start`)).not.toBeNull()
    expect(container.querySelector(`.exp-bubble-tail-end`)).toBeNull()
    expect(document.querySelector(`style[data-href="exp-bubble-tail"]`)?.textContent).toContain(`.exp-bubble-tail-start{border-start-start-radius:0}`)
  })
  it(`an out bubble with tail wears the end tail; no tail = neither`, () => {
    const { container } = render(<MessageBubble side="out" text="Ship it." tail />)
    expect(container.querySelector(`.exp-bubble-tail-end`)).not.toBeNull()
    const plain = render(<MessageBubble side="out" text="And the follow-ups." />)
    expect(plain.container.querySelector(`.exp-bubble-tail-end,.exp-bubble-tail-start`)).toBeNull()
  })
})
