import { useState } from "react"
import { StoryRingRow as StoryRingRowView, StoryViewer } from "@exp/ui"
import { renderThreadText } from "@/components/thread/thread-markdown"
import { useSessionNav } from "@/hooks/use-session-nav"
import { useStories } from "@/hooks/use-stories"

// S-3 STORIES (web): the chats page's story rings (stories.json RINGS) and
// THE viewer they open. Mounted just above the agent bubbles row until the
// rings move into the bubbles (chats.json BUBBLES, sibling session
// 58cca31b); nothing here when no ring has a story.

export function StoryRingRow({ teamId, teamSlug }: { teamId: string | undefined; teamSlug: string }) {
  const stories = useStories(teamId)
  const [openRing, setOpenRing] = useState<string | null>(null)
  // A header tap opens that session (the #1309 back stack, nothing pushed:
  // the chats list is the route underneath).
  const { open } = useSessionNav({ teamSlug, here: null, back: undefined })
  return (
    <>
      <StoryRingRowView rings={stories.rings} avatarFor={stories.avatarFor} onOpen={setOpenRing} />
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
    </>
  )
}
