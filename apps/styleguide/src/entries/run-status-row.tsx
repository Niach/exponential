import { RunStatusRow } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1175 → EXP-1293: the session view's status row — the REAL
// `RunStatusRow` over a session's raw work, in its everyday states. The words
// are the session card's state words (`sessionCard`, fixture
// `session-card.json`) and the newest tool line (`lastToolLine`); no Show
// work toggle (the session view always shows the work).

export const entry: StyleguideEntry = {
  id: `run-status-row`,
  section: `special`,
  owner: `EXP-1175`,
  title: `Run status row`,
  blurb: `The status row over a session's raw work (the session view, opened from its session card): the session mark as the spinner, the session card's state word in its tone (\`Working\`, \`Needs you\` amber, \`Usage limit\` amber, \`Done\`), the last tool line muted under it (live only), and before the first turn THE mark over \`Starting on <device>\`. EXP-1293: no Show work toggle, the view always shows the work.`,
  status: {
    web: {
      state: `ok`,
      symbol: `RunStatusRow`,
      file: `packages/ui/src/run-status-row.tsx`,
      note: `components/agent-session.tsx: state = sessionState (lib/session-tree) + lib/agent-feed.ts lastToolLine`,
    },
    desktop: {
      state: `ok`,
      symbol: `steer_viewer::session_status_row`,
      file: `apps/desktop/crates/ui/src/steer_viewer.rs`,
      note: `styleguide/entries/run_status_row.rs`,
    },
    ios: { state: `n/a`, note: `EXP-1291 removed RunStatusRow.swift; the session header's AgentRunMark carries the state` },
    android: { state: `n/a`, note: `N-11: no status row over the feed; the session header's AgentRunMark carries the state` },
  },
  island: () => (
    <div className="flex flex-col divide-y divide-border">
      <RunStatusRow
        agent="claude"
        starting
        caption="Starting on MacBook Pro"
        tone="muted"
      />
      <RunStatusRow
        agent="claude"
        markState="working"
        caption="Working"
        tone="muted"
        toolLine="Read data/sync/BoardSnapshotCache.kt"
      />
      <RunStatusRow
        agent="claude"
        markState="needs_input"
        caption="Needs you"
        tone="amber"
        toolLine="Bash ./gradlew :app:startupProfile --variant=release"
      />
      <RunStatusRow agent="codex" markState="ended" caption="Done" tone="muted" />
    </div>
  ),
}
