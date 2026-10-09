import { conceptIcon, GlassSectionHeader, PrList, StackRail } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1248: THE PR row, the REAL `@exp/ui` PrRow in its two shapes: a PR TREE
// (follow-up runs based on their parent's branch) nests with tree guides; a
// linear STACK hangs off one rail down to its base branch, the current member
// washed, and a hovered member offers the ghost "Merge through here" (pinned
// on one row here). Reviews, PR trees and the Guide's stack card share it.

const ExternalIcon = conceptIcon(`ui-external-link`)

const TREE = [
  { key: `1250`, identifier: `EXP-1250`, title: `opening tabs closes all at some point` },
  { key: `1252`, identifier: `EXP-1252`, title: `tabs per team on desktop`, depth: 1 },
  { key: `1253`, identifier: `EXP-1253`, title: `inbox stepping reuses one tab`, depth: 1 },
]

const STACK = [
  { key: `100`, identifier: `VAPP-100`, title: `SwiftUI parity with the round-1/2 contract`, current: true },
  { key: `98`, identifier: `VAPP-98`, title: `Exponential UI renderer hardening round 1` },
  { key: `93`, identifier: `VAPP-93`, title: `ui.exponential.at: the Exponential UI site` },
  { key: `91`, identifier: `VAPP-91`, title: `Exponential UI host API ×4` },
]

function PrRowSpecimen() {
  return (
    <div className="flex w-[640px] max-w-full flex-col gap-4">
      <div>
        <GlassSectionHeader label="Exponential" />
        <PrList rows={TREE.map((row) => ({ ...row, onOpen: () => {} }))} />
        <StackRail
          members={STACK}
          baseBranch="master"
          word="stack"
          onOpen={() => {}}
          onMergeThrough={() => {}}
          defaultHoveredKey="93"
        />
      </div>
      <div>
        <GlassSectionHeader label="Not linked to an issue" />
        <PrList
          rows={[
            {
              key: `1013`,
              identifier: `#1013`,
              title: `Dockerfiles: stub the exponential-ui workspace packages`,
              onOpen: () => {},
              trailing: <ExternalIcon className="size-3.5 shrink-0 text-muted-foreground" />,
            },
          ]}
        />
      </div>
    </div>
  )
}

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
      note: `Reviews (wave B) and the Guide's stack card draw it`,
    },
    desktop: {
      state: `leftover`,
      symbol: `reviews_view row builders`,
      file: `apps/desktop/crates/ui/src/reviews_view.rs`,
      note: `four row builders with a trailing Merge (wave B: one PrRow)`,
    },
    ios: {
      state: `leftover`,
      symbol: `ReviewsView rows`,
      file: `apps/ios/Exponential/UI/MyWork/ReviewsView.swift`,
      note: `branch line + Merge pill (wave B: one PrRow)`,
    },
    android: {
      state: `leftover`,
      symbol: `ReviewPrRow`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/reviews/ReviewPrRow.kt`,
      note: `branch line + Merge pill (wave B: one PrRow)`,
    },
  },
  island: () => <PrRowSpecimen />,
}
