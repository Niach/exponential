import { PrChecksSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1611 / #EXP-1710 (fixtures/pr-checks.json, merge-train.json): CI and
// the merge train on the thread's PR ×4, the `pr-checks` specimen
// (`packages/ui/src/pr-checks.tsx`), case for case with the desktop twin
// (`styleguide/entries/pr_checks.rs`): THE header merge STATUS pill in every
// merge-train `statusPill` case, then the `…` menu's Checks submenu drawn
// open as a static panel over pr-checks.json `specimen`.

export const entry: StyleguideEntry = {
  id: `pr-checks`,
  section: `special`,
  owner: `EXP-1611`,
  title: `PR checks`,
  blurb: `The thread header's PR carries its merge status and CI, identical on all four clients and read only from the synced rows (no client calls GitHub). THE header pill is the merge STATUS pill over \`pull_requests.merge_*\` (merge-train.json \`statusPill\`): THE Pill md, not primary, its glyph by concept and a red tone tinting the glyph only: Draft, Checks running | Session working (THE Spinner), Queued · 2nd, Testing with 3 | Testing (THE Spinner), Merged, Conflict (red pr-conflict), Checks failing (red ci-failed), On hold | Auto-merge stuck (pr-hold, red for the limits). Its tap opens the Checks list; no rows = readonly. THE ONLY action: \`approval\` mode, a ready PR = the primary \`Needs approval\`, whose tap asks the plain or stack merge confirm and queues it. The \`…\` menu (\`mergeMenuEntries\`): the Checks submenu whenever the PR has rows (any viewer; the summary glyph and \`{n} failing\` | \`{n} running\` | \`{n} passed\`, then one row per check with its tinted glyph, name and duration; a pick opens the logs, a row without a url is disabled), Merge now (owners, the shared alert card \`merge-now-pr\`), Hold | Resume auto-merge, Close PR. Model = \`statusPill\` / \`mergeMenuEntries\` / \`checksList\` / \`checksSummary\` (fixtures \`merge-train.json\`, \`pr-checks.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `PrStatusPill, prChecksMenuEntry`,
      file: `packages/ui/src/pr-checks.tsx`,
      note: `sites: session-merge-button.tsx (pill, Merge now, Hold | Resume), hooks/use-pull-request-checks.ts (the … menu rows)`,
    },
    desktop: {
      state: `ok`,
      symbol: `checks_submenu`,
      file: `apps/desktop/crates/ui/src/pr_checks.rs`,
      note: `styleguide/entries/pr_checks.rs`,
    },
    ios: {
      state: `ok`,
      symbol: `PrChecksMenuRows`,
      file: `apps/ios/Exponential/UI/Work/PrChecksMenu.swift`,
    },
    android: {
      state: `ok`,
      symbol: `MergeStatusPill`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/MergeControl.kt`,
    },
  },
  island: () => <PrChecksSpecimen />,
}
