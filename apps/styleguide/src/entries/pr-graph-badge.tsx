import { Button, conceptIcon } from "@exp/ui"

import type { StyleguideEntry } from "./types.ts"

// SLOP-16 — the work header's graph badge is a quiet ICON BUTTON, not a chip:
// the stacked issue chip (EXP-1058) restated the title right beside it. The
// glyph names the badge SHAPE (`lib/pr-graph.ts` `badgeShape`, ×4,
// face-independent since EXP-1097), a muted mono `+N` counts the rest
// (`badgeChip`). Click opens THE "Related work" view (SLOP-16 r3): the
// standard modal, group bands over rows the product already draws. The
// specimen shows the button only, since the view reads four Electric
// collections.

const StackIcon = conceptIcon(`pr-stack`)
const BatchIcon = conceptIcon(`pr-batch`)
const TreeIcon = conceptIcon(`session-tree`)
const BlockedIcon = conceptIcon(`relation-blocked-by`)

function BadgeButton({
  icon: Icon,
  label,
  count,
}: {
  icon: typeof StackIcon
  label: string
  count: number
}) {
  return (
    <Button
      type="button"
      variant="ghost"
      size="icon-sm"
      aria-label={label}
      title={label}
      className={`shrink-0 text-muted-foreground hover:text-foreground${count > 0 ? ` w-auto gap-1 px-2` : ``}`}
    >
      <Icon className="size-4" />
      {count > 0 && (
        <span className="font-mono text-xs text-muted-foreground">+{count}</span>
      )}
    </Button>
  )
}

export const entry: StyleguideEntry = {
  id: `pr-graph-badge`,
  section: `special`,
  owner: `EXP-1058`,
  title: `Work header badge`,
  blurb: `What this work is PART OF, as a quiet icon button beside the \`…\` (SLOP-16; it replaced EXP-1058's stacked issue chip, which restated the title). The glyph follows the badge SHAPE: a pull request stack (or stack + batch) = \`pr-stack\`, a batch = \`pr-batch\`, a run family = \`session-tree\`, open blockers = \`relation-blocked-by\`; a muted mono \`+N\` beside it counts every other issue on the stack or batch, every other run of the family, or the other open blockers (nothing when N is 0). FACE-INDEPENDENT (EXP-1097): the same button on Issue, Run and Changes; absent when there is nothing to say. The tooltip names the shape. Click opens THE "Related work" view, ONE layout on every platform (SLOP-16 r3): the platform's standard modal (web: the \`dialog\` at its default width, which drops to its bottom-sheet arm on a phone), one GROUP BAND (\`section-header\`, never folding) per section in \`overlaySections\` order, the face's own first, over rows the product already draws: "Blocked by" over the COMPACT mini-graph (small chips, scrolling sideways); "In batch with" (Issue, Changes) / "Issues" (Run) over the relations card's issue rows; "Runs" over the session tree's run rows; "Pull requests" over the Reviews queue's stack row, bottom-up with tree guides, \`Merge stack\` on the bottom row. A batch's issues appear ONCE: under the batch band, or folded under their PR row when that band is absent. No new layout, no captions, no cards.`,
  status: {
    web: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/web/src/components/pr-graph-badge.tsx`,
      note: `icon button per lib/pr-graph.ts badgeShape + badgeChip count; Related work dialog/sheet: bands over the existing rows`,
    },
    desktop: {
      state: `ok`,
      symbol: `pr_graph::badge`,
      file: `apps/desktop/crates/ui/src/pr_graph.rs`,
      note: `icon button per domain pr_graph::badge_shape + badge_chip count; Related work dialog: bands over the existing rows`,
    },
    ios: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/ios/Exponential/UI/Work/PrGraphBadge.swift`,
      note: `icon button per ExpCore PrGraph.badgeShape + badgeChip count; Related work sheet: bands over the existing rows`,
    },
    android: {
      state: `ok`,
      symbol: `PrGraphBadge`,
      file: `apps/android/app/src/main/java/com/exponential/app/ui/work/PrGraphBadge.kt`,
      note: `icon button per domain PrGraph.badgeShape + badgeChip count; Related work sheet: bands over the existing rows`,
    },
  },
  island: () => (
    <div className="flex flex-wrap items-center gap-6 p-2">
      <BadgeButton icon={StackIcon} label="Pull request stack" count={2} />
      <BadgeButton icon={BatchIcon} label="Batch pull request" count={1} />
      <BadgeButton icon={TreeIcon} label="The runs around this one" count={0} />
      <BadgeButton icon={BlockedIcon} label="Blocked by" count={0} />
    </div>
  ),
}
