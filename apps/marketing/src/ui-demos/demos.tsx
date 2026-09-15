/* ─── The docs' UI islands — the demo map (EXP-887) ───

   Every entry is ONE real `@exp/ui` component tree. `scripts/ui-demos.tsx`
   renders each to static markup inside a declarative shadow root and compiles
   the package stylesheet with THIS directory as the Tailwind scan base — so
   the wrapper classes written here are emitted alongside the components' own.

   Why a shadow root: marketing has no Tailwind and hand-writes tokens of the
   same names as the web theme (`--border`, `--input`, `--accent`), so the web
   CSS must never reach this document. It rides inside the island instead.

   Keep every demo a RESTING state: `renderToStaticMarkup` has no portals, no
   hover and no open menus, and the handlers below exist only so the removable
   variant renders its ✕ (see `island.ts` for the full list of limits).

   Statuses come from the package's own resolution table rather than literal
   icon names and colour classes, so a docs chip can never drift from the one
   the four clients draw. */

import type { ReactElement } from "react"
import {
  BUILTIN_STATUS_COLOR_CLASS,
  ChipRemoveButton,
  IssueChip,
  Pill,
  categoryStatusIcon,
  getActionIcon,
  type StatusGlyphProps,
} from "@exp/ui"

/** The builtin In Progress / In Review pair — the 2/4 and 3/4 pie clocks. */
const IN_PROGRESS: StatusGlyphProps = {
  icon: categoryStatusIcon(`started`, 0, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_progress,
}
const IN_REVIEW: StatusGlyphProps = {
  icon: categoryStatusIcon(`started`, 1, 2),
  colorClass: BUILTIN_STATUS_COLOR_CLASS.in_review,
}

/** The "Fix merge conflicts" builtin's curated glyph. */
const ActionIcon = getActionIcon({ icon: `git-branch` })

/** Static markup: nothing here is ever clicked. */
const noop = () => {}

export const UI_DEMOS_SOURCE: Record<string, () => ReactElement> = {
  /* /docs/issues/ — what a `#EXP-885` reference becomes once it resolves. */
  "issue-chip": () => (
    <div className="flex flex-wrap items-center gap-2">
      <IssueChip
        identifier="EXP-885"
        title="One issue chip, every surface"
        status={IN_REVIEW}
      />
      <IssueChip
        identifier="EXP-887"
        title="Real components in the docs"
        status={IN_PROGRESS}
      />
    </div>
  ),

  /* /docs/coding/ — the Agent composer's subject row: issue chips, each with
     the ✕ that is the only thing that drops one, or the single action chip. */
  "subject-chips": () => (
    <div className="flex flex-wrap items-center gap-2">
      <IssueChip
        identifier="EXP-885"
        title="One issue chip, every surface"
        status={IN_REVIEW}
        onRemove={noop}
        removeLabel="Remove EXP-885"
      />
      <IssueChip
        identifier="EXP-887"
        title="Real components in the docs"
        status={IN_PROGRESS}
        onRemove={noop}
        removeLabel="Remove EXP-887"
      />
      <Pill
        size="sm"
        mode="readonly"
        className="max-w-[20rem] pr-1"
        leading={<ActionIcon className="size-3 shrink-0" />}
      >
        <span className="min-w-0 truncate">Fix merge conflicts</span>
        <ChipRemoveButton
          label="Remove Fix merge conflicts"
          onRemove={noop}
        />
      </Pill>
    </div>
  ),
}
