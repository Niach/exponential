import { PlanPageSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1493: THE plan page (fixture `plan-page.json` ×4): the expanded form
// of THE plan card, wide and phone, drawn from `plan-page.json` `specimen`.

export const entry: StyleguideEntry = {
  id: `plan-page`,
  section: `special`,
  owner: `EXP-1493`,
  title: `Plan page`,
  blurb: `#EXP-1662 THE plan tab (fixture \`plan-page.json\` ×4; Exponential-specific, not genUI): a chat's plan is a tab of its strip; selecting it replaces only the content, on the chat's own ground, with no page header. Wide (web md+, the IDE): THE Outline (the h2 sections with their picture counts, scroll-spy, the Questions row) + the document (a big title, THE markdown in a 720 px column, h2 anchors, two or more adjacent pictures as THE Mosaic). Phones: the same document, picture groups as THE Carousel (i / n, dots, swipe), ☰ opens the outline in a sheet. #FEED-191: the questions = wide, THE rail on the right (360 px, the sidebar background, a hairline on its left, sticky full height): the Questions heading with the step label trailing, THE stepper, one question at a time, Back / Next, the decision pinned at its foot; phones, a panel floating over the document's foot, hugging its content up to 40% of the body. In the Conversation a plan is ONE row, \`Plan: title\`, that opens the tab. The strip chip shows the newest waiting plan, else the plan shown during this visit of the chat (a decided plan leaves the strip once the person leaves); an approved plan's chip leads with THE emerald check. Any picture opens THE lightbox over every picture of the plan.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PlanPage`,
      file: `packages/ui/src/plan-page.tsx`,
      note: `host: chat-screen.tsx ChatPlanBody; rail + phone panel: session-chat-view.tsx PlanRail, the deep link route redirects to the tab`,
    },
    desktop: {
      state: `ok`,
      symbol: `plan_page`,
      file: `apps/desktop/crates/ui/src/plan_page.rs`,
      note: `parts: outline.rs, mosaic.rs, image_preview.rs; twin: styleguide/entries/plan_page.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `PlanPageScreen`,
      file: `apps/ios/Exponential/UI/Plan/PlanPageScreen.swift`,
      note: `parts: ExpUI PlanPage.swift (PlanPictureCarousel, PlanOutlineList); specimen: ChatSpecimens \`plan-page\``,
    },
    android: {
      state: `ok`,
      symbol: `PlanPageScreen`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/plan/PlanPage.kt`,
      note: `parts: PlanPictureCarousel, PlanOutlineList; specimen: styleguide/PlanPageSpecimen.kt`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <PlanPageSpecimen />
      <PlanPageSpecimen size="phone" />
    </div>
  ),
}
