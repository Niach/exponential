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
  blurb: `THE viewer a story ring opens (fixture \`stories.json\` ×4; Exponential-specific, not eUI). #EXP-1493: ONE frame per story (\`storyFrames(ring)\`): a plan = its COVER (the first picture, THE kind badge + title + description, the counts line, up to 4 thumbnails, one full-width Open plan opening THE plan page); a picture story = ONE gallery frame (THE carousel: swipe inside steps the picture, i / n, the caption under it, the filmstrip); else THE surface with no card chrome; else the bare title. Phones = full screen, the body edge to edge on black; web wide + the IDE = a centred modal sized by its picture with side arrows outside it that step stories (only the bare scrim closes). Floating over a top scrim: THE StoryProgress (one segment per story, \`progress.gap\` apart; a gallery fills across its pictures), the header (the ring\'s avatar, the session name, its meta, close; a tap on the avatar or name opens that session), the badge + title line and the description. Footer over the bottom scrim: Open plan, a question\'s choices (+ the free-text field when custom), or a reply field. Tap thirds, swipe, ←/→ step stories; Escape / swipe down closes; a result advances per picture (\`autoAdvanceMs\`), a long press holds it. Answers go through THE card-answer path (\`sessions.answer\`).`,
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
