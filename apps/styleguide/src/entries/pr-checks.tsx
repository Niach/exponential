import { PrChecksSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// #EXP-1611 (fixtures/pr-checks.json): CI on the thread's PR ×4, the
// `pr-checks` specimen (`packages/ui/src/pr-checks.tsx`), case for case with
// the desktop twin (`styleguide/entries/pr_checks.rs`): THE header Merge pill
// in its four check states, then the `…` menu's Checks submenu drawn open as
// a static panel over the fixture's `specimen`.

export const entry: StyleguideEntry = {
  id: `pr-checks`,
  section: `special`,
  owner: `EXP-1611`,
  title: `PR checks`,
  blurb: `The thread header's PR carries its CI, identical on all four clients and read only from the synced \`pull_request_checks\` rows (no client calls GitHub). THE Merge pill over \`pull_requests.checks_state\` (\`mergePill\`): none and success = the primary \`Merge PR\` | \`Merge stack\` with the pr-merged glyph; pending = the same pill with THE Spinner, still enabled; failure = the plain pill \`Checks failing\` with the red ci-failed glyph, whose tap asks prompts.json \`merge-failing-pr\` (Cancel focused, Merge destructive, body = the failing names, five then +N) instead of the plain or stack confirm. The \`…\` menu leads with the Checks submenu whenever the PR has rows (any viewer): the summary glyph and \`{n} failing\` | \`{n} running\` | \`{n} passed\`, then one row per check (failed, running, queued, passed, skipped; ties by name) with its tinted glyph, name and duration; a pick opens the logs, a row without a url is disabled. Model = \`checksList\` / \`checksSummary\` (fixture \`pr-checks.json\` ×4).`,
  status: {
    web: {
      state: `ok`,
      symbol: `PrMergePill, prChecksMenuEntry`,
      file: `packages/ui/src/pr-checks.tsx`,
      note: `sites: session-merge-button.tsx (pill), hooks/use-pull-request-checks.ts (the … menu rows)`,
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
