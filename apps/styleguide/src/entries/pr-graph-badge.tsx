import {
  BUILTIN_STATUS_COLOR_CLASS,
  CHIP_GLYPH_CLASS,
  categoryStatusIcon,
  ChipBox,
  conceptIcon,
  IssueChip,
  IssueChipStack,
} from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// EXP-1058 — the work header's graph badge IS the stacked issue chip: the
// representative issue in front, `+N` for the rest of the stack / batch / run
// family behind it (`lib/pr-graph.ts` `badgeChip`, ×4). Hover (pointer) or tap
// (phones) opens the same overlay as before; the specimen shows the chip only,
// since the overlay reads four Electric collections.

const TreeIcon = conceptIcon(`session-tree`)

const startedStatus = {
  icon: categoryStatusIcon(`started`, 0, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_progress,
}

export const entry: StyleguideEntry = {
  id: `pr-graph-badge`,
  section: `special`,
  owner: `EXP-1058`,
  title: `Work header badge`,
  blurb: `What this work is PART OF, as the stacked issue chip (EXP-1058): the front chip names the subject pull request's representative issue, the ghosts behind it and \`+N\` count every other issue on its stack or batch, or every other run of its family on the Run face (a run without an issue names itself, with the session-tree glyph). Absent when there is nothing to say. Hover on pointer platforms, tap on phones: the overlay lists the face's section — Blocked by and In batch with on Issue, Runs on Run, the pull request stack bottom-up on Changes.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/web/src/components/pr-graph-badge.tsx`,
      note: `the rule is lib/pr-graph.ts badgeChip`,
    },
    desktop: {
      state: `ok`,
      symbol: `pr_graph::badge`,
      file: `apps/desktop/crates/ui/src/pr_graph.rs`,
      note: `the rule is domain pr_graph::badge_chip`,
    },
    ios: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/ios/Exponential/UI/Work/PrGraphBadge.swift`,
      note: `the rule is ExpCore PrGraph.badgeChip`,
    },
    android: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/PrGraphBadge.kt`,
      note: `the rule is domain PrGraph.badgeChip`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-center gap-6 p-2">
      <IssueChipStack count={2}>
        <IssueChip identifier="STRA-43" title="Indexer: trades, bundles, offers" status={startedStatus} />
      </IssueChipStack>
      <IssueChipStack count={1}>
        <IssueChip identifier="STRA-45" title="Indexer query API for the web app" status={startedStatus} />
      </IssueChipStack>
      <IssueChipStack count={2}>
        <ChipBox
          slot="issue-chip"
          openLabel="The runs around this one"
          body={
            <>
              <TreeIcon className={`${CHIP_GLYPH_CLASS} text-muted-foreground`} />
              <span className="min-w-0 truncate text-[0.8125rem] font-medium text-foreground">Chat</span>
            </>
          }
        />
      </IssueChipStack>
    </div>
  ),
}
