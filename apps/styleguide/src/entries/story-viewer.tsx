import { StoryViewerSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// S-3 STORIES (fixtures/stories.json): THE story viewer, open on a draft
// question over an eUI surface with two choices (the app's
// `?dev=story-viewer` capture draws the same frame).

export const entry: StyleguideEntry = {
  id: `story-viewer`,
  section: `special`,
  owner: `S-3`,
  title: `Story viewer`,
  blurb: `THE viewer a story ring opens (fixture \`stories.json\` ×4; Exponential-specific, not eUI). Phones (iOS, Android, web-mobile) = full screen over everything; web wide + the IDE = a centred modal (\`viewer.width\` × \`viewer.height\` max, \`viewer.radius\`) over a scrim, with side arrows. Top: THE StoryProgress (one segment per story, \`progress.gap\` apart, \`progress.height\` tall: done = foreground, the current = its fill, todo = the glass-stroke track). Header: the ring's avatar, the story's session name, its meta muted (\`Question · 2m\`), the close ghost glyph; a tap on the avatar or name opens that session. Body: the title in one line, the pictures (a pager when several), the surface (THE surface card renderer), else the plan markdown. Footer: the plan options (Approve / Change + its feedback), the question's choices (+ the free-text field when custom), a reply field, or nothing for a result. Navigation: tap thirds, swipe, ←/→, Escape / swipe down closes; only a result advances on its own (\`autoAdvanceMs\`), a long press holds it. Answers go through THE card-answer path (\`sessions.answer\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `StoryViewer / StoryViewerPanel`,
      file: `packages/ui/src/stories.tsx`,
      note: `host state: apps/web/src/hooks/use-stories.tsx (mark seen, answers)`,
    },
    desktop: {
      state: `n/a`,
      note: `the IDE twin lands with the desktop half of S-3 (domain::stories)`,
    },
    ios: {
      state: `ok`,
      symbol: `StoryViewer`,
      file: `apps/ios/Exponential/UI/Chats/StoryViewer.swift`,
    },
    android: {
      state: `ok`,
      symbol: `StoryViewer`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/stories/StoryViewer.kt`,
    },
  },
  island: () => <StoryViewerSpecimen />,
}
