import { StoryViewerSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// S-3 STORIES (fixtures/stories.json `specimen`): THE story viewer, open on
// ring 0's first frame (the waiting board question's bare eUI surface with
// two described choices), the progress frozen at 0 (×4, and the app's
// `?dev=story-viewer` capture).

export const entry: StyleguideEntry = {
  id: `story-viewer`,
  section: `special`,
  owner: `S-3`,
  title: `Story viewer`,
  blurb: `THE viewer a story ring opens (fixture \`stories.json\` ×4; Exponential-specific, not eUI). It steps FRAMES (\`storyFrames(ring)\`): a picture story = one frame per picture, else one surface frame, else the plan text, else the bare title. Phones (iOS, Android, web-mobile) = full screen, the body edge to edge on black; web wide + the IDE = a centred modal (\`viewer.width\` × \`viewer.height\` max, \`viewer.radius\`) with the same layout inside and side arrows outside it that step (only the bare scrim closes). Floating over a top black-to-clear scrim (\`viewer.scrimHeight\`): THE StoryProgress (one segment per frame, \`progress.gap\` within a story, \`progress.storyGap\` between stories), the header (the ring's avatar, the session name, its meta muted, the close glyph; a tap on the avatar or name opens that session), then the frame's title (one line) and description (muted, up to 3 lines), each once. Body: a picture fitted on black, THE surface with no card chrome, or the plan markdown. Footer, on a story's last frame over the mirrored bottom scrim: the plan options (Approve / Change + its feedback), the question's choices (+ the free-text field when custom), or a reply field. Navigation: tap thirds, swipe, ←/→, the side arrows; Escape / swipe down closes; every frame has its own timer (a result's \`autoAdvanceMs\`), a long press holds it. Answers go through THE card-answer path (\`sessions.answer\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `StoryViewer / StoryViewerPanel`,
      file: `packages/ui/src/stories.tsx`,
      note: `host state: apps/web/src/hooks/use-stories.tsx (mark seen, answers)`,
    },
    desktop: {
      state: `ok`,
      symbol: `open / StoryContent`,
      file: `apps/desktop/crates/ui/src/story_viewer.rs`,
      note: `styleguide/entries/story_viewer.rs`,
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
