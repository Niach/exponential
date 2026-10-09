import { render, screen } from "@testing-library/react"
import { describe, expect, it } from "vitest"

import { UserMessageBubble } from "./user-message-bubble"

// Wave D: a message's files render as the comment thread's file chips.
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
