import { PrRowSpecimen } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1248: THE PR row, the REAL `@exp/ui` PrRow in its two shapes: a PR TREE
// (sub-session PRs based on their parent's branch) nests with tree guides; a
// linear STACK hangs off one rail down to its base branch, the current member
// washed, and a hovered member offers the ghost "Merge through here" (pinned
// on one row here). EXP-1293: one row per `pull_requests` row (the stack edge
// = `base_pull_request_id`); a chat's Guide draws them.

export const entry: StyleguideEntry = {
  id: `pr-row`,
  section: `special`,
  owner: `EXP-1248`,
  title: `PR row`,
  blurb: `ONE line per pull request ×4: [ring lead · mono identifier · title · quiet word]. The lead is the StatusGlyph ring: emerald ring = an open PR, filled centre = the current member, muted ring = the base branch. NO branch line, NO PR number, NO counts, NO age, NO inline Merge. A PR TREE (any fork) nests with tree guides; a linear STACK renders top-first on a 1px rail down to a mono base-branch row, "stack" on its top row, the current member washed; hovering a member offers a ghost "Merge through here" (phones: the row's long-press menu). 36px rows, 40px on phones. Fixture \`list-item.json\`.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PrRow / PrList / StackRail / PrNode`,
      file: `packages/ui/src/pr-row.tsx`,
      note: `a chat's Guide draws it, one row per pull_requests row`,
    },
    desktop: {
      state: `ok`,
      symbol: `pr_rows::pr_row / pr_list / stack_rail`,
      file: `apps/desktop/crates/ui/src/pr_rows.rs`,
      note: `the Guide stack card and the Related-work Stack band`,
    },
    ios: {
      state: `ok`,
      symbol: `PrRow / StackRail`,
      file: `apps/ios/ExpUI/Sources/PrRow.swift`,
      note: `the Guide stack card`,
    },
    android: {
      state: `ok`,
      symbol: `PrRow / PrList`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/components/PrRow.kt`,
      note: `the Guide stack card`,
    },
  },
  island: () => <PrRowSpecimen full />,
}
