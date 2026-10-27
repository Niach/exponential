import gettingStarted from "@exp/domain-contract/fixtures/getting-started.json"
import { GettingStartedList } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1270 (EXP-1259 D6, `getting-started.json`, the #EXP-1270 redesign):
// the real `@exp/ui` `GettingStartedList` the web app mounts, fed the
// fixture's own expected states: a fresh team (Create a board expanded),
// mid-path (four done, Invite your team expanded) and the phone's six in a
// phone-wide frame (taller rows, Docs as an icon button).

const [fresh, fourDone, , , phone] = gettingStarted.cases

function List({
  state,
  phone = false,
}: {
  state: (typeof gettingStarted.cases)[number][`expected`]
  phone?: boolean
}) {
  return (
    <GettingStartedList
      entries={state.entries}
      done={state.done}
      total={state.total}
      progress={state.progress}
      phone={phone}
    />
  )
}

export const entry: StyleguideEntry = {
  id: `getting-started`,
  section: `special`,
  owner: `EXP-1270`,
  title: `Getting started`,
  blurb: `ONE block ×4: the group band (the title, the muted "${gettingStarted.progress}", a ${gettingStarted.meter}px meter, Docs) over flat hairline rows in the fixture's order (Create a board, Connect GitHub, Add a device, Start a session, Invite your team, Set up a server, Connect your tools, Feedback widget). Done rows are muted with a green check and stay clickable; the ONE next row is expanded (the active fill, a small "${gettingStarted.nextLabel}" label, one sentence of why and one primary button); later rows are plain with a quiet chevron. No numbers, no locks (Start a session without a device opens Add a device). Rows are at least ${gettingStarted.row.height}px (phones ${gettingStarted.row.heightPhone}px), the block at most ${gettingStarted.maxWidth}px wide, scrolling vertically only. Shown on an empty board, on the Agent home while the team has no board, and behind the sidebar entry in a ${gettingStarted.maxWidth}px dialog (phones: a bottom sheet, Docs as an icon button); gone once every step is done. Members never see Invite or the widget; the phones drop the two web-managed steps.`,
  status: {
    web: {
      state: `ok`,
      symbol: `GettingStartedList / GettingStartedSection`,
      file: `packages/ui/src/getting-started-list.tsx`,
      note: `wired in apps/web components/getting-started/getting-started-section.tsx`,
    },
    desktop: {
      state: `ok`,
      symbol: `derive_getting_started / inline_list`,
      file: `apps/desktop/crates/ui/src/getting_started.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `GettingStartedList`,
      file: `apps/ios/Exponential/UI/GettingStarted/GettingStartedList.swift`,
    },
    android: {
      state: `ok`,
      symbol: `GettingStartedList`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/gettingstarted/GettingStartedList.kt`,
    },
  },
  island: () => (
    <div className="grid gap-8">
      <List state={fresh!.expected} />
      <List state={fourDone!.expected} />
      <div className="w-[390px] max-w-full overflow-hidden rounded-xl border border-glass-stroke">
        <List state={phone!.expected} phone />
      </div>
    </div>
  ),
}
