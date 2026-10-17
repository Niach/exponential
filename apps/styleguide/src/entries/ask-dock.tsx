import { AskDockSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// THE ask dock (fixture `ask-dock.json` ×4): `AskDockSpecimen`, the REAL
// `ThreadAskDock` in its three states (the app's `?dev=ask-dock` capture
// renders the same board).

export const entry: StyleguideEntry = {
  id: `ask-dock`,
  section: `special`,
  owner: `EXP-1341`,
  title: `Ask dock`,
  blurb: `The question and plan cards waiting on you leave the transcript and take the COMPOSER's slot, in a session and a thread alike (a thread with one waiting card looks exactly like a session). One card = THE big card (\`AskCard\` + \`AskPrompt\`), its body capped at 40% of the viewport and scrolling inside. Several (a thread's child sessions) = compact AskCards stacked 6px apart, oldest first: the kind icon, the tinted label, the asking session's name, the plan's first plain line and a chevron; a tap grows one into the big card (height over motion \`standard\`, the content fading in), only that card answers, and the back arrow leading its header returns to the stack. THE plan options ×4: Yes · Yes, and start with a fresh context · No, keep planning (opens the note field). Answered cards stay in the transcript, closed. Model = \`askDock()\`, \`threadPendingAsks()\` (fixture \`ask-dock.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `AskDock`,
      file: `packages/ui/src/ask-dock.tsx`,
      note: `thread: ThreadAskDock (session-thread-view.tsx); session view: agent-session.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `ask_dock`,
      file: `apps/desktop/crates/ui/src/ask_dock.rs`,
      note: `styleguide/entries/ask_dock.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `AskDock`,
      file: `apps/ios/ExpUI/Sources/AskDock.swift`,
      note: `model: ExpCore AskDock.swift`,
    },
    android: {
      state: `ok`,
      symbol: `AskDock`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/AskDock.kt`,
      note: `model: domain/AskDock.kt`,
    },
  },
  island: () => <AskDockSpecimen />,
}
