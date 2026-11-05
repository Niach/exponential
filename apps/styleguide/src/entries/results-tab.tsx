import { ResultsTabSpecimen } from "@exp/ui"
import { RESULTS_TAB } from "@exp/domain-contract/results-tab"

import type { StyleguideEntry } from "./types.ts"

// #FEED-191 RESULTS TAB (fixtures/results-tab.json `tab.specimen`): THE Results
// tab body of the session screen, wide and phone (and the app's
// `?dev=results-tab` capture).

export const entry: StyleguideEntry = {
  id: `results-tab`,
  section: `special`,
  owner: `FEED-191`,
  title: `Results tab`,
  blurb: `THE session screen's last tab (fixture \`results-tab.json\` ×4; Exponential-specific, not genUI): every result the thread's tree posted, never expiring, newest first, INLINE. A section = the title (semibold, one line) with the age muted at its end, no badge; the description muted under it; then ONE row of thumbnails (${RESULTS_TAB.tile.thumbHeight.wide}px tall, width by aspect up to ${RESULTS_TAB.tile.thumbMaxWidth.wide}px, the hairline) with the caption, else the label, under each, a press opening THE lightbox over that result's pictures; else THE surface card. Above them, while the thread has its pull request: the Stack band + THE StackRail and ONE Changes row opening the complete diff. None = THE EmptyState.`,
  status: {
    web: {
      state: `ok`,
      symbol: `ResultsTab / ResultSection`,
      file: `packages/ui/src/results-tab.tsx`,
      note: `host: apps/web/src/components/thread/thread-results.tsx (useResultsTab, PR block, review pane)`,
    },
    desktop: { state: `n/a`, note: `the desktop lane (model: domain::results_tab)` },
    ios: { state: `n/a`, note: `the native lane (model: ExpCore ResultsTab.swift)` },
    android: { state: `n/a`, note: `the native lane (model: domain/ResultsTab.kt)` },
  },
  island: () => (
    <div className="flex flex-wrap items-start gap-6">
      <ResultsTabSpecimen />
      <ResultsTabSpecimen width={360} size="phone" />
    </div>
  ),
}
