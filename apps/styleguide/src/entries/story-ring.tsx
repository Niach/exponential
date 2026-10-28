import { StoryRingSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// S-3 STORIES (fixtures/stories.json): THE story ring, the same four rings
// as the natives' specimens and the app's `?dev=story-ring` capture.

export const entry: StyleguideEntry = {
  id: `story-ring`,
  section: `special`,
  owner: `S-3`,
  title: `Story ring`,
  blurb: `A session's own show cards (a plan, a question, a result) as one ring per agent and per chat with no agent (\`stories()\`, fixture \`stories.json\` ×4). The avatar sits inside a ring split into one arc per story, \`ring.gap\` degrees apart, \`ring.width\` px: a waiting story = the plan/question tint (ask-dock: plan primary, question yellow), an unseen one = foreground, a seen one = the glass-stroke hairline. The name sits under it (foreground medium while anything is unseen, else muted). Order: the Assistant first, then rings with waiting, then unseen, then the rest, each by its newest story. A tap opens THE story viewer at that ring (test id \`story-ring\`). Specimen: the Assistant (an unseen + a seen result), a chat with a waiting question, an agent with two unseen results and a seen one, a chat whose only result was seen.`,
  status: {
    web: {
      state: `ok`,
      symbol: `StoryRing / StoryRingRow`,
      file: `packages/ui/src/stories.tsx`,
      note: `chats page: apps/web/src/components/stories/story-ring-row.tsx; model: hooks/use-stories.tsx`,
    },
    desktop: {
      state: `n/a`,
      note: `the IDE twin lands with the desktop half of S-3 (domain::stories)`,
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
