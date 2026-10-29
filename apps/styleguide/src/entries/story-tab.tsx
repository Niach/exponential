import { StoryTabSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// STORIES TAB (fixtures/stories.json `tab.specimen` = tabCases[1]): THE
// Stories tab body of the session screen, wide and phone (×4, and the app's
// `?dev=story-tab` capture).

export const entry: StyleguideEntry = {
  id: `story-tab`,
  section: `special`,
  owner: `S-3`,
  title: `Stories tab`,
  blurb: `THE session screen's Stories tab (fixture \`stories.json\` STORIES TAB ×4; Exponential-specific, not eUI): every story the thread's tree posted, never expiring, waiting first then newest first; a settled plan/question stays with its outcome in the meta (\`Plan · Approved · 1d\`). A tile = the title on one line with the meta muted at its end (a waiting one leads with the ask tint dot and tints its meta), then one row of picture thumbnails (\`tile.thumbHeight\` tall, width by aspect up to \`tile.thumbMaxWidth\`, \`tile.radius\`, the hairline) with the caption, else the label, under each. No prose, no bands. Above the tiles, while the thread has its pull request: the Stack band + THE StackRail and ONE Changes row opening the complete diff. A tap opens THE story viewer at that story. None = THE EmptyState.`,
  status: {
    web: {
      state: `ok`,
      symbol: `StoryTab / StoryTile`,
      file: `packages/ui/src/story-tab.tsx`,
      note: `host: apps/web/src/components/thread/thread-stories.tsx (useStoryTab, PR block, review pane)`,
    },
    desktop: {
      state: `ok`,
      symbol: `story_tab_body / story_tile`,
      file: `apps/desktop/crates/ui/src/story_tab.rs`,
      note: `host: thread_screen.rs (the Stories strip tab, PR block, review pane); styleguide twin entries/story_tab.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `StoryTabView / StoryTile`,
      file: `apps/ios/Exponential/UI/Thread/ThreadStories.swift`,
      note: `host: ThreadScreen.swift (the Stories pager page + strip tab, PR block, review page); specimen StorySpecimensScreen sg_story-tab`,
    },
    android: {
      state: `ok`,
      symbol: `StoryTab / StoryTile`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/stories/StoryTab.kt`,
      note: `host: ui/thread/ThreadStories.kt (the Stories strip tab, PR block, review page); specimen StorySpecimens.kt sg_story-tab`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <StoryTabSpecimen />
      <StoryTabSpecimen width={360} size="phone" />
    </div>
  ),
}
