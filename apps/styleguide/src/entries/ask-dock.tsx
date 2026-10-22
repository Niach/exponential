import { AskDockSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// N-12: THE ask cards on THE Dock (fixtures ask-dock.json + dock.json ×4):
// `AskDockSpecimen`, the REAL `DockCard` / `DockPile` rows of a waiting
// plan and question and the plan's sheet (`ThreadPlanSheet` in the sheet's
// static frame); the app's `?dev=ask-dock` capture renders the same board.

export const entry: StyleguideEntry = {
  id: `ask-dock`,
  section: `special`,
  owner: `EXP-1341`,
  title: `Ask dock`,
  blurb: `N-12: a question or plan card waiting on you is a DockCard of THE Dock, floating above the composer in a session and a thread alike (dock-card entry): the kind icon, the tinted label, the asking session's name, a plan's question count, the first plain line of the text and the open chevron; several waiting cards pile with the Queue and the queued messages, the newest ask in front; a plan or question that arrives while you look opens its sheet by itself. The sheet (plan-card.json stepper): page 0 the plan (pictures, the inline surface; fewer than two questions sit under it), then one page per question with "Question n of m" in the header, Back / Next in the pinned footer, and on the last page THE decision ×4: "No, keep planning" (secondary; picked = the feedback field above the footer, Cancel + the reject as the primary) and "Yes" (the primary; its menu holds "Yes, and start with a fresh context"), disabled while a required answer is missing; a question's footer = Answer. Answered cards stay in the transcript, closed. Model = \`askDockTitle\`, \`threadPendingAsks()\` (ask-dock.json) + \`dockRows\` (dock.json) ×4.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ThreadDock / ThreadPlanSheet / ThreadQuestionSheet`,
      file: `packages/ui/src/session-thread-view.tsx`,
      note: `the primitive: dock.tsx; the session view's relay cards: agent-session.tsx`,
    },
    desktop: {
      state: `leftover`,
      symbol: `ask_dock`,
      file: `apps/desktop/crates/ui/src/ask_dock.rs`,
      note: `N-12: still the composer-slot card without a cap; the IDE child moves it onto dock.rs`,
    },
    ios: {
      state: `leftover`,
      symbol: `AskDock`,
      file: `apps/ios/Exponential/UI/Session/AskDock.swift`,
      note: `N-12: still the composer-slot card; the iOS child moves it onto Dock.swift (model: ExpCore AskDock.swift + Dock.swift)`,
    },
    android: {
      state: `leftover`,
      symbol: `AskDock`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/AskDock.kt`,
      note: `N-12: still the composer-slot card; the Android child moves it onto Dock.kt (model: domain/AskDock.kt + Dock.kt)`,
    },
  },
  island: () => <AskDockSpecimen />,
}
