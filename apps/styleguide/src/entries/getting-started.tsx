import gettingStarted from "@exp/domain-contract/fixtures/getting-started.json"
import { GettingStartedStrip } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1270 (EXP-1259 D6, `getting-started.json`, draft 5): the real `@exp/ui`
// `GettingStartedStrip` the web app mounts, fed the fixture's own expected
// states: four of eight done (Invite your team next), a member's six, and the
// phone's six in a narrow frame (the row scrolls, it never wraps).

const [, fourDone, , member, phone] = gettingStarted.cases

function Strip({ state }: { state: (typeof gettingStarted.cases)[number][`expected`] }) {
  return (
    <GettingStartedStrip
      entries={state.entries}
      done={state.done}
      total={state.total}
      progress={state.progress}
    />
  )
}

export const entry: StyleguideEntry = {
  id: `getting-started`,
  section: `special`,
  owner: `EXP-1270`,
  title: `Getting started`,
  blurb: `ONE block ×4: the title, the muted "${gettingStarted.progress}", a ${gettingStarted.meter}px meter and the Docs button over ONE row of ${gettingStarted.card.width}×${gettingStarted.card.height} square cards in the fixture's order (Create a board, Connect GitHub, Add a device, Start a session, Invite your team, Set up a server, Connect your tools, Feedback widget). The next step is highlighted, done steps keep a green check and stay clickable, the rest show their number; no descriptions, no locked state, no per-card buttons (Start a session without a device opens Add a device). Narrower than its cards the row scrolls sideways; phones get the same strip. Shown on an empty board, on the Agent home while the team has no board, and behind the sidebar entry in a centred dialog; gone once every step is done. Members never see Invite or the widget; the phones drop the two web-managed steps.`,
  status: {
    web: {
      state: `ok`,
      symbol: `GettingStartedStrip / GettingStartedSection`,
      file: `packages/ui/src/getting-started-strip.tsx`,
      note: `wired in apps/web components/getting-started/getting-started-section.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `getting_started`,
      file: `apps/desktop/crates/ui/src/getting_started.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `GettingStartedCards`,
      file: `apps/ios/Exponential/UI/GettingStarted/GettingStartedCards.swift`,
    },
    android: {
      state: `ok`,
      symbol: `GettingStartedCards`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/gettingstarted/GettingStartedCards.kt`,
    },
  },
  island: () => (
    <div className="grid gap-8">
      <Strip state={fourDone!.expected} />
      <Strip state={member!.expected} />
      <div className="w-[390px] max-w-full overflow-hidden rounded-xl border border-glass-stroke p-4">
        <Strip state={phone!.expected} />
      </div>
    </div>
  ),
}
