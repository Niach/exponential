import { StoryRingSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// S-3 STORIES (fixtures/stories.json `specimen`): THE story ring, the same
// four rings ×4 (the fold of the hardcoded specimen input) and the app's
// `?dev=story-ring` capture.

export const entry: StyleguideEntry = {
  id: `story-ring`,
  section: `special`,
  owner: `S-3`,
  title: `Story ring`,
  blurb: `A session's own show cards (a plan, a question, a result) as one ring per agent and per chat with no agent (\`stories()\`, fixture \`stories.json\` ×4). The avatar sits inside a ring split into one arc per story, \`ring.gap\` degrees apart, \`ring.width\` px: a waiting story = the plan/question tint (ask-dock: plan primary, question yellow), an unseen one = foreground, a seen one = the glass-stroke hairline. The name sits under it (foreground medium while anything is unseen, else muted). A ring holds the last 24 h only (#EXP-1461, \`constants.windowHours\`, waiting ones too); a child session's stories ride its root's ring. Order: the Assistant first, then rings with waiting, then unseen, then the rest, each by its newest story. A tap opens THE story viewer at that ring (test id \`story-ring\`). Avatars: an agent ring = the agent's icon in its colour on the muted disc (the bubble's look); a chat ring = the chat row's bare brand mark on the muted disc. Specimen (\`stories.json\` \`specimen\`, hardcoded ×4): the rings of \`stories(specimen.input)\` (Board header polish with a waiting question, Triage with a waiting plan + an unseen result, Designer with an unseen result, Ship the launch checklist whose only result was seen) with their titles, the same rings at 36 px without titles, then a StoryProgress (4 segments, the second at 0.4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `StoryRing / StoryRingRow`,
      file: `packages/ui/src/stories.tsx`,
      note: `chats page: apps/web/src/components/stories/story-ring-row.tsx; model: hooks/use-stories.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `story_ring / story_ring_button`,
      file: `apps/desktop/crates/ui/src/story_ring.rs`,
      note: `styleguide/entries/story_ring.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `StoryRing`,
      file: `apps/ios/ExpUI/Sources/StoryRing.swift`,
    },
    android: {
      state: `ok`,
      symbol: `StoryRing`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/StoryRing.kt`,
    },
  },
  island: () => <StoryRingSpecimen />,
}
