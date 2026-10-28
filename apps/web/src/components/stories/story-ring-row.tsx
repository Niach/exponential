import { useState, type ReactNode } from "react"
import { StoryViewer } from "@exp/ui"
import type { StoryRing } from "@exp/domain-contract/stories"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useSessionNav } from "@/hooks/use-session-nav"
import { useStories } from "@/hooks/use-stories"

// S-3 STORIES (web): the chats page's story rings (stories.json RINGS) and
// THE viewer they open. The rings ride the agent bubbles row (chats.json
// BUBBLES, `AgentBubbles rings`): a bubble wears its agent's ring, the rest
// follow it; nothing here paints a row of its own.

export function useChatStories(
  teamId: string | undefined,
  teamSlug: string
): {
  rings: StoryRing[]
  avatarFor: (ring: StoryRing) => ReactNode
  open: (key: string) => void
  viewer: ReactNode
} {
  const stories = useStories(teamId)
  const [openRing, setOpenRing] = useState<string | null>(null)
  // A header tap opens that session (the #1309 back stack, nothing pushed:
  // the chats list is the route underneath).
  const { open } = useSessionNav({ teamSlug, here: null, back: undefined })
  return {
    rings: stories.rings,
    avatarFor: stories.avatarFor,
    open: setOpenRing,
    viewer: (
      <StoryViewer
        rings={stories.rings}
        openRing={openRing}
        onClose={() => setOpenRing(null)}
        avatarFor={stories.avatarFor}
        cardFor={stories.cardFor}
        onShown={stories.markShown}
        onOpenSession={open}
        onDecide={stories.onDecide}
        onAnswer={stories.onAnswer}
        pending={stories.pending}
        renderText={renderThreadText}
      />
    ),
  }
}
