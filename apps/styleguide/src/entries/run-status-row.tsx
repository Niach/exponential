import { RunStatusRow } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1175: the Run face's status row — the REAL `RunStatusRow` in its
// three everyday states, plus (EXP-1245) a settled turn's row in the owner's
// thread of turns, which carries no toggle. The words come from the app's `runRowCaption` /
// `lastToolLine` (fixture `run-row.json` ×4); the island passes them literal.

export const entry: StyleguideEntry = {
  id: `run-status-row`,
  section: `special`,
  owner: `EXP-1175`,
  title: `Run status row`,
  blurb: `The Run face opens as the THREAD, not the transcript: one status row over the run's published results. The row = the run mark as the spinner, the caption in its tone (\`Building on <device> · <elapsed>\` while it works, \`Needs input\` amber, \`Ready for review\` emerald, \`Done\` sky, \`Paused\`, \`Ended on <device> · <elapsed>\`), the last tool line muted under it (live only) and \`Show work\` on the right. Show work swaps the thread for the full transcript IN PLACE (same scroller, strips and composer) and is remembered per user. Pending questions and plans never hide. EXP-1245: in the owner's thread of turns every turn draws its own row (\`turnRowCaption\`: \`Done on <device> · <turn time>\` once settled, the open turn timed from its own start). Label + state only (fixture \`run-row.json\` \`captions\` + \`turnCaptions\`).`,
  status: {
    web: {
      state: `ok`,
      symbol: `RunStatusRow`,
      file: `packages/ui/src/run-status-row.tsx`,
      note: `rules: lib/work-faces.ts runRowCaption + lib/agent-feed.ts lastToolLine, fixture run-row.json`,
    },
    desktop: {
      state: `ok`,
      symbol: `steer_viewer::render_status_row`,
      file: `apps/desktop/crates/ui/src/steer_viewer.rs`,
      note: `styleguide/entries/run_status_row.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `RunStatusRow`,
      file: `apps/ios/Exponential/UI/Session/RunStatusRow.swift`,
    },
    android: {
      state: `ok`,
      symbol: `RunStatusRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/session/RunStatusRow.kt`,
    },
  },
  island: () => (
    <div className="flex flex-col divide-y divide-border">
      <RunStatusRow
        agent="claude"
        markState="working"
        caption="Building on MacBook Pro · 4m 12s"
        tone="muted"
        toolLine="Read apps/web/src/lib/work-faces.ts"
        showWork={false}
        toggleLabel="Show work"
        onToggle={() => {}}
      />
      <RunStatusRow
        agent="claude"
        markState="needs_input"
        caption="Needs input · MacBook Pro"
        tone="amber"
        toolLine={null}
        showWork={false}
        toggleLabel="Show work"
        onToggle={() => {}}
      />
      <RunStatusRow
        agent="codex"
        markState="ended"
        caption="Ended on MacBook Pro · 18m"
        tone="muted"
        toolLine={null}
        showWork
        toggleLabel="Hide work"
        onToggle={() => {}}
      />
      <RunStatusRow
        agent="claude"
        markState="ended"
        caption="Done on macbook · 7m 17s"
        tone="muted"
      />
    </div>
  ),
}
