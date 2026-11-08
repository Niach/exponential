import { describe, expect, it } from "vitest"
import { render, screen } from "@testing-library/react"
import fixture from "@exp/domain-contract/fixtures/sealed-content.json"
import type { SessionChatRow } from "@exp/domain-contract/session-tree"
import { ChatList, SEALED_LOCKED_TEST_ID, ChatConversation, TooltipProvider } from "@exp/ui"

// EXP-1459 (sealed-content.json `placeholder`): THE locked state on the web:
// one transcript pill per locked row (both layouts, no time), the list's
// preview italic.

const ROWS: SessionChatRow[] = [
  { type: `locked`, id: `k1`, reason: `noKey`, author: `agent` },
  { type: `locked`, id: `k2`, reason: `unreadable`, author: `user` },
]

function transcript(layout: `transcript` | `bubbles`) {
  return render(
    <TooltipProvider>
      <ChatConversation
        rows={ROWS}
        layout={layout}
        renderText={(text) => text}
        attachmentSrc={() => ``}
        messageTime={() => `21:02`}
        arrived={() => false}
      />
    </TooltipProvider>
  )
}

describe(`locked content`, () => {
  for (const layout of [`transcript`, `bubbles`] as const) {
    it(`${layout}: a pill per locked row with the reason's copy and no time`, () => {
      const { unmount } = transcript(layout)
      const pills = screen.getAllByTestId(SEALED_LOCKED_TEST_ID)
      expect(pills.map((pill) => pill.textContent)).toEqual([fixture.placeholder.copy.noKey, fixture.placeholder.copy.unreadable])
      expect(pills.map((pill) => pill.dataset.kind)).toEqual([`locked-noKey`, `locked-unreadable`])
      expect(screen.queryByText(/21:02/)).toBeNull()
      unmount()
    })
  }

  it(`the chats list paints a locked preview italic`, () => {
    render(
      <TooltipProvider>
        <ChatList
          rows={[
            {
              sessionId: `r1`,
              teamId: `t1`,
              title: `Session`,
              deviceId: null,
              group: `recent`,
              lead: `waiting`,
              preview: { kind: `locked`, text: fixture.placeholder.copy.noKey },
              activityAt: `2026-10-30T10:00:00Z`,
              time: `10:00`,
              unreadCount: 0,
              needsYou: false,
            } as never,
          ]}
        />
      </TooltipProvider>
    )
    const preview = screen.getByText(fixture.placeholder.copy.noKey)
    expect(preview.closest(`.italic`)).not.toBeNull()
  })
})
