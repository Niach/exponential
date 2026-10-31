import { PlanPageSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1493: THE plan page (fixture `plan-page.json` ×4): the expanded form
// of THE plan card, wide and phone, drawn from `plan-page.json` `specimen`.

export const entry: StyleguideEntry = {
  id: `plan-page`,
  section: `special`,
  owner: `EXP-1493`,
  title: `Plan page`,
  blurb: `THE plan page (fixture \`plan-page.json\` ×4; Exponential-specific, not eUI): the expanded form of THE plan card, deep-linkable (\`/t/$teamSlug/sessions/$sessionId/plan/$messageId\`), opened by a story's Open plan, the plan card's header link, an inbox row or a Needs-you row. Wide (web md+, the IDE): one header row (back, the chat's name, Plan, THE kind badge, the counts line, Open chat, close) over three columns: THE Outline (the h2 sections with their picture counts, scroll-spy, the Questions row), the document (THE markdown in a 720 px column, h2 anchors, two or more adjacent pictures as THE Mosaic), the sticky rail (the plan card's stepper, one question at a time, and the decision). Phones: the badge and counts lead the same document; picture groups are THE Carousel (i / n, dots, swipe); ☰ opens the outline in a sheet; the foot is THE Dock's plan card opening the questions-only sheet; no composer. Any picture opens THE lightbox over every picture of the plan.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PlanPage`,
      file: `packages/ui/src/plan-page.tsx`,
      note: `route: apps/web/src/routes/t/$teamSlug/sessions/$sessionId_.plan.$messageId.tsx; rail: session-thread-view.tsx PlanRail`,
    },
    desktop: { state: `leftover`, note: `#EXP-1497: the desktop child of #EXP-1493 lands it` },
    ios: {
      state: `ok`,
      symbol: `PlanPageScreen`,
      file: `apps/ios/Exponential/UI/Plan/PlanPageScreen.swift`,
      note: `parts: ExpUI PlanPage.swift (StoryKindBadge, PlanPictureCarousel, PlanOutlineList); specimen: ChatSpecimens StorySpecimensScreen \`plan-page\``,
    },
    android: {
      state: `ok`,
      symbol: `PlanPageScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/plan/PlanPage.kt`,
      note: `parts: StoryKindBadge (components/StoryRing.kt), PlanPictureCarousel, PlanOutlineList; specimen: styleguide/PlanPageSpecimen.kt`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <PlanPageSpecimen />
      <PlanPageSpecimen size="phone" />
    </div>
  ),
}
