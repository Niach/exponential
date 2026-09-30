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
// family behind it, or the first open blocker (`lib/pr-graph.ts` `badgeChip`,
// ×4, face-independent since EXP-1097). Hover (pointer) or tap
// (phones) opens the same overlay as before; the specimen shows the chip only,
// since the overlay reads four Electric collections.
//
// SLOP-15: the ghosts became faint outlines (2px steps, no fill) and the
// phone's compact chip is the chip's own `size="sm"`. SLOP-16 brought the
// three natives along: outline ghosts, the small chip, the compact hover
// graph (`compactNodeWidth`, fixture-locked ×4).

const TreeIcon = conceptIcon(`session-tree`)

const startedStatus = {
  icon: categoryStatusIcon(`started`, 0, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_progress,
}

const reviewStatus = {
  icon: categoryStatusIcon(`started`, 1, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_review,
}

export const entry: StyleguideEntry = {
  id: `pr-graph-badge`,
  section: `special`,
  owner: `EXP-1058`,
  title: `Work header badge`,
  blurb: `What this work is PART OF, as the stacked issue chip (EXP-1058): the front chip names the subject pull request's representative issue, the ghosts behind it and \`+N\` BESIDE them count every other issue on its stack or batch, or every other run of its family (a run without an issue names itself, with the session-tree glyph). SLOP-15: the ghosts are SUBTLE — the chip's outline alone, no fill, in 2px steps up and to the right, the far one at half strength — so the stack reads as one chip with depth, never as three boxes jostling in the header. EXP-1097: the chip is FACE-INDEPENDENT — the same on Issue, Run and Changes: a stack/batch first, then a run family, then open blockers (the first open blocker in front, the others counted); absent when there is nothing to say. Phones draw the chip's SMALL mode (\`IssueChip size="sm"\`: glyph · identifier, no title) beside the \`…\`. Hover on pointer platforms, tap on phones: the overlay lists every section the work has, the face's own first — Blocked by on Issue (the mini-graph in compact boxes), Runs on Run, the pull request stack bottom-up on Changes — every issue in it as the small chip, its title in the tooltip and the preview card.`,
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
      note: `the rule is domain pr_graph::badge_chip; SLOP-16: outline ghosts, small chips in the overlay, compact hover graph`,
    },
    ios: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/ios/Exponential/UI/Work/PrGraphBadge.swift`,
      note: `the rule is ExpCore PrGraph.badgeChip; SLOP-16: outline ghosts, IssueChip size .sm, compact hover graph`,
    },
    android: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/PrGraphBadge.kt`,
      note: `the rule is domain PrGraph.badgeChip; SLOP-16: outline ghosts, IssueChipSize.Sm, compact hover graph`,
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
      {/* EXP-1097: open blockers alone — the first one in front. */}
      <IssueChipStack count={2}>
        <IssueChip identifier="EXP-1121" title="Coding readiness model + copy" status={reviewStatus} />
      </IssueChipStack>
      {/* EXP-1097: the phone header's compact chip — SLOP-15: the chip's own
          small mode, nothing hand-rolled. */}
      <IssueChipStack count={2}>
        <IssueChip identifier="EXP-1121" title="Coding readiness model + copy" status={reviewStatus} size="sm" />
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
